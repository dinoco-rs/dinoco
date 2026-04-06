use chrono::TimeZone;

use dinoco::{
    DateTimeUtc, DinocoAdapter, DinocoClient, DinocoResult, DinocoValue, InsertModel, IntoDinocoValue, Model,
    NaiveDate, Projection, Rowable, ScalarField, UpdateModel, Utc,
};
use dinoco::{delete, delete_many, find_first, find_many, insert_into, insert_many, update, update_many};
use dinoco_engine::{MySqlAdapter, PostgresAdapter, SqliteAdapter};

mod common;

const TABLE_NAME: &str = "dinoco_core_records";

#[derive(Debug, Clone, Rowable)]
struct Record {
    id: i64,
    name: String,
    email: String,
    active: bool,
    score: f64,
    joined_on: NaiveDate,
    created_at: DateTimeUtc<Utc>,
    note: Option<String>,
}

struct RecordWhere {
    id: ScalarField<i64>,
    name: ScalarField<String>,
    email: ScalarField<String>,
    active: ScalarField<bool>,
    score: ScalarField<f64>,
    joined_on: ScalarField<NaiveDate>,
    created_at: ScalarField<DateTimeUtc<Utc>>,
    note: ScalarField<Option<String>>,
}

struct RecordInclude {}

#[tokio::test]
async fn sqlite_crud_methods_work_with_multiple_types() -> DinocoResult<()> {
    let client = DinocoClient::<SqliteAdapter>::new(common::sqlite_url("crud-adapters"), vec![]).await?;

    client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{TABLE_NAME}""#), &[]).await?;
    create_sqlite_table(&client).await?;
    exercise_crud_flow(&client).await
}

#[tokio::test]
async fn postgres_crud_methods_work_with_multiple_types() -> DinocoResult<()> {
    let client = DinocoClient::<PostgresAdapter>::new(common::postgres_url(), vec![]).await?;

    client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{TABLE_NAME}""#), &[]).await?;
    create_postgres_table(&client).await?;
    exercise_crud_flow(&client).await?;
    client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{TABLE_NAME}""#), &[]).await?;

    Ok(())
}

#[tokio::test]
async fn mysql_crud_methods_work_with_multiple_types() -> DinocoResult<()> {
    let client = DinocoClient::<MySqlAdapter>::new(common::mysql_url(), vec![]).await?;

    client.primary().execute(&format!("DROP TABLE IF EXISTS `{TABLE_NAME}`"), &[]).await?;
    create_mysql_table(&client).await?;
    exercise_crud_flow(&client).await?;
    client.primary().execute(&format!("DROP TABLE IF EXISTS `{TABLE_NAME}`"), &[]).await?;

    Ok(())
}

async fn create_sqlite_table(client: &DinocoClient<SqliteAdapter>) -> DinocoResult<()> {
    client
        .primary()
        .execute(
            &format!(
                r#"CREATE TABLE "{TABLE_NAME}" (
                    "id" INTEGER PRIMARY KEY,
                    "name" TEXT NOT NULL,
                    "email" TEXT NOT NULL UNIQUE,
                    "active" BOOLEAN NOT NULL,
                    "score" REAL NOT NULL,
                    "joined_on" DATE NOT NULL,
                    "created_at" DATETIME NOT NULL,
                    "note" TEXT NULL
                )"#
            ),
            &[],
        )
        .await
}

async fn create_postgres_table(client: &DinocoClient<PostgresAdapter>) -> DinocoResult<()> {
    client
        .primary()
        .execute(
            &format!(
                r#"CREATE TABLE "{TABLE_NAME}" (
                    "id" BIGINT PRIMARY KEY,
                    "name" TEXT NOT NULL,
                    "email" TEXT NOT NULL UNIQUE,
                    "active" BOOLEAN NOT NULL,
                    "score" DOUBLE PRECISION NOT NULL,
                    "joined_on" DATE NOT NULL,
                    "created_at" TIMESTAMP NOT NULL,
                    "note" TEXT NULL
                )"#
            ),
            &[],
        )
        .await
}

async fn create_mysql_table(client: &DinocoClient<MySqlAdapter>) -> DinocoResult<()> {
    client
        .primary()
        .execute(
            &format!(
                "CREATE TABLE `{TABLE_NAME}` (\
                    `id` BIGINT PRIMARY KEY,\
                    `name` VARCHAR(255) NOT NULL,\
                    `email` VARCHAR(255) NOT NULL UNIQUE,\
                    `active` BOOLEAN NOT NULL,\
                    `score` DOUBLE NOT NULL,\
                    `joined_on` DATE NOT NULL,\
                    `created_at` DATETIME(6) NOT NULL,\
                    `note` TEXT NULL\
                )"
            ),
            &[],
        )
        .await
}

