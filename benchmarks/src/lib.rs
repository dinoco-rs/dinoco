//! Shared fixtures for the Dinoco benchmarks: models, per-dialect schemas,
//! seeded databases, raw-driver baselines and a counting allocator.
//!
//! Every benchmark uses only the public API, so the same code measures the
//! code base before and after a change.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use dinoco::chrono::{DateTime, TimeZone, Utc};
use dinoco::{Entity, insert_many};
use dinoco_engine::{Backend, DinocoAdapter, DinocoClient, MySqlAdapter, PostgresAdapter, SqliteAdapter};

pub const USERS: usize = 1_000;
pub const POSTS_PER_USER: usize = 5;

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "bench_user")]
pub struct BenchUser {
    #[dinoco(primary_key)]
    pub id: String,
    pub email: String,
    pub name: String,
    pub age: i64,
    pub active: bool,
    pub score: f64,
    pub bio: Option<String>,
    pub created_at: DateTime<Utc>,

    #[dinoco(one_to_many, foreign_key = "user_id", references = "id")]
    pub posts: Vec<BenchPost>,
}

#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "bench_post")]
pub struct BenchPost {
    #[dinoco(primary_key)]
    pub id: String,
    pub user_id: String,
    pub title: String,
    pub body: String,
    pub published: bool,

    #[dinoco(many_to_one, foreign_key = "user_id", references = "id")]
    pub user: Option<Box<BenchUser>>,
}

/// Write target of the insert benchmarks; truncated between runs.
#[derive(Debug, Clone, Entity)]
#[dinoco(table_name = "bench_event")]
pub struct BenchEvent {
    #[dinoco(primary_key)]
    pub id: String,
    pub kind: String,
    pub payload: String,
    pub amount: i64,
    pub ratio: f64,
    pub flagged: bool,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    Sqlite,
    Postgres,
    Mysql,
}

impl Dialect {
    pub fn name(self) -> &'static str {
        match self {
            Dialect::Sqlite => "sqlite",
            Dialect::Postgres => "postgres",
            Dialect::Mysql => "mysql",
        }
    }
}

pub fn timestamp(index: usize) -> DateTime<Utc> {
    Utc.timestamp_opt(1_700_000_000 + index as i64 * 60, 0).single().expect("valid timestamp")
}

pub fn user(index: usize) -> BenchUser {
    BenchUser {
        id: format!("u-{index:06}"),
        email: format!("user{index}@dinoco.rs"),
        name: format!("User number {index}"),
        age: 18 + (index % 60) as i64,
        active: !index.is_multiple_of(3),
        score: index as f64 * 1.5,
        bio: index.is_multiple_of(2).then(|| format!("Biography of user {index}, written for the benchmark corpus.")),
        created_at: timestamp(index),
        posts: Vec::new(),
    }
}

pub fn post(user_index: usize, index: usize) -> BenchPost {
    BenchPost {
        id: format!("p-{user_index:06}-{index:02}"),
        user_id: format!("u-{user_index:06}"),
        title: format!("Post {index} of user {user_index}"),
        body: "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor.".to_string(),
        published: index.is_multiple_of(2),
        user: None,
    }
}

pub fn event(index: usize) -> BenchEvent {
    BenchEvent {
        id: format!("e-{index:08}"),
        kind: "signup".to_string(),
        payload: format!("{{\"index\":{index},\"source\":\"benchmark\"}}"),
        amount: index as i64,
        ratio: index as f64 / 7.0,
        flagged: index.is_multiple_of(5),
        note: index.is_multiple_of(2).then(|| "even".to_string()),
        created_at: timestamp(index),
    }
}

