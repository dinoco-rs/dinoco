mod common;

use std::sync::OnceLock;
use std::time::Duration;

use dinoco::{
    DinocoAdapter, DinocoCache, DinocoClient, DinocoClientConfig, DinocoError, DinocoRedisConfig, DinocoResult, Model,
    MySqlAdapter, PostgresAdapter, Projection, Rowable, ScalarField, SqliteAdapter, UpdateModel, delete, insert_into,
    update, workers,
};

use tokio::sync::{Mutex, MutexGuard};

use std::sync::Arc;

static QUEUE_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
const TABLE_NAME: &str = "users";

#[derive(Debug, Clone, Rowable)]
struct User {
    id: i64,
    name: String,
}

struct UserWhere {
    id: ScalarField<i64>,
    name: ScalarField<String>,
}

#[derive(Default)]
struct UserInclude {}

impl Default for UserWhere {
    fn default() -> Self {
        Self { id: ScalarField::new("id"), name: ScalarField::new("name") }
    }
}

impl Model for User {
    type Include = UserInclude;
    type Where = UserWhere;

    fn table_name() -> &'static str {
        "users"
    }
}

impl Projection<User> for User {
    fn columns() -> &'static [&'static str] {
        &["id", "name"]
    }
}

impl dinoco::InsertModel for User {
    fn insert_columns() -> &'static [&'static str] {
        &["id", "name"]
    }

    fn into_insert_row(self) -> Vec<dinoco::DinocoValue> {
        vec![self.id.into(), self.name.into()]
    }

    fn insert_identity_conditions(&self) -> Vec<dinoco::Expression> {
        vec![dinoco::Expression::Column("id".to_string()).eq(self.id)]
    }
}

impl UpdateModel for User {
    fn update_columns() -> &'static [&'static str] {
        &["id", "name"]
    }

    fn into_update_row(self) -> Vec<dinoco::DinocoValue> {
        vec![self.id.into(), self.name.into()]
    }

    fn update_identity_conditions(&self) -> Vec<dinoco::Expression> {
        vec![dinoco::Expression::Column("id".to_string()).eq(self.id)]
    }
}

#[tokio::test]
async fn enqueue_executes_worker_with_rehydrated_record() -> DinocoResult<()> {
    let _lock = lock_queue_tests().await;
    let client = match queue_client("queue-created").await {
        Ok(client) => client,
        Err(error) if common::should_skip_external_adapter_test(&error) => return Ok(()),
        Err(error) => return Err(error),
    };

    let received = Arc::new(Mutex::new(Vec::new()));
    let event = format!("user.created.{}", dinoco::Uuid::now_v7());

    insert_into::<User>()
        .values(User { id: 1, name: "Matheus".to_string() })
        .enqueue(event.clone())
        .execute(&client)
        .await?;

    let handled = received.clone();
    let worker = workers::<SqliteAdapter>()
        .on::<User, _, _>(event, move |job| {
            let handled = handled.clone();

            async move {
                handled.lock().await.push(job.data.name.clone());
                job.success()
            }
        })
        .run()
        .await?;

    wait_for_len(&received, 1).await?;
    assert_eq!(received.lock().await.as_slice(), &["Matheus".to_string()]);
    worker.abort();

    Ok(())
}

#[tokio::test]
async fn enqueue_skips_handler_when_record_no_longer_exists() -> DinocoResult<()> {
    let _lock = lock_queue_tests().await;
    let client = match queue_client("queue-missing").await {
        Ok(client) => client,
        Err(error) if common::should_skip_external_adapter_test(&error) => return Ok(()),
        Err(error) => return Err(error),
    };

    let received = Arc::new(Mutex::new(Vec::new()));
    let event = format!("user.missing.{}", dinoco::Uuid::now_v7());

    insert_into::<User>()
        .values(User { id: 2, name: "Delete Me".to_string() })
        .enqueue(event.clone())
        .execute(&client)
        .await?;

    delete::<User>().cond(|x| x.id.eq(2_i64)).execute(&client).await?;

    let handled = received.clone();
    let worker = workers::<SqliteAdapter>()
        .on::<User, _, _>(event, move |job| {
            let handled = handled.clone();

            async move {
                handled.lock().await.push(job.data.id);
                job.success()
            }
        })
        .run()
        .await?;

    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(received.lock().await.is_empty());
    worker.abort();

    Ok(())
}

