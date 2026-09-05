# Dinoco

Dinoco is a schema-driven ORM for Rust. You describe your database once, in a single `dinoco/schema.dinoco` file, and Dinoco takes care of the rest: it validates that description, plans and applies adapter-specific migrations, generates plain Rust entities, and gives those entities a fully typed, `async` query API.

The core idea is simple: SQL stays visible and inspectable (every generated migration is real SQL you can read and review), while table names, column names, value types, and relation paths move into Rust code that the compiler checks for you. A typo in a field name, a relation that points at the wrong model, or a value of the wrong type — all of that fails to compile, instead of failing in production.

> [!NOTE]
> Dinoco is intentionally narrow in scope. It is an ORM and migration tool for PostgreSQL, MySQL, and SQLite — not a web framework, not a query language of its own, and not a generic "database abstraction" that hides SQL behind an opaque row type. If you already know SQL, most of what follows will feel familiar.

## Why Dinoco

Most Rust database libraries ask you to choose between two extremes: hand-write SQL and lose type safety, or adopt a heavy abstraction that hides what's actually being executed. Dinoco tries to sit in the middle:

- **One schema, three databases.** The same `.dinoco` schema compiles against PostgreSQL, MySQL, or SQLite. Switching adapters is a configuration change, not a rewrite.
- **Generated, not hand-written, models.** Rust structs, relation accessors, and query builders are generated from the schema, so they can never drift from it. Edit the schema, regenerate, and the compiler tells you exactly what broke.
- **Real migrations, not magic.** `dinoco migrate generate` introspects your live database, diffs it against your schema, and writes plain `up.sql`/`down.sql` files for you to review before they run.
- **No hidden N+1 queries.** Relation includes are always batched (a bounded number of follow-up queries per level of nesting), never a query-per-row.

## What you get out of the box

- Generated Rust models with `Entity`, typed fields, relation metadata, and a practical `new()` constructor.
- Async `find_first`, `find_many`, `count`, `insert_into`, `insert_many`, `update`, `update_many`, `find_and_update`, `delete`, and `delete_many` builders.
- Custom projections through `.select::<T>()`, so a query can return exactly the fields you ask for.
- Batched relation includes (`.includes(...)`) for both to-many and to-one relations.
- Direct PostgreSQL, PgBouncer, MySQL, and SQLite adapters, plus round-robin read replicas with an explicit primary-read override.
- Migration planning backed by live database introspection, with drift detection so Dinoco never silently overwrites schema changes it doesn't know about.
- A formatter and a full VS Code language server for `.dinoco` files: diagnostics, completion, hover, go to definition, rename, and semantic highlighting.

## How the workflow fits together

The day-to-day loop has four steps:

1. Edit `dinoco/schema.dinoco`.
2. Run `dinoco migrate generate` to validate the schema, plan the database change, and review it.
3. Let the CLI apply the migration once you're happy with the generated SQL.
4. Import the generated models from `dinoco/models/` and query them from your application code.

The generated `dinoco/` folder is part of your application — check it into version control like any other source file. It is not a second source of truth: the schema stays authoritative, and the generated Rust code is simply the typed bridge the runtime uses to talk to it.

## A minimal example

```rust
mod dinoco;

use dinoco::models::User;
use dinoco::{connect, models};
use ::dinoco::{find_many, insert_into};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = connect().await?;

    let user = User::new("ana@example.com".to_string(), "Ana".to_string());
    insert_into::<User>().values(&user).execute(&client).await?;

    let users = find_many::<User>()
        .includes(|x| x.posts())
        .execute(&client)
        .await?;

    println!("{} users", users.len());
    Ok(())
}
```

## The v2.0.0 surface

Dinoco keeps its public workflow deliberately compact. The CLI exposes `init`, migration generation and execution, and model generation — nothing more. The runtime exposes the builders listed above; there is no separate "create struct" type distinct from the entity itself, and relation fields are populated the same way scalar fields are, through the entity you already have.

> [!TIP]
> If you're coming from an ORM that generates separate `NewX`/`XChangeset`/`UpdateX` types for every model, expect Dinoco to feel lighter: one generated struct per model does double duty for both reads and writes.

## Next steps

Continue with the [quickstart](/en-us/docs/orm/guide/quickstart) to go from an empty Rust crate to a working project with a real database in a few minutes, or jump straight to the [complete example](/en-us/docs/orm/guide/cookbook) if you'd rather read a finished schema and query set first.
