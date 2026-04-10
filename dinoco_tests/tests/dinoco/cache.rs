use std::sync::Arc;
use std::sync::Mutex;

use dinoco::{
    CachePolicy, CachedFindFirst, CachedFindMany, DinocoAdapter, DinocoCache, DinocoClient, DinocoClientConfig,
    DinocoQueryLogWriter, DinocoQueryLogger, DinocoQueryLoggerOptions, DinocoRedisConfig, DinocoResult, DinocoValue,
    InsertModel, Model, Projection, Rowable, ScalarField, find_first, find_many, insert_many,
};
use dinoco_engine::{MySqlAdapter, PostgresAdapter, SqliteAdapter};
use uuid::Uuid;

mod common;

const TABLE_NAME: &str = "dinoco_cache_records";

#[derive(Clone)]
struct MemoryWriter {
    logs: Arc<Mutex<Vec<String>>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Rowable, dinoco::serde::Serialize, dinoco::serde::Deserialize)]
#[serde(crate = "dinoco::serde")]
struct CacheRecord {
    id: i64,
    name: String,
}

struct CacheRecordInclude {}

struct CacheRecordWhere {
    id: ScalarField<i64>,
    name: ScalarField<String>,
}

fn count_selects(messages: &[String]) -> usize {
    messages.iter().filter(|message| message.contains("SELECT") && message.contains(TABLE_NAME)).count()
}

fn count_cache_hits(messages: &[String]) -> usize {
    messages.iter().filter(|message| message.contains("CACHE HIT key=")).count()
}

fn new_logger() -> (MemoryWriter, DinocoQueryLogger) {
    let writer = MemoryWriter { logs: Arc::new(Mutex::new(Vec::new())) };
    let logger = DinocoQueryLogger::custom(writer.clone(), DinocoQueryLoggerOptions::compact());

    (writer, logger)
}

impl MemoryWriter {
    fn entries(&self) -> Vec<String> {
        self.logs.lock().expect("memory writer should lock").clone()
    }
}

impl DinocoQueryLogWriter for MemoryWriter {
    fn write(&self, message: &str) {
        self.logs.lock().expect("memory writer should lock").push(message.to_string());
    }
}

impl Projection<CacheRecord> for CacheRecord {
    fn columns() -> &'static [&'static str] {
        &["id", "name"]
    }
}

impl InsertModel for CacheRecord {
    fn insert_columns() -> &'static [&'static str] {
        &["id", "name"]
    }

    fn into_insert_row(self) -> Vec<DinocoValue> {
        vec![self.id.into(), self.name.into()]
    }

    fn insert_identity_conditions(&self) -> Vec<dinoco::Expression> {
        vec![dinoco::Expression::Column("id".to_string()).eq(self.id)]
    }
}

impl Model for CacheRecord {
    type Include = CacheRecordInclude;
    type Where = CacheRecordWhere;

    fn table_name() -> &'static str {
        TABLE_NAME
    }
}

impl Default for CacheRecordInclude {
    fn default() -> Self {
        Self {}
    }
}

impl Default for CacheRecordWhere {
    fn default() -> Self {
        Self { id: ScalarField::new("id"), name: ScalarField::new("name") }
    }
}

