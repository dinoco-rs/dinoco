# Clients and adapters

A `DinocoClient` wraps exactly one primary `Backend` and, optionally, a set of read-replica backends. Every query builder takes `&DinocoClient` by reference, so one client is meant to be created once and shared across your whole application — not rebuilt per request.

## Generated connection

The generated `dinoco/mod.rs` exports a `connect()` function that reads the environment variables named by `database_url` and `read_replicas`, then builds the active workspace's primary and replica adapters for you:

```rust
mod dinoco;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = dinoco::connect().await?;
    // Use &client in every query.
    Ok(())
}
```

Use this path for anything a normal schema config already describes, replicas included. Building a client by hand — the rest of this page — is for the minority of cases where you need explicit control over adapter construction, or replica configuration that changes at runtime rather than at deploy time.

## PostgreSQL Direct

Direct uses the pooled PostgreSQL adapter:

```rust
use dinoco_engine::{Backend, DinocoClient, PostgresAdapter};

let adapter = PostgresAdapter::direct(
    "postgres://postgres:postgres@localhost:5432/app"
).await?;
let client = DinocoClient::new(Backend::Postgres(adapter));
```

The constructor is `async` because it establishes and validates the connection pool immediately, rather than lazily on first use.

## PostgreSQL with PgBouncer

Reach for the PgBouncer adapter specifically when the URL you're connecting to is a PgBouncer endpoint, not PostgreSQL directly:

```rust
use dinoco_engine::{Backend, DinocoClient, PgBouncerAdapter};

let adapter = PgBouncerAdapter::new(
    "postgres://app:secret@pgbouncer:6432/app"
).await?;
let client = DinocoClient::new(Backend::PgBouncer(adapter));
```

Direct and PgBouncer share the exact same PostgreSQL `SqlCompiler`, so query semantics and generated migration types are identical between them — only connection handling differs.

## MySQL

```rust
use dinoco_engine::{Backend, DinocoClient, MySqlAdapter};

let adapter = MySqlAdapter::new("mysql://app:secret@localhost:3306/app");
let client = DinocoClient::new(Backend::Mysql(adapter));
```

Unlike the PostgreSQL adapters, this constructor isn't `async` — the adapter connects lazily, on the first query that actually runs.

## SQLite

```rust
use dinoco_engine::{Backend, DinocoClient, SqliteAdapter};

let adapter = SqliteAdapter::new("dinoco/database.sqlite")
    .await
    .map_err(anyhow::Error::msg)?;
let client = DinocoClient::new(Backend::Sqlite(adapter));
```

Use a path under `dinoco/` for a project-local database — the adapter creates the file if it doesn't already exist, or opens it if it does.

## Read replicas

Build each replica with the same adapter family as the primary, then attach all of them to the client together:

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

Reads alternate across the attached replicas using a lock-free round-robin index; with an empty vector, every read simply goes to the primary instead.

## Execution rules

- Inserts, updates, and deletes always run against `client.backend` — the primary. There is no configuration that routes a write to a replica.
- `find_and_update` always uses the primary too, since it's fundamentally a write dressed up as a read-and-mutate operation.
- `find_first`, `find_many`, relation includes, and counts all use the read path, which is replica-eligible.
- `.read_in_primary()` forces one specific find (and everything it `.includes(...)`) onto the primary, overriding replica routing for that call only.
- The closure transaction API always pins itself to one physical connection from the primary backend for its entire duration.
- Row decoding is adapter-specific: SQLite, deadpool-pooled PostgreSQL, native PostgreSQL, and MySQL each implement their own direct row conversion, so there's no generic "row" abstraction layer adding overhead in between.

> [!WARNING]
> Construct a client once and keep it alive for your application's lifetime — a `DinocoClient`, and the connection pool(s) it owns, are meant to be long-lived. Recreating adapters per request throws away pooling entirely and puts connection setup directly on your hot path.