async fn exercise_crud_flow<A: DinocoAdapter>(client: &DinocoClient<A>) -> DinocoResult<()> {
    let first = record(
        1,
        "Matheus",
        "matheus@dinoco.dev",
        true,
        9.25,
        date(2026, 4, 1),
        datetime(2026, 4, 1, 10, 30, 15),
        Some("core".to_string()),
    );
    let second = record(2, "Ana", "ana@dinoco.dev", true, 8.5, date(2026, 4, 2), datetime(2026, 4, 2, 8, 0, 0), None);
    let third = record(
        3,
        "Caio",
        "caio@dinoco.dev",
        false,
        7.75,
        date(2026, 4, 3),
        datetime(2026, 4, 3, 18, 45, 5),
        Some("legacy".to_string()),
    );

    insert_into::<Record>().values(first.clone()).execute(client).await?;
    insert_many::<Record>().values(vec![second.clone(), third.clone()]).execute(client).await?;

    let found = find_first::<Record>()
        .cond(|x| x.email.eq("matheus@dinoco.dev"))
        .execute(client)
        .await?
        .expect("record should exist after insert");

    assert_eq!(found.id, 1);
    assert_eq!(found.joined_on, first.joined_on);
    assert_eq!(found.note.as_deref(), Some("core"));

    let active_records =
        find_many::<Record>().cond(|x| x.active.eq(true)).order_by(|x| x.score.desc()).execute(client).await?;

    assert_eq!(active_records.iter().map(|item| item.id).collect::<Vec<_>>(), vec![1, 2]);
    assert!((active_records[0].score - 9.25).abs() < f64::EPSILON);

    let without_note = find_many::<Record>().cond(|x| x.note.is_null()).execute(client).await?;

    assert_eq!(without_note.len(), 1);
    assert_eq!(without_note[0].id, 2);

    let joined_after = find_many::<Record>()
        .cond(|x| x.joined_on.gte(date(2026, 4, 2)))
        .order_by(|x| x.id.asc())
        .execute(client)
        .await?;

    assert_eq!(joined_after.iter().map(|item| item.id).collect::<Vec<_>>(), vec![2, 3]);

    update::<Record>()
        .cond(|x| x.id.eq(1_i64))
        .values(record(
            1,
            "Matheus Updated",
            "matheus-updated@dinoco.dev",
            true,
            9.75,
            date(2026, 4, 10),
            datetime(2026, 4, 10, 9, 15, 0),
            Some("updated".to_string()),
        ))
        .execute(client)
        .await?;

    update_many::<Record>()
        .cond(|x| x.score.gte(7.5_f64))
        .values(vec![
            record(
                2,
                "Ana Batch",
                "ana-batch@dinoco.dev",
                true,
                8.75,
                date(2026, 4, 12),
                datetime(2026, 4, 12, 11, 0, 0),
                Some("batch".to_string()),
            ),
            record(
                3,
                "Caio Batch",
                "caio-batch@dinoco.dev",
                false,
                8.0,
                date(2026, 4, 13),
                datetime(2026, 4, 13, 14, 20, 0),
                None,
            ),
        ])
        .execute(client)
        .await?;

    let updated = find_many::<Record>().order_by(|x| x.id.asc()).execute(client).await?;

    assert_eq!(updated[0].email, "matheus-updated@dinoco.dev");
    assert_eq!(updated[1].name, "Ana Batch");
    assert_eq!(updated[1].note.as_deref(), Some("batch"));
    assert_eq!(updated[2].created_at, datetime(2026, 4, 13, 14, 20, 0));
    assert_eq!(updated[2].note, None);

    delete::<Record>().cond(|x| x.id.eq(1_i64)).execute(client).await?;
    delete_many::<Record>().cond(|x| x.active.eq(false)).execute(client).await?;

    let remaining = find_many::<Record>().order_by(|x| x.id.asc()).execute(client).await?;

    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, 2);
    assert!(find_first::<Record>().cond(|x| x.id.eq(3_i64)).execute(client).await?.is_none());

    Ok(())
}

fn record(
    id: i64,
    name: &str,
    email: &str,
    active: bool,
    score: f64,
    joined_on: NaiveDate,
    created_at: DateTimeUtc<Utc>,
    note: Option<String>,
) -> Record {
    Record { id, name: name.to_string(), email: email.to_string(), active, score, joined_on, created_at, note }
}

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).expect("date should be valid")
}

fn datetime(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> DateTimeUtc<Utc> {
    Utc.with_ymd_and_hms(year, month, day, hour, minute, second).single().expect("datetime should be valid")
}

impl Projection<Record> for Record {
    fn columns() -> &'static [&'static str] {
        &["id", "name", "email", "active", "score", "joined_on", "created_at", "note"]
    }
}

impl InsertModel for Record {
    fn insert_columns() -> &'static [&'static str] {
        &["id", "name", "email", "active", "score", "joined_on", "created_at", "note"]
    }

    fn into_insert_row(self) -> Vec<DinocoValue> {
        vec![
            self.id.into(),
            self.name.into(),
            self.email.into(),
            self.active.into(),
            self.score.into(),
            self.joined_on.into(),
            self.created_at.into(),
            self.note.into_dinoco_value(),
        ]
    }
}

impl UpdateModel for Record {
    fn update_columns() -> &'static [&'static str] {
        &["name", "email", "active", "score", "joined_on", "created_at", "note"]
    }

    fn into_update_row(self) -> Vec<DinocoValue> {
        vec![
            self.name.into(),
            self.email.into(),
            self.active.into(),
            self.score.into(),
            self.joined_on.into(),
            self.created_at.into(),
            self.note.into_dinoco_value(),
        ]
    }

    fn update_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![dinoco_engine::Expression::Column("id".to_string()).eq(self.id)]
    }
}

impl Model for Record {
    type Include = RecordInclude;
    type Where = RecordWhere;

    fn table_name() -> &'static str {
        TABLE_NAME
    }
}

impl Default for RecordWhere {
    fn default() -> Self {
        Self {
            id: ScalarField::new("id"),
            name: ScalarField::new("name"),
            email: ScalarField::new("email"),
            active: ScalarField::new("active"),
            score: ScalarField::new("score"),
            joined_on: ScalarField::new("joined_on"),
            created_at: ScalarField::new("created_at"),
            note: ScalarField::new("note"),
        }
    }
}

impl Default for RecordInclude {
    fn default() -> Self {
        Self {}
    }
}
