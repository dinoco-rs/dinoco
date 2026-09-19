use dinoco::*;

#[dinoco(migration)]
pub struct CreateUsers;

impl DinocoMigration for CreateUsers {
    async fn up(&self, manager: &DinocoManager) -> Result<(), DbErr> {
        manager
            .create_table(CreateTableMigration {
                table: "user".to_string(),
                if_not_exists: false,
                columns: vec![
                    MigrationColumn::new("id", MigrationColumnType::String).primary_key(),
                    MigrationColumn::new("name", MigrationColumnType::String),
                ],
                foreign_keys: Vec::new(),
            })
            .await
    }

    async fn down(&self, manager: &DinocoManager) -> Result<(), DbErr> {
        manager.drop_table(DropTableMigration { table: "user".to_string(), if_exists: false }).await
    }
}

#[dinoco(migration)]
pub struct AddUserEmail;

impl DinocoMigration for AddUserEmail {
    async fn up(&self, manager: &DinocoManager) -> Result<(), DbErr> {
        manager
            .add_column(AddColumnMigration {
                table: "user".to_string(),
                column: MigrationColumn::new("email", MigrationColumnType::String).nullable(),
            })
            .await?;
        manager
            .create_index(CreateIndexMigration {
                table: "user".to_string(),
                index: MigrationIndex {
                    name: "user_email_idx".to_string(),
                    columns: vec!["email".to_string()],
                    automatic: false,
                    kind: MigrationIndexKind::Standard,
                },
            })
            .await
    }

    async fn down(&self, manager: &DinocoManager) -> Result<(), DbErr> {
        manager
            .drop_index(DropIndexMigration {
                table: "user".to_string(),
                index: MigrationIndex {
                    name: "user_email_idx".to_string(),
                    columns: vec!["email".to_string()],
                    automatic: false,
                    kind: MigrationIndexKind::Standard,
                },
            })
            .await?;
        manager.drop_column(DropColumnMigration { table: "user".to_string(), column: "email".to_string() }).await
    }
}

/// Uses raw SQL and fails on purpose.
#[dinoco(migration)]
pub struct Broken;

impl DinocoMigration for Broken {
    async fn up(&self, manager: &DinocoManager) -> Result<(), DbErr> {
        manager.execute("INSERT INTO user (id, name) VALUES ('1', 'ok')").await?;
        manager.execute("SELECT * FROM table_that_does_not_exist").await
    }

    async fn down(&self, _manager: &DinocoManager) -> Result<(), DbErr> {
        Ok(())
    }
}

fn registry() -> Vec<MigrationEntry> {
    vec![
        MigrationEntry::new("20260101000000_create_users", CreateUsers),
        MigrationEntry::new("20260102000000_add_user_email", AddUserEmail),
    ]
}

async fn client() -> (tempfile::TempDir, DinocoClient) {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("manual.db").to_string_lossy().to_string();
    let adapter = <SqliteAdapter as DinocoAdapter>::new(path).await.expect("sqlite");

    (directory, DinocoClient::new(Backend::Sqlite(adapter)))
}

async fn columns(client: &DinocoClient) -> Vec<String> {
    let Backend::Sqlite(adapter) = &client.backend else { unreachable!() };
    let connection = adapter.pool.get().await.expect("connection");
    connection
        .interact(|connection| -> rusqlite::Result<Vec<String>> {
            let mut statement = connection.prepare("SELECT name FROM pragma_table_info('user')")?;
            statement.query_map([], |row| row.get::<_, String>(0))?.collect()
        })
        .await
        .expect("interact")
        .expect("columns")
}

#[tokio::test]
async fn up_applies_pending_migrations_in_order_and_is_idempotent() -> anyhow::Result<()> {
    let (_directory, client) = client().await;
    let entries = registry();

    let before = migration_status(&client, &entries).await?;
    assert!(before.iter().all(|item| !item.applied));

    let ran = migrate_up(&client, &entries).await?;
    assert_eq!(ran, ["20260101000000_create_users", "20260102000000_add_user_email"]);
    assert_eq!(columns(&client).await, ["id", "name", "email"]);

    assert!(migrate_up(&client, &entries).await?.is_empty(), "applied migrations are skipped");
    assert!(migration_status(&client, &entries).await?.iter().all(|item| item.applied));

    Ok(())
}

