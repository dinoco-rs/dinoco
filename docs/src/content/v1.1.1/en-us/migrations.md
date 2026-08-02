# Migrations

Dinoco migrations are SQL artifacts produced from a validated schema diff. The active adapter's `SqlCompiler` writes dialect-specific enum, table, column, and foreign-key statements.

## Migration lifecycle

1. Compile `dinoco/schema.dinoco`, require exactly one primary key per model, and validate types, model attributes, and relations.
2. Connect to the primary database and introspect its actual structure.
3. Build the desired structure in isolated test tables inside the same database.
4. Compare the current and desired database schemas.
5. Print every planned step and warn about unsafe or destructive changes.
6. Ask for confirmation when data may be lost.
7. Write `up.sql` and `down.sql`, apply `up.sql`, record the migration, and regenerate Rust models.

The database, not a stale binary schema snapshot, is the current-state source for this comparison.

## Generate a migration

Set only the primary database URL:

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/app"
dinoco migrate generate
```

On PostgreSQL and MySQL, the CLI materializes the desired schema in the primary database using isolated tables prefixed with `dinoco_migration_test_`. It removes those tables, their foreign keys, and auxiliary enums as soon as planning finishes, including after an error. SQLite continues to use a temporary file for validation.

The `dinoco_migration_test_` prefix is reserved by Dinoco and must not be used for application tables. No additional shadow URL is required.

## How changes are detected

The planner compares:

- tables created and removed;
- columns added, removed, renamed, or changed;
- scalar type and database-native type changes;
- optional-to-required and required-to-optional transitions;
- defaults, primary keys, and uniqueness represented by the database;
- enums created, altered, and removed;
- foreign keys and referential actions added, changed, or removed;
- standard indexes declared with `@index` or `@@indexes`, unique groups declared with `@@uniques`, full-text indexes declared with `@fulltext` or `@@fulltexts`, primary-key indexes supplied by their constraints, and automatic foreign-key indexes;
- relations added or removed through their physical constraints.

Rename detection uses structural comparison and emits `RenameColumn` when the planner can identify the old and new field safely. Always verify an inferred rename; a drop plus unrelated add can look similar.

## Index migrations

The planner treats standard, unique, and full-text indexes separately. `@index` and `@@indexes` emit non-unique B-tree statements. Composite `@@uniques` emits `CREATE UNIQUE INDEX`. `@fulltext` and `@@fulltexts` emit PostgreSQL GIN, MySQL `FULLTEXT`, and no index on SQLite.

Primary keys declared with `@id` or `@@ids` appear in the desired schema, but their constraint satisfies the index and prevents a duplicate `CREATE INDEX`. Every foreign key receives an automatic index, including composite relations and both sides of an implicit many-to-many pivot.

Changes to an index name, columns, order, or kind emit matching drop/create steps. See [Indexes and constraints](/v1.1.1/guide/indexes) for schema rules.

## Dangerous changes

Dropping a populated table or column, narrowing a type, removing enum values, and making a nullable column required can destroy data or fail for existing rows. Dinoco prints a highlighted warning and defaults its confirmation prompt to `No`.

The first migration against a database that already has user tables also requires confirmation when `dinoco_migrations` is absent. This protects databases that were not previously managed by Dinoco.

In CI, `DINOCO_CLI_CONFIRM_DESTRUCTIVE=true` can answer the destructive prompt. Treat that variable as a privileged release setting, not a convenient default.

## Review the generated SQL

Each migration has its own directory:

```text
dinoco/migrations/1721320123456_generated/
  up.sql
  down.sql
```

Review `up.sql` for locks, data rewrites, index impact, and adapter-specific enum behavior. Review `down.sql` before relying on rollback: irreversible operations are written as explanatory SQL comments when prior values or deleted data cannot be reconstructed safely.

Commit both files. Do not edit an already applied migration; create a new schema change so every environment keeps the same history.

## Run pending migrations

```bash
dinoco migrate run
```

The command creates `dinoco_migrations` when necessary, sorts migration directories, skips names already recorded, and applies pending `up.sql` files in order.

## Run migrations when a local SQLite database starts

`connect()` now opens the first SQLite connection eagerly, creating the file when it does not exist. Code generation embeds the selected workspace's `up.sql` files in the generated `dinoco` module and exports a helper that applies them through the same client:

```rust
let client = dinoco::connect().await?;
let report = dinoco::migrate(&client).await?;

if report.changed() {
    println!("Applied migrations: {:?}", report.applied);
}
```

Calling `connect()` alone never applies migrations or creates application tables. Call `migrate()` explicitly only where the application should manage its local database schema.

You can also call `dinoco::runtime::run_migrations` directly. Runtime execution does not compile the schema or generate models: the SQL files are already embedded in the binary with `include_str!`. Migrations are sorted, executed in SQLite transactions, and recorded with checksums; removing or changing an applied migration returns an error.

## Generated models

Migration generation always regenerates `dinoco/mod.rs` and `dinoco/models/`. If the planner finds no database change, it still refreshes models before stopping. This keeps Rust output aligned with harmless schema or code-generation changes.
