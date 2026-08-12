use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dinoco_cli::db::CliDatabase;
use dinoco_engine::{DinocoAdapter, MySqlAdapter, PostgresAdapter};

const POSTGRES_ADMIN_URL: &str = "postgres://postgres:postgres@localhost:5432/postgres";
const MYSQL_ADMIN_URL: &str = "mysql://root:root@localhost:3306/mysql";
static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
enum ServerDatabase {
    Postgres,
    Mysql,
}

#[tokio::test]
async fn postgres_migrations_preserve_data_repair_missing_tables_and_verify_checksums() -> anyhow::Result<()> {
    exercise_server_migrations(ServerDatabase::Postgres).await
}

#[tokio::test]
async fn mysql_migrations_preserve_data_repair_missing_tables_and_verify_checksums() -> anyhow::Result<()> {
    exercise_server_migrations(ServerDatabase::Mysql).await
}

async fn exercise_server_migrations(database: ServerDatabase) -> anyhow::Result<()> {
    let adapter = match database {
        ServerDatabase::Postgres => "pg",
        ServerDatabase::Mysql => "mysql",
    };
    let suffix = format!("{}_{}_{}", std::process::id(), monotonic(), NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed));
    let database_name = format!("dinoco_{adapter}_{suffix}");
    create_database(database, &database_name).await?;
    let database_url = match database {
        ServerDatabase::Postgres => format!("postgres://postgres:postgres@localhost:5432/{database_name}"),
        ServerDatabase::Mysql => format!("mysql://root:root@localhost:3306/{database_name}"),
    };
    let project = temporary_project(adapter, &suffix);
    write_schema(&project, database);

    let outcome = async {
        let initial = run_cli(&project, &database_url, &["migrate", "generate"], &[]);
        assert_success(&initial);
        assert_eq!(migration_dirs(&project).len(), 1);

        let db = connect(database, &database_url).await?;
        db.execute("INSERT INTO account (name) VALUES ('preserved')").await?;
        db.execute("DROP TABLE business").await?;
        db.execute("DROP TABLE address").await?;
        drop(db);

        let repair =
            run_cli(&project, &database_url, &["migrate", "generate"], &[("DINOCO_CLI_CONFIRM_DRIFT", "true")]);
        assert_success(&repair);
        let repair_output = combined_output(&repair);
        assert!(repair_output.contains("Database schema drift detected"), "{repair_output}");
        assert!(repair_output.contains("Missing table `business`"), "{repair_output}");
        assert!(repair_output.contains("Missing table `address`"), "{repair_output}");
        assert_eq!(migration_dirs(&project).len(), 1, "drift repair must not invent schema history");

        let db = connect(database, &database_url).await?;
        assert_eq!(db.count("SELECT COUNT(*) FROM account WHERE name = 'preserved'").await?, 1);
        let schema = db.inspect_schema().await?;
        assert!(schema.tables.iter().any(|table| table.name == "account"));
        assert!(schema.tables.iter().any(|table| table.name == "business"));
        assert!(schema.tables.iter().any(|table| table.name == "address"));
        let metadata = db.migration_metadata().await?;
        assert!(metadata.checksums_required);
        assert!(metadata.schema_snapshots_required);
        assert_eq!(metadata.checksums.as_ref().map(|items| items.len()), Some(1));
        assert_eq!(metadata.schema_snapshots.as_ref().map(|items| items.len()), Some(1));
        drop(db);

        let stable = run_cli(&project, &database_url, &["migrate", "generate"], &[]);
        assert_success(&stable);
        assert!(combined_output(&stable).contains("No schema changes were found"));

        let migration = migration_dirs(&project).into_iter().next().expect("initial migration");
        let up_path = migration.join("up.sql");
        let mut up_sql = fs::read_to_string(&up_path)?;
        up_sql.push_str("\n-- tampered after application\n");
        fs::write(&up_path, up_sql)?;
        let tampered = run_cli(&project, &database_url, &["migrate", "run"], &[]);
        assert!(!tampered.status.success(), "{}", combined_output(&tampered));
        assert!(combined_output(&tampered).to_ascii_lowercase().contains("checksum"));

        Ok::<_, anyhow::Error>(())
    }
    .await;

    let _ = fs::remove_dir_all(&project);
    let cleanup = drop_database(database, &database_name).await;
    outcome.and(cleanup)
}