#[tokio::test]
async fn enqueue_in_waits_until_delay_is_reached() -> DinocoResult<()> {
    let _lock = lock_queue_tests().await;
    let client = match queue_client("queue-delay").await {
        Ok(client) => client,
        Err(error) if common::should_skip_external_adapter_test(&error) => return Ok(()),
        Err(error) => return Err(error),
    };

    let received = Arc::new(Mutex::new(Vec::new()));
    let event = format!("user.updated.{}", dinoco::Uuid::now_v7());

    insert_into::<User>().values(User { id: 3, name: "Antes".to_string() }).execute(&client).await?;

    update::<User>()
        .cond(|x| x.id.eq(3_i64))
        .values(User { id: 3, name: "Depois".to_string() })
        .enqueue_in(event.clone(), 60)
        .execute(&client)
        .await?;

    let handled = received.clone();
    let worker = workers::<SqliteAdapter>().on::<User, _, _>(event, move |job| {
        let handled = handled.clone();

        async move {
            handled.lock().await.push(job.data.name.clone());
            job.success()
        }
    })
    .run()
    .await?;

    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(received.lock().await.is_empty());

    wait_for_len(&received, 1).await?;
    assert_eq!(received.lock().await.as_slice(), &["Depois".to_string()]);
    worker.abort();

    Ok(())
}

#[tokio::test]
async fn postgres_queue_behaviour_matches_sqlite() -> DinocoResult<()> {
    let _lock = lock_queue_tests().await;

    if let Err(err) = async {
        let _adapter_lock = common::lock_postgres().await;
        let client = postgres_queue_client().await?;

        exercise_queue_created_flow(&client).await?;
        exercise_queue_missing_flow(&client).await?;
        exercise_queue_delayed_flow(&client).await?;
        drop_queue_table_postgres(&client).await?;

        Ok(())
    }
    .await
    {
        if common::should_skip_external_adapter_test(&err) {
            eprintln!("skipping postgres queue adapter test: {err}");
            return Ok(());
        }

        return Err(err);
    }

    Ok(())
}

#[tokio::test]
async fn mysql_queue_behaviour_matches_sqlite() -> DinocoResult<()> {
    let _lock = lock_queue_tests().await;

    if let Err(err) = async {
        let _adapter_lock = common::lock_mysql().await;
        let client = mysql_queue_client().await?;

        exercise_queue_created_flow(&client).await?;
        exercise_queue_missing_flow(&client).await?;
        exercise_queue_delayed_flow(&client).await?;
        drop_queue_table_mysql(&client).await?;

        Ok(())
    }
    .await
    {
        if common::should_skip_external_adapter_test(&err) {
            eprintln!("skipping mysql queue adapter test: {err}");
            return Ok(());
        }

        return Err(err);
    }

    Ok(())
}

async fn exercise_queue_created_flow<A>(client: &DinocoClient<A>) -> DinocoResult<()>
where
    A: DinocoAdapter + Clone + Send + Sync + 'static,
{
    let received = Arc::new(Mutex::new(Vec::new()));
    let event = format!("user.created.{}", dinoco::Uuid::now_v7());

    insert_into::<User>()
        .values(User { id: 11, name: "Matheus".to_string() })
        .enqueue(event.clone())
        .execute(client)
        .await?;

    let handled = received.clone();
    let worker = workers::<A>()
        .on::<User, _, _>(event, move |job| {
            let handled = handled.clone();

            async move {
                handled.lock().await.push(job.data.name.clone());
                job.success()
            }
        })
        .run()
        .await?;

    wait_for_len(&received, 1).await?;
    assert_eq!(received.lock().await.as_slice(), &["Matheus".to_string()]);
    worker.abort();

    delete::<User>().cond(|x| x.id.eq(11_i64)).execute(client).await?;

    Ok(())
}

async fn exercise_queue_missing_flow<A>(client: &DinocoClient<A>) -> DinocoResult<()>
where
    A: DinocoAdapter + Clone + Send + Sync + 'static,
{
    let received = Arc::new(Mutex::new(Vec::new()));
    let event = format!("user.missing.{}", dinoco::Uuid::now_v7());

    insert_into::<User>()
        .values(User { id: 12, name: "Delete Me".to_string() })
        .enqueue(event.clone())
        .execute(client)
        .await?;

    delete::<User>().cond(|x| x.id.eq(12_i64)).execute(client).await?;

    let handled = received.clone();
    let worker = workers::<A>()
        .on::<User, _, _>(event, move |job| {
            let handled = handled.clone();

            async move {
                handled.lock().await.push(job.data.id);
                job.success()
            }
        })
        .run()
        .await?;

    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(received.lock().await.is_empty());
    worker.abort();

    Ok(())
}

