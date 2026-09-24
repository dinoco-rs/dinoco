# Writing migrations

A manual migration is a type that implements `DinocoMigration`. `dinoco migrate generate <name>` creates the file for you:

```bash
dinoco migrate generate create_users
```

```rust
use ::dinoco::*;

#[dinoco(migration)]
pub struct CreateUsers;

impl DinocoMigration for CreateUsers {
    async fn up(&self, manager: &DinocoManager) -> Result<(), DbErr> {
        Ok(())
    }

    async fn down(&self, manager: &DinocoManager) -> Result<(), DbErr> {
        Ok(())
    }
}
```

## The pieces

- **`#[dinoco(migration)]`** marks the type as a migration. The type itself is left untouched; the attribute adds a compile-time check that it really implements `DinocoMigration`, so a forgotten `impl` is reported on the type instead of deep inside the registry.
- **`DinocoMigration`** has two required methods, `up` and `down`. Both are plain `async fn` — no `#[async_trait]` needed.
- **`DinocoManager`** is what `up`/`down` use to change the database. It compiles each operation to the dialect of the connected database (PostgreSQL, PgBouncer, MySQL, or SQLite).
- **`DbErr`** is the error type: an alias for `anyhow::Error`, so `?` works on anything and you can attach context with `.context(...)`.

Both methods must finish with `Ok(())` (or return the last `.await` directly). The future they return must be `Send`, which is true unless you hold a non-`Send` value across an `.await`.

## Create a table

```rust
async fn up(&self, manager: &DinocoManager) -> Result<(), DbErr> {
    manager
        .create_table(CreateTableMigration {
            table: "user".to_string(),
            if_not_exists: false,
            columns: vec![
                MigrationColumn::new("id", MigrationColumnType::String).primary_key(),
                MigrationColumn::new("email", MigrationColumnType::String).unique(),
                MigrationColumn::new("bio", MigrationColumnType::Text).nullable(),
                MigrationColumn::new("created_at", MigrationColumnType::DateTime)
                    .default(MigrationDefault::CurrentTimestamp),
            ],
            foreign_keys: Vec::new(),
        })
        .await
}

async fn down(&self, manager: &DinocoManager) -> Result<(), DbErr> {
    manager
        .drop_table(DropTableMigration { table: "user".to_string(), if_exists: false })
        .await
}
```

`MigrationColumn::new(name, type)` builds a required, non-unique column without a default. Chain `.primary_key()`, `.unique()`, `.nullable()`, and `.default(...)` to change that. You can also fill the struct fields directly.

## Several steps in one migration

Every `manager` call is awaited in order. Undo them in the **reverse** order in `down`:

```rust
async fn up(&self, manager: &DinocoManager) -> Result<(), DbErr> {
    manager
        .add_column(AddColumnMigration {
            table: "user".to_string(),
            column: MigrationColumn::new("email", MigrationColumnType::String).nullable(),
        })
        .await?;

    manager
        .create_index(CreateIndexMigration {
            table: "user".to_string(),
            index: MigrationIndex {
                name: "user_email_idx".to_string(),
                columns: vec!["email".to_string()],
                automatic: false,
                kind: MigrationIndexKind::Standard,
            },
        })
        .await
}

async fn down(&self, manager: &DinocoManager) -> Result<(), DbErr> {
    manager
        .drop_index(DropIndexMigration {
            table: "user".to_string(),
            index: MigrationIndex {
                name: "user_email_idx".to_string(),
                columns: vec!["email".to_string()],
                automatic: false,
                kind: MigrationIndexKind::Standard,
            },
        })
        .await?;

    manager
        .drop_column(DropColumnMigration { table: "user".to_string(), column: "email".to_string() })
        .await
}
```

## Foreign keys

```rust
manager
    .add_foreign_key(AddForeignKeyMigration {
        table: "post".to_string(),
        foreign_key: MigrationForeignKey {
            name: "post_author_id_fkey".to_string(),
            columns: vec!["author_id".to_string()],
            references_table: "user".to_string(),
            references_columns: vec!["id".to_string()],
            on_update: ReferentialAction::Cascade,
            on_delete: ReferentialAction::Restrict,
        },
    })
    .await?;
```

SQLite cannot add or drop a foreign key on an existing table, or alter a column in place. Those calls return an error (`this operation is not supported by the connected database`) instead of silently doing nothing — declare the foreign key in `create_table`'s `foreign_keys`, or rebuild the table with raw SQL.

## Data migrations and raw SQL

`manager.execute(sql)` runs one raw statement. Use it for backfills and for anything the typed methods do not cover:

```rust
async fn up(&self, manager: &DinocoManager) -> Result<(), DbErr> {
    manager
        .add_column(AddColumnMigration {
            table: "user".to_string(),
            column: MigrationColumn::new("display_name", MigrationColumnType::String).nullable(),
        })
        .await?;

    manager.execute("UPDATE \"user\" SET display_name = email WHERE display_name IS NULL").await
}
```

Quote identifiers and write SQL for the database you deploy to — raw statements are not translated between dialects. For anything else that needs the connection, `manager.backend()` returns the underlying `Backend`.

## Naming, order and files

- Migration names are `<UTC timestamp>_<snake_case_name>`, e.g. `20260919120000_create_users`. That string is what the history table stores, so **never rename an applied migration** — Dinoco would see the old name as applied but missing.
- The Rust type is the PascalCase form of the name (`CreateUsers`). A name that would start with a digit gets a `Migration` prefix.
- The order in `migrations()` is the execution order. Append new migrations at the end; do not reorder applied ones.
- Migrations are compiled by the runner (see [CLI workflow](/en-us/docs/orm/guide/migration-workflow#how-migrations-are-executed)), so they should only depend on `::dinoco` — not on your application's own modules.

## Failure behavior

A migration is recorded as applied only after its `up` returns `Ok`. If it fails, nothing is recorded and the run stops, leaving earlier migrations applied.

Dinoco does not wrap a migration in a transaction. PostgreSQL and SQLite could roll back DDL, MySQL cannot, and a partial failure would then behave differently per database. Keep each migration small, prefer `if_not_exists`/`if_exists` in idempotent steps, and be ready to fix a half-applied migration by hand before re-running.