pub fn schema(dialect: Dialect) -> Vec<&'static str> {
    match dialect {
        Dialect::Sqlite => vec![
            "DROP TABLE IF EXISTS bench_post",
            "DROP TABLE IF EXISTS bench_user",
            "DROP TABLE IF EXISTS bench_event",
            "CREATE TABLE bench_user (id TEXT PRIMARY KEY, email TEXT NOT NULL, name TEXT NOT NULL, age INTEGER NOT NULL, active BOOLEAN NOT NULL, score REAL NOT NULL, bio TEXT, created_at DATETIME NOT NULL)",
            "CREATE TABLE bench_post (id TEXT PRIMARY KEY, user_id TEXT NOT NULL REFERENCES bench_user (id), title TEXT NOT NULL, body TEXT NOT NULL, published BOOLEAN NOT NULL)",
            "CREATE INDEX bench_post_user_id ON bench_post (user_id)",
            "CREATE TABLE bench_event (id TEXT PRIMARY KEY, kind TEXT NOT NULL, payload TEXT NOT NULL, amount INTEGER NOT NULL, ratio REAL NOT NULL, flagged BOOLEAN NOT NULL, note TEXT, created_at DATETIME NOT NULL)",
        ],
        Dialect::Postgres => vec![
            "DROP TABLE IF EXISTS bench_post",
            "DROP TABLE IF EXISTS bench_user",
            "DROP TABLE IF EXISTS bench_event",
            "CREATE TABLE bench_user (id TEXT PRIMARY KEY, email TEXT NOT NULL, name TEXT NOT NULL, age BIGINT NOT NULL, active BOOLEAN NOT NULL, score DOUBLE PRECISION NOT NULL, bio TEXT, created_at TIMESTAMP NOT NULL)",
            "CREATE TABLE bench_post (id TEXT PRIMARY KEY, user_id TEXT NOT NULL REFERENCES bench_user (id), title TEXT NOT NULL, body TEXT NOT NULL, published BOOLEAN NOT NULL)",
            "CREATE INDEX bench_post_user_id ON bench_post (user_id)",
            "CREATE TABLE bench_event (id TEXT PRIMARY KEY, kind TEXT NOT NULL, payload TEXT NOT NULL, amount BIGINT NOT NULL, ratio DOUBLE PRECISION NOT NULL, flagged BOOLEAN NOT NULL, note TEXT, created_at TIMESTAMP NOT NULL)",
        ],
        Dialect::Mysql => vec![
            "DROP TABLE IF EXISTS bench_post",
            "DROP TABLE IF EXISTS bench_user",
            "DROP TABLE IF EXISTS bench_event",
            "CREATE TABLE bench_user (id VARCHAR(64) PRIMARY KEY, email VARCHAR(255) NOT NULL, name VARCHAR(255) NOT NULL, age BIGINT NOT NULL, active TINYINT(1) NOT NULL, score DOUBLE NOT NULL, bio VARCHAR(255), created_at DATETIME NOT NULL)",
            "CREATE TABLE bench_post (id VARCHAR(64) PRIMARY KEY, user_id VARCHAR(64) NOT NULL, title VARCHAR(255) NOT NULL, body VARCHAR(255) NOT NULL, published TINYINT(1) NOT NULL, INDEX bench_post_user_id (user_id), CONSTRAINT bench_post_user FOREIGN KEY (user_id) REFERENCES bench_user (id))",
            "CREATE TABLE bench_event (id VARCHAR(64) PRIMARY KEY, kind VARCHAR(64) NOT NULL, payload VARCHAR(255) NOT NULL, amount BIGINT NOT NULL, ratio DOUBLE NOT NULL, flagged TINYINT(1) NOT NULL, note VARCHAR(255), created_at DATETIME NOT NULL)",
        ],
    }
}

/// Connection URLs of the server databases. They default to the URLs the
/// `docker_adapters` integration tests use.
pub fn postgres_url() -> String {
    std::env::var("DINOCO_BENCH_POSTGRES_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/postgres".to_string())
}

pub fn mysql_url() -> String {
    std::env::var("DINOCO_BENCH_MYSQL_URL").unwrap_or_else(|_| "mysql://root:root@localhost:3306/mysql".to_string())
}

/// Every connected backend of the benchmark. Server databases that cannot be
/// reached are skipped (and reported) instead of failing the run.
pub struct Fixture {
    pub dialect: Dialect,
    pub client: DinocoClient,
    pub raw: RawDriver,
}

