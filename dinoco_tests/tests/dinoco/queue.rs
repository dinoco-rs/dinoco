mod common;

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use dinoco::{
    DinocoAdapter, DinocoCache, DinocoClient, DinocoClientConfig, DinocoError, DinocoGenericRow, DinocoRedisConfig,
    DinocoResult, DinocoRow, Extend, FindAndUpdateModel, IncludeLoaderFuture, Insert, InsertModel, InsertPayload,
    IntoDinocoValue, Model, MySqlAdapter, PostgresAdapter, Projection, QueueWorkers, RelationField, Rowable,
    ScalarField, SqliteAdapter, Update, UpdateField, UpdateModel, delete, find_and_update, insert_into, update,
};
use dinoco_engine::QueryBuilder;

use tokio::sync::{Mutex, MutexGuard};

use std::sync::Arc;

static QUEUE_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
const TABLE_NAME: &str = "users";

#[derive(Debug, Clone, Rowable)]
struct User {
    id: i64,
    name: String,
}

#[derive(Debug, Clone, Rowable)]
struct Post {
    id: i64,
    title: String,
    authorId: i64,
}

struct UserWhere {
    id: ScalarField<i64>,
    name: ScalarField<String>,
}

struct PostWhere {
    id: ScalarField<i64>,
    title: ScalarField<String>,
    authorId: ScalarField<i64>,
}

#[derive(Default)]
struct UserInclude {}

#[derive(Default)]
struct PostInclude {}

#[derive(Debug, Clone, Extend)]
#[extend(Post)]
struct PostListItem {
    id: i64,
    title: String,
}

#[derive(Debug, Clone, Extend)]
#[extend(User)]
struct UserWithPosts {
    id: i64,
    name: String,
    posts: Vec<PostListItem>,
}

struct UserUpdate {
    name: UpdateField<String>,
}

impl Default for UserWhere {
    fn default() -> Self {
        Self { id: ScalarField::new("id"), name: ScalarField::new("name") }
    }
}

impl Default for UserUpdate {
    fn default() -> Self {
        Self { name: UpdateField::new("name") }
    }
}

impl Default for PostWhere {
    fn default() -> Self {
        Self { id: ScalarField::new("id"), title: ScalarField::new("title"), authorId: ScalarField::new("authorId") }
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

impl Projection<Post> for Post {
    fn columns() -> &'static [&'static str] {
        &["id", "title", "authorId"]
    }
}

impl Model for Post {
    type Include = PostInclude;
    type Where = PostWhere;

    fn table_name() -> &'static str {
        "posts"
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

impl FindAndUpdateModel for User {
    type Update = UserUpdate;

