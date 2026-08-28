# Dinoco v1.2.6

Dinoco is a schema-driven ORM for Rust. You describe the database once in `dinoco/schema.dinoco`; Dinoco validates that description, produces adapter-specific migrations, generates Rust entities, and gives those entities a typed query API.

The goal is simple: keep SQL behavior visible while moving table names, column names, value types, and relation paths into compile-time checked Rust code.

## What Dinoco gives you

- One schema for PostgreSQL, MySQL, and SQLite.
- Generated Rust models with `Entity`, typed fields, relation metadata, and a practical `new()` constructor.
- Async find, count, insert, update, atomic update, and delete builders.
- Custom projections through `EntityExtend` and `.select::<T>()`.
- Batched `many` includes and joined `one` includes, avoiding an N+1 query for each parent row.
- Direct PostgreSQL, PgBouncer, MySQL, and SQLite adapters.
- Read replicas with round-robin routing and an explicit primary-read override.
- Migration planning backed by database introspection and isolated validation tables.
- A formatter and VS Code language support for `.dinoco` files.

Dinoco does not hide the database behind a generic row representation. Each adapter decodes its native rows into generated entities, while its `SqlCompiler` emits the dialect it understands.

## How the workflow fits together

The normal development loop has four steps:

1. Edit `dinoco/schema.dinoco`.
2. Run `dinoco migrate generate` to validate and plan the database change.
3. Review the generated `up.sql` and `down.sql`, then let the CLI apply the migration.
4. Import the models generated under `dinoco/models/` and query them from Rust.

The generated folder is part of your application, not a second source of truth. Your schema remains authoritative; the generated Rust code is the typed bridge used by the runtime.

```rust
mod dinoco;

use dinoco::models::User;
use dinoco::{connect, models};
use ::dinoco::{find_many, insert_into};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = connect().await?;

    let user = User::new(
        "ana@example.com".to_string(),
        "Ana".to_string(),
    );
    insert_into::<User>().values(&user).execute(&client).await?;

    let users = find_many::<User>()
        .includes(|x| x.posts())
        .execute(&client)
        .await?;

    println!("{} users", users.len());
    Ok(())
}
```

## The v1.2.6 surface

Version 1.2.6 intentionally has a compact public workflow. The CLI exposes `init`, migration generation and execution, and model generation. The runtime exposes builders such as `find_many`, `insert_into`, `update_many`, and `delete`.

Older experimental APIs such as `#[insertable]`, separate create structs, `with_relation`, database reset, schema restore, queues, and cache helpers are not part of this version. Insert data is represented by the entity itself, including its relation fields.

Continue with the quickstart to build a working project from an empty Rust crate.