pub enum RawDriver {
    Sqlite(SqliteAdapter),
    Postgres(PostgresAdapter),
    Mysql(MySqlAdapter),
}

pub async fn fixtures(dialects: &[Dialect]) -> Vec<Fixture> {
    let mut fixtures = Vec::new();

    for dialect in dialects {
        match connect(*dialect).await {
            Ok(fixture) => fixtures.push(fixture),
            Err(error) => eprintln!("[bench] skipping {}: {error:#}", dialect.name()),
        }
    }

    fixtures
}

pub async fn connect(dialect: Dialect) -> anyhow::Result<Fixture> {
    let (backend, raw) = match dialect {
        Dialect::Sqlite => {
            let adapter = SqliteAdapter::memory().await?;
            (Backend::Sqlite(adapter.clone()), RawDriver::Sqlite(adapter))
        }
        Dialect::Postgres => {
            let adapter = PostgresAdapter::direct(postgres_url()).await?;
            (Backend::Postgres(adapter.clone()), RawDriver::Postgres(adapter))
        }
        Dialect::Mysql => {
            let adapter = MySqlAdapter::new(mysql_url());
            adapter.execute("SELECT 1", &[]).await?;
            (Backend::Mysql(adapter.clone()), RawDriver::Mysql(adapter))
        }
    };

    let fixture = Fixture { dialect, client: DinocoClient::new(backend), raw };
    fixture.reset().await?;
    Ok(fixture)
}

impl Fixture {
    async fn execute(&self, sql: &str) -> anyhow::Result<()> {
        match &self.raw {
            RawDriver::Sqlite(adapter) => adapter.execute(sql, &[]).await?,
            RawDriver::Postgres(adapter) => adapter.execute(sql, &[]).await?,
            RawDriver::Mysql(adapter) => adapter.execute(sql, &[]).await?,
        };
        Ok(())
    }

    /// Recreates the schema and seeds `USERS` users with `POSTS_PER_USER`
    /// posts each.
    pub async fn reset(&self) -> anyhow::Result<()> {
        for statement in schema(self.dialect) {
            self.execute(statement).await?;
        }

        for chunk in (0..USERS).collect::<Vec<_>>().chunks(250) {
            insert_many::<BenchUser>().values(chunk.iter().map(|index| user(*index))).execute(&self.client).await?;
            let posts =
                chunk.iter().flat_map(|index| (0..POSTS_PER_USER).map(move |post_index| post(*index, post_index)));
            insert_many::<BenchPost>().values(posts).execute(&self.client).await?;
        }

        Ok(())
    }

    pub async fn clear_events(&self) -> anyhow::Result<()> {
        self.execute("DELETE FROM bench_event").await
    }
}

/// The columns a raw driver query decodes, mirroring `BenchUser`'s scalars.
#[derive(Debug)]
pub struct RawUser {
    pub id: String,
    pub email: String,
    pub name: String,
    pub age: i64,
    pub active: bool,
    pub score: f64,
    pub bio: Option<String>,
    pub created_at: DateTime<Utc>,
}

const RAW_USERS_SQL: &str =
    "SELECT id, email, name, age, active, score, bio, created_at FROM bench_user ORDER BY id LIMIT ";