    fn primary_key_columns() -> &'static [&'static str] {
        &["id"]
    }
}

impl UserInclude {
    fn posts(&self) -> RelationField<Post> {
        RelationField::new("posts")
    }
}

impl User {
    pub fn __dinoco_load_posts<'a, P, C, A>(
        item_keys: Vec<Option<i64>>,
        include: &'a dinoco::IncludeNode,
        client: &'a DinocoClient<A>,
        read_mode: dinoco::ReadMode,
        relation_field: impl Fn(&mut P) -> &mut Vec<C> + Copy + Send + 'a,
    ) -> IncludeLoaderFuture<'a, P>
    where
        A: DinocoAdapter,
        C: Projection<Post> + Clone,
    {
        Box::pin(async move {
            struct ChildRow<C> {
                item: C,
                relation_key: i64,
            }

            impl<C> DinocoRow for ChildRow<C>
            where
                C: Projection<Post>,
            {
                fn from_row<R: DinocoGenericRow>(row: &R) -> DinocoResult<Self> {
                    Ok(Self { item: C::from_row(row)?, relation_key: row.get(C::columns().len())? })
                }
            }

            let keys = item_keys.iter().flatten().copied().collect::<Vec<_>>();

            if keys.is_empty() {
                return Ok(Box::new(|_: &mut [P]| {}) as dinoco::IncludeApplier<'a, P>);
            }

            let adapter = client.read_adapter(matches!(read_mode, dinoco::ReadMode::Primary));
            let mut statement = include
                .statement
                .clone()
                .unwrap_or_else(|| dinoco_engine::SelectStatement::new().from("posts").select(C::columns()));

            if statement.select.is_empty() {
                statement.select = C::columns().iter().map(|column| format!("posts.{column}")).collect();
            }

            statement.select.push("posts.authorId".to_string());
            statement.conditions.push(
                dinoco::Expression::Column("posts.authorId".to_string())
                    .in_values(keys.iter().copied().map(IntoDinocoValue::into_dinoco_value).collect()),
            );

            let (sql, params) = adapter.dialect().build_select(&statement);
            let child_rows = adapter.query_as::<ChildRow<C>>(&sql, &params).await?;
            let relation_keys = child_rows.iter().map(|row| row.relation_key).collect::<Vec<_>>();
            let mut children = child_rows.into_iter().map(|row| row.item).collect::<Vec<_>>();

            C::load_includes(&mut children, &include.includes, client, read_mode).await?;

            let mut grouped: HashMap<i64, Vec<C>> = HashMap::new();

            for (relation_key, child) in relation_keys.into_iter().zip(children.into_iter()) {
                grouped.entry(relation_key).or_default().push(child);
            }

            Ok(Box::new(move |items: &mut [P]| {
                for (item, key) in items.iter_mut().zip(item_keys.iter().copied()) {
                    *relation_field(item) = key.and_then(|key| grouped.remove(&key)).unwrap_or_default();
                }
            }) as dinoco::IncludeApplier<'a, P>)
        })
    }
}

trait InsertQueueExt<M, V>
where
    M: InsertModel,
    V: InsertPayload<M>,
{
    fn enqueue(self, event: impl Into<String>) -> Insert<M, V>;
}

impl<M, V> InsertQueueExt<M, V> for Insert<M, V>
where
    M: InsertModel,
    V: InsertPayload<M>,
{
    fn enqueue(self, event: impl Into<String>) -> Insert<M, V> {
        self.__enqueue(event)
    }
}

trait UpdateQueueExt<M>
where
    M: UpdateModel,
{
    fn enqueue_in(self, event: impl Into<String>, delay_ms: u64) -> Update<M>;
}

impl<M> UpdateQueueExt<M> for Update<M>
where
    M: UpdateModel,
{
    fn enqueue_in(self, event: impl Into<String>, delay_ms: u64) -> Update<M> {
        self.__enqueue_in(event, delay_ms)
    }
}

trait FindAndUpdateQueueExt<M>
where
    M: FindAndUpdateModel,
{
    fn enqueue(self, event: impl Into<String>) -> dinoco::FindAndUpdate<M>;
}

impl<M> FindAndUpdateQueueExt<M> for dinoco::FindAndUpdate<M>
where
    M: FindAndUpdateModel,
{
    fn enqueue(self, event: impl Into<String>) -> dinoco::FindAndUpdate<M> {
        self.__enqueue(event)
    }
}

fn workers<A>() -> QueueWorkers<A>
where
    A: DinocoAdapter + Clone + Send + Sync + 'static,
{
    QueueWorkers::new()
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

    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(received.lock().await.is_empty());

    wait_for_len(&received, 1).await?;
    assert_eq!(received.lock().await.as_slice(), &["Depois".to_string()]);
    worker.abort();

    Ok(())
}

#[tokio::test]
async fn enqueue_worker_can_load_relation_with_on_with_relation() -> DinocoResult<()> {
    let _lock = lock_queue_tests().await;
    let client = match queue_client("queue-relation").await {
        Ok(client) => client,
        Err(error) if common::should_skip_external_adapter_test(&error) => return Ok(()),
        Err(error) => return Err(error),
    };

    client
        .primary()
        .execute(
            r#"INSERT INTO posts ("id", "title", "authorId") VALUES (101, 'Post A', 21), (102, 'Post B', 21)"#,
            &[],
        )
        .await?;

    let received = Arc::new(Mutex::new(Vec::new()));
    let event = format!("user.related.{}", dinoco::Uuid::now_v7());

    insert_into::<User>()
        .values(User { id: 21, name: "Matheus".to_string() })
        .enqueue(event.clone())
        .execute(&client)
        .await?;

    let handled = received.clone();
    let worker = workers::<SqliteAdapter>()
        .on_with_relation::<User, UserWithPosts, _, _, _, _>(
            event,
            |user| user.posts().select::<PostListItem>(),
            move |job| {
                let handled = handled.clone();

                async move {
                    let name = job.data.name.clone();
                    let titles = job.data.posts.iter().map(|post| post.title.clone()).collect::<Vec<_>>();

                    handled.lock().await.push((name, titles));
                    job.success()
                }
            },
        )
        .run()
        .await?;

    wait_for_len(&received, 1).await?;

    let received = received.lock().await;
    assert_eq!(received[0].0, "Matheus");
    assert_eq!(received[0].1, vec!["Post A".to_string(), "Post B".to_string()]);
    worker.abort();

    Ok(())
}

#[tokio::test]
async fn find_and_update_can_enqueue_worker_job() -> DinocoResult<()> {
    let _lock = lock_queue_tests().await;
    let client = match queue_client("queue-find-and-update").await {
        Ok(client) => client,
        Err(error) if common::should_skip_external_adapter_test(&error) => return Ok(()),
        Err(error) => return Err(error),
    };

    insert_into::<User>().values(User { id: 31, name: "Antes".to_string() }).execute(&client).await?;

    let received = Arc::new(Mutex::new(Vec::new()));
    let event = format!("user.find-and-update.{}", dinoco::Uuid::now_v7());

    find_and_update::<User>()
        .cond(|x| x.id.eq(31_i64))
        .update(|x| x.name.set("Depois"))
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

            CREATE TABLE posts (
                id INTEGER PRIMARY KEY,
                title TEXT NOT NULL,
                authorId INTEGER NOT NULL
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