#[tokio::test]
async fn sqlite_cache_queries_and_client_cache_methods_work_end_to_end() -> DinocoResult<()> {
    let (writer, logger) = new_logger();
    let client = DinocoClient::<SqliteAdapter>::new(
        common::sqlite_url("cache"),
        vec![],
        DinocoClientConfig::default()
            .with_query_logger(logger)
            .with_redis(DinocoRedisConfig::from_url(common::redis_url())),
    )
    .await?;

    client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{TABLE_NAME}""#), &[]).await?;
    create_sqlite_table(&client).await?;
    exercise_cache_flow(&client, &writer, &format!(r#"UPDATE "{TABLE_NAME}" SET "name" = 'Ana Maria' WHERE "id" = 1"#))
        .await?;
    client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{TABLE_NAME}""#), &[]).await?;

    Ok(())
}

#[tokio::test]
async fn postgres_cache_queries_and_client_cache_methods_work_end_to_end() -> DinocoResult<()> {
    if let Err(err) = async {
        let _lock = common::lock_postgres().await;
        let (writer, logger) = new_logger();
        let client = DinocoClient::<PostgresAdapter>::new(
            common::postgres_url(),
            vec![],
            DinocoClientConfig::default()
                .with_query_logger(logger)
                .with_redis(DinocoRedisConfig::from_url(common::redis_url())),
        )
        .await?;

        client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{TABLE_NAME}""#), &[]).await?;
        create_postgres_table(&client).await?;
        exercise_cache_flow(
            &client,
            &writer,
            &format!(r#"UPDATE "{TABLE_NAME}" SET "name" = 'Ana Maria' WHERE "id" = 1"#),
        )
        .await?;
        client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{TABLE_NAME}""#), &[]).await?;

        Ok(())
    }
    .await
    {
        if common::should_skip_external_adapter_test(&err) {
            eprintln!("skipping postgres cache adapter test: {err}");
            return Ok(());
        }

        return Err(err);
    }

    Ok(())
}

#[tokio::test]
async fn mysql_cache_queries_and_client_cache_methods_work_end_to_end() -> DinocoResult<()> {
    if let Err(err) = async {
        let _lock = common::lock_mysql().await;
        let (writer, logger) = new_logger();
        let client = DinocoClient::<MySqlAdapter>::new(
            common::mysql_url(),
            vec![],
            DinocoClientConfig::default()
                .with_query_logger(logger)
                .with_redis(DinocoRedisConfig::from_url(common::redis_url())),
        )
        .await?;

        client.primary().execute(&format!("DROP TABLE IF EXISTS `{TABLE_NAME}`"), &[]).await?;
        create_mysql_table(&client).await?;
        exercise_cache_flow(
            &client,
            &writer,
            &format!("UPDATE `{TABLE_NAME}` SET `name` = 'Ana Maria' WHERE `id` = 1"),
        )
        .await?;
        client.primary().execute(&format!("DROP TABLE IF EXISTS `{TABLE_NAME}`"), &[]).await?;

        Ok(())
    }
    .await
    {
        if common::should_skip_external_adapter_test(&err) {
            eprintln!("skipping mysql cache adapter test: {err}");
            return Ok(());
        }

        return Err(err);
    }

    Ok(())
}

async fn create_mysql_table(client: &DinocoClient<MySqlAdapter>) -> DinocoResult<()> {
    client
        .primary()
        .execute(
            &format!(
                "CREATE TABLE `{TABLE_NAME}` (\
                    `id` BIGINT PRIMARY KEY,\
                    `name` VARCHAR(255) NOT NULL\
                )"
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
                    "name" TEXT NOT NULL
                )"#
            ),
            &[],
        )
        .await
}

async fn create_sqlite_table(client: &DinocoClient<SqliteAdapter>) -> DinocoResult<()> {
    client
        .primary()
        .execute(
            &format!(
                r#"CREATE TABLE "{TABLE_NAME}" (
                    "id" INTEGER PRIMARY KEY,
                    "name" TEXT NOT NULL
                )"#
            ),
            &[],
        )
        .await
}

async fn exercise_cache_flow<A>(
    client: &DinocoClient<A>,
    writer: &MemoryWriter,
    update_record_sql: &str,
) -> DinocoResult<()>
where
    A: DinocoAdapter,
{
    let cache = DinocoCache::new(client);
    let manual_key = format!("dinoco:test:manual:{}", Uuid::now_v7());
    let many_key = format!("dinoco:test:many:{}", Uuid::now_v7());
    let first_key = format!("dinoco:test:first:{}", Uuid::now_v7());

    insert_many::<CacheRecord>()
        .values(vec![CacheRecord { id: 1, name: "Ana".to_string() }, CacheRecord { id: 2, name: "Bia".to_string() }])
        .execute(client)
        .await?;

    cache.set(&manual_key, &vec!["alpha".to_string(), "beta".to_string()]).await?;
    assert_eq!(cache.get::<Vec<String>>(&manual_key).await?, Some(vec!["alpha".to_string(), "beta".to_string()]));

    cache.set_with_ttl(&manual_key, &vec!["ttl".to_string()], 60).await?;
    assert_eq!(cache.get::<Vec<String>>(&manual_key).await?, Some(vec!["ttl".to_string()]));

    cache.delete(&manual_key).await?;
    assert_eq!(cache.get::<Vec<String>>(&manual_key).await?, None);

    let initial_selects = count_selects(&writer.entries());
    let many = CachedFindMany::new(
        find_many::<CacheRecord>().order_by(|where_| where_.id.asc()),
        CachePolicy::new(many_key.clone()),
    )
    .execute(client)
    .await?;
    let after_first_many = count_selects(&writer.entries());

    assert_eq!(many.len(), 2);
    assert_eq!(after_first_many, initial_selects + 1);
    assert_eq!(
        cache.get::<Vec<CacheRecord>>(&many_key).await?,
        Some(vec![CacheRecord { id: 1, name: "Ana".to_string() }, CacheRecord { id: 2, name: "Bia".to_string() }])
    );

    insert_many::<CacheRecord>().values(vec![CacheRecord { id: 3, name: "Caio".to_string() }]).execute(client).await?;
    let before_cached_many = count_selects(&writer.entries());

    let cached_many = CachedFindMany::new(
        find_many::<CacheRecord>().order_by(|where_| where_.id.asc()),
        CachePolicy::new(many_key.clone()),
    )
    .execute(client)
    .await?;
    let after_second_many = count_selects(&writer.entries());

    assert_eq!(cached_many.len(), 2);
    assert_eq!(after_second_many, before_cached_many);
    assert_eq!(count_cache_hits(&writer.entries()), 1);

    cache.delete(&many_key).await?;

    let fresh_many = CachedFindMany::new(
        find_many::<CacheRecord>().order_by(|where_| where_.id.asc()),
        CachePolicy::with_ttl(many_key.clone(), 60),
    )
    .execute(client)
    .await?;
    let after_third_many = count_selects(&writer.entries());

    assert_eq!(fresh_many.len(), 3);
    assert_eq!(after_third_many, after_second_many + 1);

    let first = CachedFindFirst::new(
        find_first::<CacheRecord>().cond(|where_| where_.id.eq(1)),
        CachePolicy::with_ttl(first_key.clone(), 60),
    )
    .execute(client)
    .await?;

    assert_eq!(first, Some(CacheRecord { id: 1, name: "Ana".to_string() }));
    assert_eq!(
        cache.get::<Option<CacheRecord>>(&first_key).await?,
        Some(Some(CacheRecord { id: 1, name: "Ana".to_string() }))
    );

    client.primary().execute(update_record_sql, &[]).await?;

    let cached_first = CachedFindFirst::new(
        find_first::<CacheRecord>().cond(|where_| where_.id.eq(1)),
        CachePolicy::new(first_key.clone()),
    )
    .execute(client)
    .await?;

    assert_eq!(cached_first, Some(CacheRecord { id: 1, name: "Ana".to_string() }));
    assert_eq!(count_cache_hits(&writer.entries()), 2);

    cache.delete(&first_key).await?;

    let refreshed_first =
        CachedFindFirst::new(find_first::<CacheRecord>().cond(|where_| where_.id.eq(1)), CachePolicy::new(first_key))
            .execute(client)
            .await?;

    assert_eq!(refreshed_first, Some(CacheRecord { id: 1, name: "Ana Maria".to_string() }));

    Ok(())
}