/// Reads `limit` users straight through the driver Dinoco wraps, decoding by
/// column index into a plain struct. This is the floor Dinoco's own
/// `find_many` overhead is measured against.
pub async fn raw_users(raw: &RawDriver, limit: usize) -> anyhow::Result<Vec<RawUser>> {
    let sql = format!("{RAW_USERS_SQL}{limit}");

    match raw {
        RawDriver::Sqlite(adapter) => {
            let connection = adapter.pool.get().await?;
            connection
                .interact(move |connection| -> anyhow::Result<Vec<RawUser>> {
                    let mut statement = connection.prepare_cached(&sql)?;
                    let rows = statement.query_map([], |row| {
                        Ok(RawUser {
                            id: row.get(0)?,
                            email: row.get(1)?,
                            name: row.get(2)?,
                            age: row.get(3)?,
                            active: row.get(4)?,
                            score: row.get(5)?,
                            bio: row.get(6)?,
                            created_at: row.get(7)?,
                        })
                    })?;
                    Ok(rows.collect::<Result<Vec<_>, _>>()?)
                })
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?
        }
        RawDriver::Postgres(adapter) => {
            let connection = adapter.pool.get().await?;
            let statement = connection.prepare_cached(&sql).await?;
            let rows = connection.query(&statement, &[]).await?;
            rows.iter()
                .map(|row| {
                    Ok(RawUser {
                        id: row.try_get(0)?,
                        email: row.try_get(1)?,
                        name: row.try_get(2)?,
                        age: row.try_get(3)?,
                        active: row.try_get(4)?,
                        score: row.try_get(5)?,
                        bio: row.try_get(6)?,
                        created_at: row.try_get::<_, dinoco::chrono::NaiveDateTime>(7)?.and_utc(),
                    })
                })
                .collect()
        }
        RawDriver::Mysql(adapter) => {
            use dinoco_engine::mysql_async::prelude::Queryable;

            let mut connection = adapter.pool.get_conn().await?;
            let rows: Vec<dinoco_engine::mysql_async::Row> = connection.exec(sql, ()).await?;
            rows.into_iter()
                .map(|mut row| {
                    Ok(RawUser {
                        id: row.take(0).ok_or_else(|| anyhow::anyhow!("id"))?,
                        email: row.take(1).ok_or_else(|| anyhow::anyhow!("email"))?,
                        name: row.take(2).ok_or_else(|| anyhow::anyhow!("name"))?,
                        age: row.take(3).ok_or_else(|| anyhow::anyhow!("age"))?,
                        active: row.take(4).ok_or_else(|| anyhow::anyhow!("active"))?,
                        score: row.take(5).ok_or_else(|| anyhow::anyhow!("score"))?,
                        bio: row.take(6).ok_or_else(|| anyhow::anyhow!("bio"))?,
                        created_at: row
                            .take::<dinoco::chrono::NaiveDateTime, _>(7)
                            .ok_or_else(|| anyhow::anyhow!("created_at"))?
                            .and_utc(),
                    })
                })
                .collect()
        }
    }
}

/// Global allocator wrapper counting allocations, allocated bytes and the
/// live-heap high-water mark across every thread (SQLite work runs on
/// blocking threads, so per-thread counters would miss it).
pub struct CountingAllocator;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_BYTES: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_pointer = unsafe { System.realloc(pointer, layout, new_size) };
        if !new_pointer.is_null() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            if new_size > layout.size() {
                let grown = new_size - layout.size();
                ALLOCATED_BYTES.fetch_add(grown as u64, Ordering::Relaxed);
                let live = LIVE_BYTES.fetch_add(grown, Ordering::Relaxed) + grown;
                PEAK_BYTES.fetch_max(live, Ordering::Relaxed);
            } else {
                LIVE_BYTES.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        new_pointer
    }
}

fn record_allocation(size: usize) {
    ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    ALLOCATED_BYTES.fetch_add(size as u64, Ordering::Relaxed);
    let live = LIVE_BYTES.fetch_add(size, Ordering::Relaxed) + size;
    PEAK_BYTES.fetch_max(live, Ordering::Relaxed);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AllocationSnapshot {
    pub allocations: u64,
    pub bytes: u64,
    pub live: usize,
}

pub fn snapshot() -> AllocationSnapshot {
    AllocationSnapshot {
        allocations: ALLOCATIONS.load(Ordering::Relaxed),
        bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
        live: LIVE_BYTES.load(Ordering::Relaxed),
    }
}

/// Resets the high-water mark to the current live heap and returns it.
pub fn reset_peak() -> usize {
    let live = LIVE_BYTES.load(Ordering::Relaxed);
    PEAK_BYTES.store(live, Ordering::Relaxed);
    live
}

pub fn peak() -> usize {
    PEAK_BYTES.load(Ordering::Relaxed)
}
