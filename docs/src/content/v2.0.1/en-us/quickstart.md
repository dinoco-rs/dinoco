# Quickstart

This walkthrough takes you from an empty Rust binary crate to a migrated PostgreSQL database with a typed insert and a typed query, in about five minutes. If you'd rather read a finished, more realistic schema first, skip ahead to the [complete example](/en-us/docs/orm/guide/cookbook).

## Prerequisites

- A current stable Rust toolchain (`rustup show` should print a `stable` channel).
- Access to a PostgreSQL, MySQL, or SQLite database. This guide uses PostgreSQL with a direct connection; the [clients and adapters](/en-us/docs/orm/orm/clients-adapters) page covers the other two.

## 1. Install Dinoco

Install the CLI binary and add the runtime crates your generated code will depend on:

```bash
cargo install dinoco --version 2.0.1
cargo add dinoco@2.0.1 dinoco_engine@2.0.1 anyhow
cargo add tokio --features macros,rt-multi-thread
```

Each crate has one job:

- `dinoco` — the query API (`find_many`, `insert_into`, and so on) and the `Entity` derive your generated models use.
- `dinoco_engine` — the adapters and connection pooling used by the generated `connect()` function.
- `anyhow` / `tokio` — used by the generated code and by the example below for error handling and the async runtime.

> [!TIP]
> Also install the [VS Code extension](/en-us/docs/orm/tooling/vscode) before you go further — you'll get inline diagnostics and completion while editing `dinoco/schema.dinoco` in the next step.

## 2. Initialize the project

From your Cargo project root, run the interactive initializer:

```bash
dinoco init
```

Pick `postgresql`, then `direct`. Dinoco writes:

```text
dinoco/
  migrations/
  schema.dinoco
```

`schema.dinoco` is created with a starter `config` block; nothing else is generated until you run a migration. Database credentials are never written into the schema file itself — Dinoco reads them from the environment at runtime:

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/my_app"
```

> [!WARNING]
> `database_url = env("DATABASE_URL")` in the schema is a reference to an environment variable name, not a place to paste a real connection string. Committing a schema with a literal URL isn't just a bad practice here — the compiler rejects it.

## 3. Define the schema

Replace the generated `dinoco/schema.dinoco` with this:

```dinoco
config {
    database     = "postgresql"
    connection   = "direct"
    database_url = env("DATABASE_URL")
}

enum Role {
    member
    admin
}

model User {
    id         String   @id @default(uuid())
    email      String   @unique
    name       String
    role       Role     @default(member)
    created_at DateTime @default(now())
}
```

Four fields (`id`, `email`, `name`, `role`) plus a timestamp — but notice that only `email` and `name` lack a `@default(...)`. That matters for the next step: the generated `User::new` constructor only takes arguments for fields without a default, since Dinoco (and the database) already knows how to fill in the rest.

## 4. Generate and run the migration

```bash
dinoco migrate generate
```

This single command does five things, in order:

1. Compiles and validates the schema (unknown types, missing primary keys, and invalid relations are all caught here, before touching the database).
2. Connects to `DATABASE_URL` and introspects the database's current structure.
3. Plans the SQL needed to get from that structure to the one your schema describes.
4. Verifies the plan against isolated `dinoco_migration_test_*` tables in the same database — no separate shadow database required.
5. Writes the migration, applies it, and regenerates the Rust models.

The migration itself lands on disk as plain SQL you can read and re-run later:

```text
dinoco/migrations/<timestamp>_<name>/
  up.sql
  down.sql
```

Generated models are placed in `dinoco/models/`, and `dinoco/mod.rs` exposes an async `connect()` function wired to the adapter your `config` block describes. To apply migrations that already exist on disk (for example, in CI or on another machine) without generating a new one:

```bash
dinoco migrate run
```

> [!NOTE]
> **Project structure.** Everything Dinoco generates lives under `dinoco/` next to your schema — treat it as part of your application and commit it to version control, the same way you would generated Protobuf or GraphQL bindings. The [schema organization](/en-us/docs/orm/guide/schema-organization) page covers multi-file schemas and larger project layouts.

## 5. Use the generated client

```rust
mod dinoco;

use ::dinoco::{find_first, insert_into};
use dinoco::{connect, User};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = connect().await?;

    let user = User::new("ana@example.com".to_string(), "Ana".to_string());

    insert_into::<User>().values(&user).execute(&client).await?;

    let saved = find_first::<User>()
        .where_(|x| x.email.eq("ana@example.com"))
        .execute(&client)
        .await?;

    println!("{saved:#?}");
    Ok(())
}
```

A couple of details worth internalizing early, since they show up in every query you'll write:

- `.values(&user)` takes the entity **by reference** — the insert does not consume `user`, so you can reuse it afterwards (for logging, a second insert, whatever you need).
- `find_first` returns `anyhow::Result<Option<User>>`. A row not being found is a normal, expected `None` — not an error. Reach for `find_first` when zero-or-one is a valid outcome, and treat an `Err` as an actual failure (a broken connection, a malformed query).

## Next steps

- [Complete example](/en-us/docs/orm/guide/cookbook) — a longer, copyable schema with relations and a many-to-many write.
- [Models and fields](/en-us/docs/orm/guide/models) — the full list of scalar types and field attributes.
- [Query overview](/en-us/docs/orm/orm/find) — how to pick between `find_first`, `find_many`, and the other read builders.
