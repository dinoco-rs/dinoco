# Quickstart

This walkthrough starts with an empty binary crate and ends with a migrated database plus a typed insert and find.

## Prerequisites

Install a current stable Rust toolchain. You also need access to PostgreSQL, MySQL, or a writable location for a SQLite file. The examples below use PostgreSQL Direct.

## 1. Install Dinoco

Install the v1.0.0 CLI and add the runtime dependencies to your project:

```bash
cargo install dinoco --version 1.0.0
cargo add dinoco@1.0.0 dinoco_engine@1.0.0 anyhow
cargo add tokio --features macros,rt-multi-thread
```

`dinoco` provides the query API and derives. `dinoco_engine` is used by the generated connection module. `anyhow` and `tokio` support the generated async application flow.

## 2. Initialize the project

Run the interactive initializer from the Cargo project root:

```bash
dinoco init
```

Choose `postgresql`, then `direct`. Dinoco creates:

```text
dinoco/
  migrations/
  schema.dinoco
```

Set the connection URL in the environment. Connection URLs cannot be written as literals in the schema.

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/my_app"
```

## 3. Define the schema

Replace `dinoco/schema.dinoco` with this small schema:

```dinoco
config {
    database = "postgresql"
    connection = "direct"
    database_url = env("DATABASE_URL")
    read_replicas = []
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

The generated `User::new` only asks for `email` and `name`. `id`, `role`, and `created_at` are omitted because their values have defaults.

## 4. Generate and run the migration

Generate a named migration:

```bash
dinoco migrate generate
```

The CLI compiles and validates the schema, connects to the configured database, introspects its current structure, verifies the intended result in isolated `dinoco_migration_test_*` tables, and writes a migration directory:

```text
dinoco/migrations/<timestamp>_<name>/
  up.sql
  down.sql
```

The generated models are placed in `dinoco/models/`, and `dinoco/mod.rs` exposes `connect()`. To apply any pending migrations later, run:

```bash
dinoco migrate run
```

## 5. Use the generated client

Add the generated module and use the entity directly as insert input:

```rust
mod dinoco;

use ::dinoco::{find_first, insert_into};
use dinoco::{connect, User};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = connect().await?;

    let user = User::new(
        "ana@example.com".to_string(),
        "Ana".to_string(),
    );

    insert_into::<User>()
        .values(&user)
        .execute(&client)
        .await?;

    let saved = find_first::<User>()
        .where_(|x| x.email.eq("ana@example.com"))
        .execute(&client)
        .await?;

    println!("{saved:#?}");
    Ok(())
}
```

`.values(&user)` accepts a borrowed entity, so the insert does not consume `user`. `find_first` returns `anyhow::Result<Option<User>>`; an empty match is a normal `None`, not an error.
