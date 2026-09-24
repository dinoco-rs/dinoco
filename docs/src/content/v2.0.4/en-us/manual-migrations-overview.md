# Manual migrations overview

Dinoco supports two migration engines. Both start from the same `schema.dinoco`, and both generate the same Rust models — they differ only in **who writes the database changes**.

| | `automatic` (default) | `manual` |
| --- | --- | --- |
| Who writes the change | Dinoco diffs the schema against the live database | You, in Rust |
| Migration artifact | `up.sql` + `down.sql` in `dinoco/migrations/<name>/` | A `.rs` file per migration in `dinoco/migrations/` |
| `dinoco migrate generate` | Plans, asks for confirmation, writes and applies the migration | Scaffolds an empty migration and regenerates the models |
| Rollback | Not supported by the CLI | `dinoco migrate rollback` runs your `down` |
| History table | `dinoco_migrations` | `dinoco_manual_migrations` |

## Choose the engine

```dinoco
config {
    database         = "sqlite"
    database_url     = env("DATABASE_URL")
    migration_engine = "manual"
}
```

`migration_engine` accepts `"automatic"` or `"manual"`, and defaults to `"automatic"` when the key is missing — existing projects keep working untouched. It is a database-level setting, so with [workspaces](/en-us/docs/orm/guide/configuration#workspaces) each workspace can choose its own engine.

A new project can start in manual mode directly:

```bash
dinoco init --migration-engine manual
```

> [!WARNING]
> Pick one engine per database. The two engines keep separate history tables and neither reads the other's, so switching an existing database from one to the other means the new engine sees an empty history. Baseline the database yourself (a first manual migration that only contains what already exists, marked with `if_not_exists`) before relying on it.

## When to choose manual

Reach for `manual` when the automatic diff is not the migration you want to ship:

- **Data migrations.** Backfilling a column, splitting a table, or rewriting values between two schema changes.
- **Precise control.** Extensions, partial indexes, triggers, or database-specific DDL that `schema.dinoco` cannot express — run them through `manager.execute("...")`.
- **Reviewed, reversible releases.** A `down` you wrote and tested yourself, and `dinoco migrate rollback` to use it.
- **Shared databases.** Databases that other tools also change, where a schema diff would keep proposing to undo their work.

Stay on `automatic` when the schema is the source of truth and you want Dinoco to plan renames, drops, and constraint changes for you.

## Project layout

```text
dinoco/
├── schema.dinoco
├── mod.rs                              generated: `pub mod models; pub mod migrations;`
├── models/                             generated
└── migrations/
    ├── mod.rs                          registry, edited by the CLI
    ├── 20260919120000_create_users.rs
    └── 20260920093000_add_user_email.rs
```

`dinoco/migrations/mod.rs` is the registry. It lists every migration in order, between marker comments the CLI maintains:

```rust
#![allow(unused)]

// dinoco:migrations:mods:start
#[path = "20260919120000_create_users.rs"]
mod m20260919120000_create_users;
// dinoco:migrations:mods:end

pub fn migrations() -> Vec<::dinoco::MigrationEntry> {
    vec![
        // dinoco:migrations:list:start
        ::dinoco::MigrationEntry::new("20260919120000_create_users", m20260919120000_create_users::CreateUsers),
        // dinoco:migrations:list:end
    ]
}
```

`dinoco migrate generate <name>` adds a line inside each marked region. Do not remove the markers; everything outside them is yours to edit. Order in the list is the order migrations run, and the timestamp prefix keeps files sorted the same way.

## What is generated for you

The generated `dinoco/mod.rs` gains `pub mod migrations;` (or `#[path = "migrations/<workspace>/mod.rs"]` for a workspace), so the registry is reachable from your application as `dinoco::migrations::migrations()`. See [Writing migrations](/en-us/docs/orm/guide/writing-migrations) next, and the [CLI workflow](/en-us/docs/orm/guide/migration-workflow) for the commands.
