# Migrations

A Dinoco migration is a SQL artifact produced from a validated diff between your schema and the real state of the database — never from an imagined or cached "previous schema." The active adapter's `SqlCompiler` writes the dialect-specific enum, table, column, and foreign-key statements needed to close that gap.

## Migration lifecycle

1. Compile `dinoco/schema.dinoco` — requiring exactly one primary key per model, and validating types, model attributes, and relations before touching the database at all.
2. Connect to the primary database and introspect its actual, current structure.
3. Build the schema's desired structure inside isolated test tables, in that same database.
4. Compare the current and desired structures.
5. Print every planned step, flagging anything unsafe or destructive.
6. Ask for confirmation whenever data might be lost.
7. Write `up.sql` and `down.sql`, apply `up.sql`, record the migration, and regenerate Rust models.

> [!NOTE]
> The database itself — not a cached binary schema snapshot from a previous run — is always the source of truth for "current state" in this comparison. There's no separate metadata file that can drift out of sync with reality.

## Generate a migration

You only need the primary database URL set:

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/app"
dinoco migrate generate
```

On PostgreSQL and MySQL, the CLI materializes the desired schema directly in the primary database, inside isolated tables prefixed `dinoco_migration_test_`. Those tables, their foreign keys, and any auxiliary enums are all removed the moment planning finishes — including when planning fails with an error. SQLite validates through a temporary file instead.

> [!WARNING]
> The `dinoco_migration_test_` prefix is reserved by Dinoco. Don't name an application table with that prefix — the migration planner will collide with it. No separate shadow database or shadow URL is required for any adapter.

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

> [!WARNING]
> Rename detection is structural, not something you declare explicitly — the planner infers a `RenameColumn` when it can confidently match an old field to a new one. Always double-check an inferred rename in the generated SQL before applying it; a genuine drop followed by an unrelated add can sometimes look structurally similar to a rename.

## Index migrations

The planner treats standard, unique, and full-text indexes as three separate concerns. `@index`/`@@indexes` emit non-unique B-tree statements. A composite `@@uniques` emits `CREATE UNIQUE INDEX`. `@fulltext`/`@@fulltexts` emit a PostgreSQL GIN index, a MySQL `FULLTEXT` index, and no index at all on SQLite (see [Full-text search](/en-us/docs/orm/orm/full-text-search) for why).

Primary keys (`@id`/`@@ids`) appear in the desired schema's comparison model, but their own constraint already satisfies the index — Dinoco never emits a redundant duplicate `CREATE INDEX` for one. Every foreign key gets its automatic index too, composite relations and both sides of an implicit many-to-many pivot included.

Any change to an index's name, columns, order, or kind produces matching drop/create steps. See [Indexes and constraints](/en-us/docs/orm/guide/indexes) for the schema-level rules behind all of this.

## Dangerous changes

Dropping a populated table or column, narrowing a column's type, removing enum values, and turning a nullable column required can all destroy data or fail outright against existing rows. Dinoco prints a highlighted warning for these and defaults the confirmation prompt to **No** — you have to actively opt in.

The very first migration against a database that already has user tables, but no `dinoco_migrations` table yet, also requires confirmation — this specifically protects a database that wasn't previously managed by Dinoco at all from an unreviewed first migration assuming it owns everything.

> [!NOTE]
> In CI, `DINOCO_CLI_CONFIRM_DESTRUCTIVE=true` can answer the destructive-change prompt non-interactively. Treat this as a privileged, deliberate release-pipeline setting — not a convenience default to set globally just to stop being asked.

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

`migrate generate` always regenerates `dinoco/mod.rs` and `dinoco/models/`, even when the planner finds zero database changes to make — models still get refreshed before the command exits. This keeps the generated Rust output aligned with schema or codegen changes that don't happen to require touching the database at all.