async fn connect(database: ServerDatabase, url: &str) -> anyhow::Result<CliDatabase> {
    match database {
        ServerDatabase::Postgres => Ok(CliDatabase::Postgres(PostgresAdapter::direct(url).await?)),
        ServerDatabase::Mysql => Ok(CliDatabase::Mysql(MySqlAdapter::new(url))),
    }
}

async fn create_database(database: ServerDatabase, name: &str) -> anyhow::Result<()> {
    match database {
        ServerDatabase::Postgres => {
            let admin = PostgresAdapter::direct(POSTGRES_ADMIN_URL).await?;
            admin.execute(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"), &[]).await?;
            admin.execute(&format!("CREATE DATABASE {name}"), &[]).await?;
        }
        ServerDatabase::Mysql => {
            let admin = MySqlAdapter::new(MYSQL_ADMIN_URL);
            admin.execute(&format!("DROP DATABASE IF EXISTS {name}"), &[]).await?;
            admin.execute(&format!("CREATE DATABASE {name}"), &[]).await?;
        }
    }
    Ok(())
}

async fn drop_database(database: ServerDatabase, name: &str) -> anyhow::Result<()> {
    match database {
        ServerDatabase::Postgres => {
            let admin = PostgresAdapter::direct(POSTGRES_ADMIN_URL).await?;
            admin.execute(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"), &[]).await?;
        }
        ServerDatabase::Mysql => {
            let admin = MySqlAdapter::new(MYSQL_ADMIN_URL);
            admin.execute(&format!("DROP DATABASE IF EXISTS {name}"), &[]).await?;
        }
    }
    Ok(())
}

fn write_schema(project: &Path, database: ServerDatabase) {
    fs::create_dir_all(project.join("dinoco")).expect("dinoco directory");
    let provider = match database {
        ServerDatabase::Postgres => "postgresql",
        ServerDatabase::Mysql => "mysql",
    };
    fs::write(
        project.join("dinoco/schema.dinoco"),
        format!(
            r#"
config {{
    database = "{provider}"
    database_url = env("DATABASE_URL")
}}

model Account {{
    id         Integer    @id @default(autoincrement())
    name       String
    businesses Business[]
    profile    Profile?
}}

model Business {{
    id         Integer @id @default(autoincrement())
    account_id Integer
    account    Account @relation(fields: [account_id], references: [id], onDelete: Cascade)
}}

model Profile {{
    id         Integer @id @default(autoincrement())
    account_id Integer @unique
    account    Account @relation(fields: [account_id], references: [id], onDelete: Cascade)
}}

model Address {{
    id Integer @id @default(autoincrement())
}}
"#
        ),
    )
    .expect("schema");
}

fn temporary_project(adapter: &str, suffix: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("dinoco-server-migration-{adapter}-{suffix}"));
    fs::create_dir(&path).expect("unique temporary project");
    path
}

fn run_cli(project: &Path, database_url: &str, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dinoco_cli"));
    command
        .args(args)
        .env("DATABASE_URL", database_url)
        .env("DINOCO_CLI_CONFIRM_MIGRATION", "true")
        .current_dir(project);
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("dinoco cli")
}

fn assert_success(output: &Output) {
    assert!(output.status.success(), "{}", combined_output(output));
}

fn combined_output(output: &Output) -> String {
    format!("{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

fn migration_dirs(project: &Path) -> Vec<PathBuf> {
    let mut migrations = fs::read_dir(project.join("dinoco/migrations"))
        .expect("migrations")
        .map(|entry| entry.expect("migration entry").path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    migrations.sort();
    migrations
}

fn monotonic() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_nanos()
}
