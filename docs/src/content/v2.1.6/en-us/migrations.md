# Migrations

A Dinoco migration is a SQL artifact produced from a validated diff between your schema and the real state of the database — never from an imagined or cached "previous schema." The active adapter's `SqlCompiler` writes the dialect-specific enum, table, column, and foreign-key statements needed to close that gap.

## Migration lifecycle

1. Compile `dinoco/schema.dinoco` — requiring exactly one primary key per model, and validating types, model attributes, and relations before touching the database at all.
2. Connect to the primary database and introspect its actual, current structure.
3. Build the schema's desired structure: in memory for PostgreSQL and MySQL, in a temporary database file for SQLite.
4. Compare the current and desired structures.
5. Print every planned step, flagging anything unsafe or destructive.
6. Take the decisions the plan needs: confirm or reject inferred renames, pick replacements for removed enum values that rows still use, and approve the plan. Destructive plans need explicit approval.
7. Write `up.sql` and `down.sql`, apply `up.sql`, record the migration, and regenerate Rust models. With `--create-only`, stop after writing the files.

> [!NOTE]
> The database itself — not a cached binary schema snapshot from a previous run — is always the source of truth for "current state" in this comparison. There's no separate metadata file that can drift out of sync with reality.

## Generate a migration

You only need the primary database URL set:

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/app"
dinoco migrate generate
```

On PostgreSQL and MySQL, the desired structure is computed from the schema without creating anything in the database. SQLite builds it in a temporary database file, which is deleted when planning finishes. No shadow database or shadow URL is required for any adapter.

Before any decision, the command prints the **Target** it works on: the workspace (when there is one), the database, its location with credentials hidden, and the migrations directory.

## How changes are detected

The planner compares, exhaustively:

- Tables created and removed.
- Columns added, removed, renamed, or changed.
- Scalar type changes and database-native type changes.
- Optional-to-required and required-to-optional transitions.
- Defaults, primary keys, and uniqueness as represented in the database.
- Enums created, altered, and removed.
- Foreign keys and referential actions added, changed, or removed.
- Standard indexes (`@index`/`@@indexes`), unique groups (`@@uniques`), full-text indexes (`@fulltext`/`@@fulltexts`), primary-key indexes supplied by their own constraints, and automatic foreign-key indexes.
- Relations added or removed, via their physical constraints.

## Renames

Rename detection is structural: when a removed table or column matches exactly one added one, the planner infers a rename, so existing data moves with it. A genuine removal followed by an unrelated addition can look the same, so an inferred rename is never applied without a decision.

In a terminal, Dinoco asks about each inferred rename:

```text
Migration Planning
A possible column rename was detected.

  Table    user
  Removed  username
  Added    display_name

? Is this a column rename?
> Yes, rename `username` to `display_name` and keep its values
  No, drop `username` (its values are deleted) and add `display_name`
  Cancel migration planning
```

Without a terminal, pass the decision as an option. The error lists every rename that needs one:

```bash
# Keep the data: rename the column
dinoco migrate generate --yes --rename user.username=display_name

# Plan a removal plus an addition instead (the old values are deleted)
dinoco migrate generate --allow-destructive --no-rename user.username=display_name

# Confirm every inferred rename
dinoco migrate generate --yes --accept-renames
```

Mappings are `TABLE.OLD=NEW` for a column and `OLD=NEW` for a table, with the table name the plan shows; the error message prints the exact mapping to pass. A confirmed rename is not a destructive change, so `--yes` is enough. When a new name matches more than one removed one, the plan is ambiguous: Dinoco stops before writing anything and asks you to change the schema in smaller steps.

## Removed enum values

Removing a value from an enum fails, or loses data, when rows still use it. Before applying such a change, Dinoco counts the rows that hold each removed value. When some do, it asks for a replacement:

```text
Enum Migration
The enum value "PROCESSING" is being removed, but existing rows still use it.

  Enum   Status
  Value  PROCESSING
  Usage  12 rows in task.status

? Select the replacement value:
> ACTIVE
  DONE
  Cancel migration planning
