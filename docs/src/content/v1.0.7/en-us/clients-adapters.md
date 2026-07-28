# Clients and adapters

`DinocoClient` owns one primary `Backend` and, optionally, several read-replica backends. Builders receive `&DinocoClient`, so the same client can be shared without transferring ownership.

## Generated connection

The generated `dinoco/mod.rs` exports `connect()`. It reads the environment variable named by `database_url` and constructs the selected primary adapter:

```rust
mod dinoco;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = dinoco::connect().await?;
    // Use &client in every query.
    Ok(())
}
```

Use this path for the normal single-primary setup. Build a client manually when you need explicit adapter ownership or read replicas.

## PostgreSQL Direct

Direct uses the pooled PostgreSQL adapter:

```rust
use dinoco_engine::{Backend, DinocoClient, PostgresAdapter};

let adapter = PostgresAdapter::direct(
    "postgres://postgres:postgres@localhost:5432/app"
).await?;
let client = DinocoClient::new(Backend::Postgres(adapter));
```

The constructor is async because it creates and validates the connection pool.

## PostgreSQL with PgBouncer

Use the PgBouncer adapter when the URL points to your PgBouncer service:

```rust
use dinoco_engine::{Backend, DinocoClient, PgBouncerAdapter};

let adapter = PgBouncerAdapter::new(
    "postgres://app:secret@pgbouncer:6432/app"
).await?;
let client = DinocoClient::new(Backend::PgBouncer(adapter));
```

Direct and PgBouncer share the PostgreSQL `SqlCompiler`, so query semantics and migration types remain the same.

## MySQL

```rust
use dinoco_engine::{Backend, DinocoClient, MySqlAdapter};

let adapter = MySqlAdapter::new(
    "mysql://app:secret@localhost:3306/app"
);
let client = DinocoClient::new(Backend::Mysql(adapter));
```

The adapter connects lazily when a query executes.

## SQLite

```rust
use dinoco_engine::{Backend, DinocoClient, SqliteAdapter};

let adapter = SqliteAdapter::new("dinoco/database.sqlite")
    .await
    .map_err(anyhow::Error::msg)?;
let client = DinocoClient::new(Backend::Sqlite(adapter));
```

Use a path under `dinoco/` when the database is project-local. The adapter creates or opens that file.

## Read replicas

Create each replica with the same adapter family as the primary, then attach the backends:

```rust
use dinoco_engine::{Backend, DinocoClient, PostgresAdapter};

let primary = PostgresAdapter::direct(primary_url).await?;
let replica_a = PostgresAdapter::direct(replica_a_url).await?;
let replica_b = PostgresAdapter::direct(replica_b_url).await?;

let client = DinocoClient::new(Backend::Postgres(primary))
    .with_read_replicas(vec![
        Backend::Postgres(replica_a),
        Backend::Postgres(replica_b),
    ]);
```

Read queries alternate across replicas with a lock-free round-robin index. If the vector is empty, they use the primary.

## Execution rules

- Inserts, updates, and deletes always use `client.backend`, the primary.
- `find_first`, `find_many`, includes, and counts use the read path.
- `.read_in_primary()` routes a find and its nested includes to the primary.
- Adapter-specific row implementations decode SQLite, deadpool PostgreSQL, native PostgreSQL, and MySQL rows directly.

Keep a client alive and reuse it for the application lifetime. Recreating adapters per request discards pooling and adds connection setup to the hot path.