#[tokio::test]
async fn down_reverts_the_newest_applied_migrations_first() -> anyhow::Result<()> {
    let (_directory, client) = client().await;
    let entries = registry();
    migrate_up(&client, &entries).await?;

    let reverted = migrate_down(&client, &entries, 1).await?;
    assert_eq!(reverted, ["20260102000000_add_user_email"]);
    assert_eq!(columns(&client).await, ["id", "name"]);
    let status = migration_status(&client, &entries).await?;
    assert_eq!(status.iter().map(|item| item.applied).collect::<Vec<_>>(), [true, false]);

    let reverted = migrate_down(&client, &entries, 5).await?;
    assert_eq!(reverted, ["20260101000000_create_users"], "reverting more than applied stops at the oldest");
    assert!(columns(&client).await.is_empty());
    assert!(migrate_down(&client, &entries, 1).await?.is_empty());

    // Reverted migrations can be applied again.
    assert_eq!(migrate_up(&client, &entries).await?.len(), 2);

    Ok(())
}

#[tokio::test]
async fn a_failing_migration_is_not_recorded_and_earlier_ones_stay_applied() -> anyhow::Result<()> {
    let (_directory, client) = client().await;
    let entries = vec![
        MigrationEntry::new("20260101000000_create_users", CreateUsers),
        MigrationEntry::new("20260103000000_broken", Broken),
    ];

    let error = migrate_up(&client, &entries).await.expect_err("second migration fails");
    assert!(format!("{error:#}").contains("20260103000000_broken"), "{error:#}");

    let status = migration_status(&client, &entries).await?;
    assert_eq!(status.iter().map(|item| item.applied).collect::<Vec<_>>(), [true, false]);

    Ok(())
}

#[tokio::test]
async fn registry_mismatches_are_reported() -> anyhow::Result<()> {
    let (_directory, client) = client().await;
    migrate_up(&client, &registry()).await?;

    let removed = vec![MigrationEntry::new("20260101000000_create_users", CreateUsers)];
    let error = migration_status(&client, &removed).await.expect_err("applied migration was unregistered");
    assert!(error.to_string().contains("20260102000000_add_user_email"), "{error}");

    let duplicated = vec![
        MigrationEntry::new("20260101000000_create_users", CreateUsers),
        MigrationEntry::new("20260101000000_create_users", CreateUsers),
    ];
    let error = migrate_up(&client, &duplicated).await.expect_err("duplicate names");
    assert!(error.to_string().contains("more than once"), "{error}");

    Ok(())
}

#[tokio::test]
async fn manual_history_does_not_touch_the_automatic_history_table() -> anyhow::Result<()> {
    let (_directory, client) = client().await;
    migrate_up(&client, &registry()).await?;

    let Backend::Sqlite(adapter) = &client.backend else { unreachable!() };
    let connection = adapter.pool.get().await?;
    let tables = connection
        .interact(|connection| -> rusqlite::Result<Vec<String>> {
            let mut statement =
                connection.prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")?;
            statement.query_map([], |row| row.get::<_, String>(0))?.collect()
        })
        .await
        .expect("interact")?;

    assert!(tables.contains(&MANUAL_MIGRATIONS_TABLE.to_string()));
    assert!(!tables.contains(&"dinoco_migrations".to_string()));

    Ok(())
}

#[tokio::test]
async fn operations_sqlite_cannot_perform_fail_instead_of_silently_doing_nothing() -> anyhow::Result<()> {
    let (_directory, client) = client().await;
    let manager = DinocoManager::new(client.backend.clone());
    manager.execute("CREATE TABLE user (id TEXT PRIMARY KEY NOT NULL)").await?;

    let error = manager
        .add_foreign_key(AddForeignKeyMigration {
            table: "user".to_string(),
            foreign_key: MigrationForeignKey {
                name: "user_self_fkey".to_string(),
                columns: vec!["id".to_string()],
                references_table: "user".to_string(),
                references_columns: vec!["id".to_string()],
                on_update: ReferentialAction::Cascade,
                on_delete: ReferentialAction::Cascade,
            },
        })
        .await
        .expect_err("SQLite cannot add a foreign key after creation");
    assert!(error.to_string().contains("not supported"), "{error}");
    assert!(error.to_string().contains("user_self_fkey"), "{error}");

    // Enums are inline column types on SQLite, so there is nothing to run.
    manager.create_enum(CreateEnumMigration { name: "role".to_string(), values: vec!["admin".to_string()] }).await?;

    // Renames go through the same manager.
    manager
        .rename_column(RenameColumnMigration {
            table: "user".to_string(),
            from: "id".to_string(),
            to: "uid".to_string(),
        })
        .await?;
    manager.rename_table(RenameTableMigration { from: "user".to_string(), to: "account".to_string() }).await?;
    manager.execute("INSERT INTO account (uid) VALUES ('1')").await?;

    Ok(())
}