async fn exercise_queue_delayed_flow<A>(client: &DinocoClient<A>) -> DinocoResult<()>
where
    A: DinocoAdapter + Clone + Send + Sync + 'static,
{
    let received = Arc::new(Mutex::new(Vec::new()));
    let event = format!("user.updated.{}", dinoco::Uuid::now_v7());

    insert_into::<User>().values(User { id: 13, name: "Antes".to_string() }).execute(client).await?;

    update::<User>()
        .cond(|x| x.id.eq(13_i64))
        .values(User { id: 13, name: "Depois".to_string() })
        .enqueue_in(event.clone(), 60)
        .execute(client)
        .await?;

    let handled = received.clone();
    let worker = workers::<A>().on::<User, _, _>(event, move |job| {
        let handled = handled.clone();

        async move {
            handled.lock().await.push(job.data.name.clone());
            job.success()
        }
    })
    .run()
    .await?;

    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(received.lock().await.is_empty());

    wait_for_len(&received, 1).await?;
    assert_eq!(received.lock().await.as_slice(), &["Depois".to_string()]);
    worker.abort();

    delete::<User>().cond(|x| x.id.eq(13_i64)).execute(client).await?;

    Ok(())
}

async fn queue_client(name: &str) -> DinocoResult<DinocoClient<SqliteAdapter>> {
    let client = tokio::time::timeout(
        Duration::from_secs(2),
        DinocoClient::<SqliteAdapter>::new(
            common::sqlite_url(name),
            Vec::new(),
            DinocoClientConfig::default().with_redis(DinocoRedisConfig::from_url(common::redis_url())),
        ),
    )
    .await
    .map_err(|_| DinocoError::ConnectionError("Timed out while connecting queue test dependencies.".to_string()))??;

    reset_queue_state(&client).await?;
    create_queue_table_sqlite(&client).await?;

    Ok(client)
}

async fn postgres_queue_client() -> DinocoResult<DinocoClient<PostgresAdapter>> {
    let client = tokio::time::timeout(
        Duration::from_secs(2),
        DinocoClient::<PostgresAdapter>::new(
            common::postgres_url(),
            Vec::new(),
            DinocoClientConfig::default().with_redis(DinocoRedisConfig::from_url(common::redis_url())),
        ),
    )
    .await
    .map_err(|_| DinocoError::ConnectionError("Timed out while connecting queue test dependencies.".to_string()))??;

    reset_queue_state(&client).await?;
    drop_queue_table_postgres(&client).await?;
    create_queue_table_postgres(&client).await?;

    Ok(client)
}

async fn mysql_queue_client() -> DinocoResult<DinocoClient<MySqlAdapter>> {
    let client = tokio::time::timeout(
        Duration::from_secs(2),
        DinocoClient::<MySqlAdapter>::new(
            common::mysql_url(),
            Vec::new(),
            DinocoClientConfig::default().with_redis(DinocoRedisConfig::from_url(common::redis_url())),
        ),
    )
    .await
    .map_err(|_| DinocoError::ConnectionError("Timed out while connecting queue test dependencies.".to_string()))??;

    reset_queue_state(&client).await?;
    drop_queue_table_mysql(&client).await?;
    create_queue_table_mysql(&client).await?;

    Ok(client)
}

async fn create_queue_table_sqlite(client: &DinocoClient<SqliteAdapter>) -> DinocoResult<()> {
    client
        .primary()
        .execute_script(
            r#"
            CREATE TABLE users (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL
            );
            "#,
        )
        .await
}

async fn create_queue_table_postgres(client: &DinocoClient<PostgresAdapter>) -> DinocoResult<()> {
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

async fn create_queue_table_mysql(client: &DinocoClient<MySqlAdapter>) -> DinocoResult<()> {
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

async fn drop_queue_table_postgres(client: &DinocoClient<PostgresAdapter>) -> DinocoResult<()> {
    client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{TABLE_NAME}""#), &[]).await
}

async fn drop_queue_table_mysql(client: &DinocoClient<MySqlAdapter>) -> DinocoResult<()> {
    client.primary().execute(&format!("DROP TABLE IF EXISTS `{TABLE_NAME}`"), &[]).await
}

async fn reset_queue_state<A>(client: &DinocoClient<A>) -> DinocoResult<()>
where
    A: DinocoAdapter,
{
    let cache = DinocoCache::new(client);
    let _ = cache.delete("dinoco:queue:jobs").await;
    let _ = cache.delete("dinoco:queue:payloads").await;

    Ok(())
}

async fn lock_queue_tests() -> MutexGuard<'static, ()> {
    QUEUE_TEST_LOCK.get_or_init(|| Mutex::new(())).lock().await
}

async fn wait_for_len<T>(items: &Arc<Mutex<Vec<T>>>, expected_len: usize) -> DinocoResult<()> {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if items.lock().await.len() >= expected_len {
                break;
            }

            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .map_err(|_| DinocoError::ParseError("Timed out while waiting for worker processing.".to_string()))?;

    Ok(())
}