```

Only values that exist before the migration and are kept by it are offered, because rows can't switch to a value the database doesn't accept yet. Without a terminal, pass `--replace-enum-value ENUM.VALUE=REPLACEMENT`:

```bash
dinoco migrate generate --allow-destructive --replace-enum-value Status.PROCESSING=DONE
```

The plan then shows a **Data Changes** section (`~ Replace `PROCESSING` with `DONE` in `task.status` (12 rows)`), and `up.sql` starts with the matching `UPDATE`, before any column stops accepting the old value. Because the original values are overwritten, `down.sql` explains that the migration can't be reversed instead of generating SQL.

This applies to MySQL and SQLite, which store enum values per column. PostgreSQL can't drop a value from an enum type, so a plan that removes one is refused with an explanation before any replacement is asked for; write a reviewed custom migration for it.

## Index migrations

The planner treats standard, unique, and full-text indexes as three separate concerns. `@index`/`@@indexes` emit non-unique B-tree statements. A composite `@@uniques` emits `CREATE UNIQUE INDEX`. `@fulltext`/`@@fulltexts` emit a PostgreSQL GIN index, a MySQL `FULLTEXT` index, and no index at all on SQLite (see [Full-text search](/en-us/docs/orm/orm/full-text-search) for why).

Primary keys (`@id`/`@@ids`) appear in the desired schema's comparison model, but their own constraint already satisfies the index — Dinoco never emits a redundant duplicate `CREATE INDEX` for one. Every foreign key gets its automatic index too, composite relations and both sides of an implicit many-to-many pivot included.

Any change to an index's name, columns, order, or kind produces matching drop/create steps. See [Indexes and constraints](/en-us/docs/orm/guide/indexes) for the schema-level rules behind all of this.

## Dangerous changes

Dropping a populated table or column, narrowing a column's type, removing enum values, and turning a nullable column required can all destroy data or fail outright against existing rows. Dinoco lists these under **Data Impact**, and in a terminal the approval prompt starts on **Cancel**, so you have to actively opt in. Without a terminal, `--yes` is not enough: pass `--allow-destructive`.

Approving is not enough for a change the existing rows can't take. Before asking, and before writing any file, `migrate generate` checks the rows: a column that becomes required while some rows hold `NULL` in it, or a required column without a default added to a table that has rows, stops the command with the table, the column and the number of rows, and nothing is written or changed. Fill in those values, or keep the column optional (or give it a `@default`) until the rows are backfilled. This matters most on MySQL, which commits each DDL statement on its own and would otherwise be left partly migrated. `--create-only` skips the check, since it applies nothing.

The very first migration against a database that already has user tables, but no `dinoco_migrations` table yet, also needs approval (`--adopt-baseline` without a terminal). It is only possible when the existing tables match the schema exactly, which protects a database that wasn't managed by Dinoco from an unreviewed first migration that assumes it owns everything. Repairing drift, changes made to the database outside Dinoco migrations, needs approval too (`--repair-drift`).

> [!NOTE]
> Treat `--allow-destructive` in CI as a privileged, deliberate release-pipeline setting, not a default added to every call to stop being asked. The `DINOCO_CLI_CONFIRM_DESTRUCTIVE=true` variable from earlier releases still works the same way.

## Create a migration without applying it

```bash
dinoco migrate generate --create-only --yes
```

`--create-only` takes the same decisions and writes the same `up.sql` and `down.sql`, but doesn't apply or record the migration and doesn't regenerate the models. Review the files, then apply them with `dinoco migrate run` and update the Rust code with `dinoco models generate`.

When the database first needs a drift repair, a baseline adoption, or an upgrade of migration history from an earlier release, `--create-only` is refused, since those steps change the database.

## Review the generated SQL

Each migration gets its own directory:

```text
dinoco/migrations/1721320123456_generated/
  up.sql
  down.sql
```

Read `up.sql` for lock behavior, data rewrites, index impact, and adapter-specific enum handling before applying it against anything that matters. Read `down.sql` too, before ever relying on it for a rollback: an operation Dinoco can't safely reverse (because the prior values or the deleted data can't be reconstructed) is written there as an explanatory SQL comment instead of a working statement.

Commit both files to version control. Never hand-edit a migration that's already been applied somewhere — create a new one for the follow-up change instead, so every environment keeps exactly the same history in the same order.

## Run pending migrations

```bash
dinoco migrate run
```

This creates `dinoco_migrations` if it doesn't exist yet, sorts migration directories, skips anything already recorded, and applies pending `up.sql` files in order — nothing here plans a new migration, it only applies what's already on disk.

## Run migrations when a local SQLite database starts

`connect()` opens the first SQLite connection eagerly and creates the database file if it doesn't exist yet — but that alone never applies any migration. Code generation separately embeds the active workspace's `up.sql` files directly into the generated `dinoco` module, and exports a helper that applies them through the same client:

```rust
let client = dinoco::connect().await?;
let report = dinoco::migrate(&client).await?;

if report.changed() {
    println!("Applied migrations: {:?}", report.applied);
}
```

> [!NOTE]
> Calling `connect()` by itself never applies migrations or creates application tables — call `migrate(&client)` explicitly, and only in the specific places where you actually want the application itself to own managing its local database schema (a desktop app shipping its own SQLite file is the classic case; a server talking to a centrally managed PostgreSQL usually isn't).

You can also call `dinoco::runtime::run_migrations` directly for more control. This runtime path never compiles the schema or generates models — the SQL is already embedded in the binary via `include_str!` at build time. Migrations still run sorted, inside real SQLite transactions, and get recorded with checksums; removing or altering a migration that's already been applied is treated as an error, not silently accepted.

## Generated models

`migrate generate` always regenerates `dinoco/mod.rs` and `dinoco/models/`, even when the planner finds zero database changes to make — models still get refreshed before the command exits. The exception is `--create-only`, which leaves them for `dinoco models generate` after the migration is applied. This keeps the generated Rust output aligned with schema or codegen changes that don't happen to require touching the database at all.
