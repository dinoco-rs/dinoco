# Dinoco v2.1.0

This page tracks what changed release over release. Each entry links to the page that documents the feature in depth — treat this as a changelog, not the primary reference.

## v2.1.0

> [!WARNING]
> Three changes can need action when upgrading, each described below: Rust 1.95 is now the minimum version, `u64`, `usize`, `u128`, and `i128` convert into `DinocoValue` with `TryFrom` instead of `From`, and `dinoco migrate generate` no longer reads answers from a pipe.

- **The CLI explains what it is about to do.** Every command that touches a database starts with a **Target** section (workspace, database, location with credentials hidden, migrations directory), and `migrate generate` shows the **Migration Plan** with `+`/`~`/`-` markers and a count by kind, the **Data Impact** of destructive changes, and the **Actions** approval will run, listing writing the files and applying them as separate steps. In a terminal, the approval prompt can show the migration SQL first, and it starts on **Cancel** when the plan is destructive. All output is in English. Results go to standard output, while warnings, errors, and progress go to standard error. Colors are used only on terminals and follow `NO_COLOR`, `--no-color`, and `CLICOLOR_FORCE`. See [Output, colors, and errors](/en-us/docs/orm/tooling/cli#output-colors-and-errors).
- **Errors say what to do next.** Each error names what failed, the facts it failed on, why when that is known, and the next step. Connection URLs are always printed with the `user:password@` part and password-like query parameters hidden, including inside driver messages.
- **Non-interactive mode and exit codes.** Without a terminal, with `--non-interactive`, or with `CI` set, the CLI never waits for input. Every decision has an option, and a missing one stops the command with exit code `2` and names the option to pass. Exit codes are `0` (success), `1` (failure), `2` (invalid usage or a decision required), and `130` (cancelled). See [Exit codes](/en-us/docs/orm/tooling/cli#exit-codes).
- Upgrading scripts: an answer piped into `migrate generate` (`echo y | dinoco migrate generate`) is no longer read. Pass `--yes`, or `--allow-destructive` for plans with destructive changes. The `DINOCO_CLI_CONFIRM_MIGRATION`, `DINOCO_CLI_CONFIRM_DESTRUCTIVE`, and `DINOCO_CLI_CONFIRM_DRIFT` variables keep working as before. See [Environment variables from earlier releases](/en-us/docs/orm/tooling/cli#environment-variables-from-earlier-releases).
- **Renames are confirmed, never assumed.** Each rename the planner infers is asked about: keep the data with a rename, or plan a removal plus an addition. Without a terminal, pass `--rename user.username=display_name`, `--no-rename ...`, or `--accept-renames`. A confirmed rename is no longer reported as a destructive change. See [Renames](/en-us/docs/orm/tooling/migrations#renames).
- **Replacements for removed enum values.** When an enum value is removed while rows still use it, `migrate generate` shows how many rows use it and asks which existing value replaces it (`--replace-enum-value Status.Processing=Pending` without a terminal). The migration rewrites those rows before the column stops accepting the old value. This covers MySQL and SQLite; PostgreSQL still refuses enum value removal. See [Removed enum values](/en-us/docs/orm/tooling/migrations#removed-enum-values).
- **`migrate generate --create-only`** writes `up.sql` and `down.sql` without applying them or regenerating the models, for review before `dinoco migrate run`. See [Create a migration without applying it](/en-us/docs/orm/tooling/migrations#create-a-migration-without-applying-it).
- **`dinoco init --database ... --connection ...`** answers the setup questions on the command line.
- **Workspaces are never chosen for you.** A schema with a single workspace uses it, and the Target section shows it. With several workspaces, the CLI asks, listing where each one's database URL comes from, or, without a terminal, asks for `--workspace`.
- **Checked integer conversions.** `DinocoValue` stores integers as `i64`. `From<u64>`, `From<usize>`, `From<u128>`, and `From<i128>` used to wrap or truncate values outside that range without a sign. They are replaced by `TryFrom`, which returns `IntegerOutOfRange`. Upgrading: use `DinocoValue::try_from(value)?`, or convert to `i64` yourself where the value is known to fit. Smaller integer types still use `From`. See [Value types](/en-us/docs/orm/reference/api-reference#value-types).
- **Less work per query.** Every adapter compiles SQL in a single pass into one buffer: compiling a find with five conditions now allocates 2 times instead of 106 to 152, and a 100-row PostgreSQL insert 2 times instead of 2,125. Rows are decoded by column position, results stream into their models, and inserts move values instead of cloning them.
- **Cargo features per database.** `dinoco` has `sqlite`, `postgres`, `mysql`, and `cli` features, all enabled by default. A SQLite-only build compiles 48 crates instead of 213. See [Cargo features](/en-us/docs/orm/orm/clients-adapters#cargo-features).
- **MySQL keeps prepared statements across pooled connections.** Connections are no longer reset when they return to the pool, which saves a round trip per query. Add `?reset_connection=true` to the URL if raw SQL changes session variables. See [MySQL](/en-us/docs/orm/guide/configuration#mysql).
- **Rust 1.95 or newer is required**, for the updated drivers: `rusqlite` 0.40, `deadpool-sqlite` 0.14, `mysql_async` 0.37, and `deadpool-postgres` 0.14.
- The VS Code extension's language server moved from `tower-lsp`, which is no longer maintained, to `tower-lsp-server`. Its features are unchanged.
- Fixed: on MySQL, `migrate generate` failed after applying a schema that uses enums, reporting a missing enum type that MySQL never creates.
- Fixed: on MySQL, migrations failed for database names longer than 46 characters, because the migration lock name exceeded MySQL's 64-character limit.

## v2.0.9

> [!WARNING]
> `transaction(...)` is back to its pre-v2.0.8 shape, and custom errors moved to the new `transaction_with_error(...)`. v2.0.8 closures that end in `Ok::<_, TransactionError>(value)` or return `TransactionError::Custom(...)` need the change described below.

- **`transaction(...)` needs no type annotations again.** Its closure returns `anyhow::Result<T>`, as before v2.0.8, so `Ok(value)` and `anyhow::bail!(...)` work without `Ok::<_, TransactionError>(...)` or an annotated binding, and the result is `Result<T, TransactionError>`. See [Create a transaction](/en-us/docs/orm/orm/transactions#create-a-transaction).
- **`transaction_with_error::<E, _>(...)` for custom errors.** It runs like `transaction`, with a closure that returns `Result<T, TransactionError<E>>`: `Err(TransactionError::Custom(error))` rolls back and reaches the caller typed, next to the database errors. Name the error at the call, `transaction_with_error::<RuntimeError, _>(&client, |tx| async move { ... })`; the `_` is the success type, inferred from `Ok(value)`. The closure can also be an async closure (`async |tx| { ... }`) or an `async fn`. See [Custom errors](/en-us/docs/orm/orm/transactions#custom-errors).
- Upgrading from v2.0.8: replace `transaction` with `transaction_with_error` where the closure returns `Err(TransactionError::Custom(...))` or uses `.map_err(TransactionError::Custom)`. Elsewhere, write `Ok::<_, TransactionError>(value)` as `Ok(value)` again: the closure returns `anyhow::Result<T>`, so the annotated form no longer compiles. Annotated bindings (`let result: Result<_, TransactionError> = ...`) and `return Err(anyhow::anyhow!(...).into())` keep compiling, and the latter can go back to `anyhow::bail!(...)`.

## v2.0.8

> [!WARNING]
> Breaking release. `transaction(...)` closures now return `Result<T, TransactionError<E>>` instead of `anyhow::Result<T>`, and `find_batch(...)`, `QueryMode`, and manual migrations were removed. Each entry below says what to change.

- **Custom errors in transactions.** Return `Err(TransactionError::Custom(error))` from a `transaction(...)` closure to roll back with an error type of your own, then match it next to the database errors: `Err(TransactionError::Custom(RuntimeError::WithoutPermission)) => ...`. `TransactionError` is now `TransactionError<E = Infallible>`, and `?` still converts insert, update, delete, and atomic-update errors and any `anyhow::Error`. See [Custom errors](/en-us/docs/orm/orm/transactions#custom-errors).
- Upgrading a closure: replace `anyhow::bail!(...)` with `return Err(anyhow::anyhow!(...).into())` (or a custom error), and write a final `anyhow::Result` expression as `Ok(expr.await?)`. Where nothing names the custom type, typically `.await?` inside a function returning `anyhow::Result`, name the default once with `Ok::<_, TransactionError>(value)` or `let result: Result<_, TransactionError> = ...`. Code that passes the error to a `fn(TransactionError)` or converts it through `From<TransactionError>` needs no change.
- **`find_batch(...)` and `QueryMode` were removed**, together with `DinocoClient::with_query_mode`/`query_mode`, `config.query_mode`, and the `DinocoJson` decoding behind the single-query mode. Run independent queries with `tokio::try_join!`, which is what the default `BatchQuery` mode did. Run `dinoco models generate` so `dinoco/mod.rs` stops calling `.with_query_mode(...)`.
- **Manual migrations were removed.** Migrations are always generated from `schema.dinoco`, as before v2.0.3. `config.migration_engine`, `dinoco init --migration-engine`, the name argument of `dinoco migrate generate`, `dinoco migrate rollback`, `dinoco migrate status`, `DinocoMigration`, `DinocoManager`, `#[dinoco(migration)]`, `migrate_up`/`migrate_down`/`migration_status`, and the `dinoco/migrations/mod.rs` registry are gone, and the VS Code extension no longer offers `migration_engine`. A schema that still sets `migration_engine` or `query_mode` fails to compile and asks you to delete the entry. In a project that used manual migrations, regenerate models (so `dinoco/mod.rs` drops `pub mod migrations;`) and delete the Rust files in `dinoco/migrations/`; `migrate generate` ignores files there and only reads migration directories. See [Migrations](/en-us/docs/orm/tooling/migrations).

## v2.0.7

- **Relation filters.** Every relation field of a model now exists on its `Where` type, so a query can be narrowed by its related rows without loading them: `find_first::<Transaction>().where_(|x| x.payout.where_(|p| p.method.eq(PayoutMethod::Pix)))` returns only the transactions whose payout is Pix. Besides `where_`, relation fields offer `where_complex`, `none`, `every`, `exists`, and `not_exists`, nest through further relations, and work on every relation shape (many-to-one, one-to-many, both sides of a one-to-one, implicit many-to-many, self relations) and in every builder that takes a `where_` — finds, `count`, `exists`, updates, deletes, include filters, and transactions. Each filter compiles to a correlated `EXISTS` subquery in the same statement. See [Relation filters](/en-us/docs/orm/orm/relation-filters).
- `FindWhere` has a new `Relation` variant (with `RelationMatch`, `RelationJoinTable`, and `RelationQuantifier`); code that matches exhaustively over `FindWhere` needs an arm for it.

## v2.0.6

> [!WARNING]
> Singular relation fields that form a cycle — both sides of a one-to-one, or many-to-ones that lead back to the same model — are now generated as `Option<Box<T>>`. Code that assigns them needs `Some(Box::new(value))`; such models didn't compile before.

- **One-to-one relations generate correctly.** The side without the foreign key (`User.profile`) used to be generated as a `many_to_one` with no keys, so its include loaded nothing and the two structs contained each other by value (`recursive types ... have infinite size`). It is now `#[dinoco(one_to_one, inverse, foreign_key = "...", references = "...")]`: `.includes(...)` loads it from either side, and populating it in `insert_into`/`insert_many` inserts the related row with its foreign key set. See [One-to-one](/en-us/docs/orm/guide/relations#one-to-one).

## v2.0.5

> [!WARNING]
> `dinoco/transform.rs` is now compiled with your application (see below). A transform that imports `dinoco_codegen::prelude::*` must switch to `use dinoco::codegen::*;` before `dinoco/mod.rs` is regenerated, or the application stops compiling.

- **`@updated_at`.** A `DateTime`/`Date` field declared with `@updated_at @default(now())` is set to the database's current UTC time by every `update`, `update_many`, and `find_and_update` that changes the row — in the same `UPDATE`, with no extra bound parameter, including `.returning(...)`, `.pluck(...)`, and transactions. An explicit `.set(...)` on the field wins; relation-only updates leave it alone. The compiler requires `@default(now())` and rejects other types, primary keys, and arguments. The VS Code extension completes, highlights, and documents it. See [Update timestamps](/en-us/docs/orm/guide/defaults-enums#update-timestamps).
- **Transforms only need `dinoco`.** Everything `transform.rs` uses (`DinocoTransformer`, `Model`, `Field`, `ImplMethod`, `quote!`, ...) is re-exported as `dinoco::codegen::*`, so projects no longer depend on `dinoco_codegen`. See [Code transforms](/en-us/docs/orm/guide/transforms#the-transform-file).
- **Editor support for `transform.rs`.** When `dinoco/transform.rs` exists, the generated `dinoco/mod.rs` declares `mod transform;`, so rust-analyzer and `cargo check` analyze it. The CLI still applies it through its own runner. See [Where the transform runs](/en-us/docs/orm/guide/transforms#where-the-transform-runs).
- An optional date field with `@default(now())` (`DateTime?`, `Date?`) now generates `default = Some(...)`; it used to generate code that didn't compile.

## v2.0.4

> [!WARNING]
> `insert_into(...)` and `insert_many(...)` now return `Result<_, CreateError>` instead of `anyhow::Result<_>`, and `CreateError::Constraint { kind, .. }` was replaced by one variant per constraint kind. `?` into `anyhow::Error` keeps working; code that passes the result to a function taking `anyhow::Error` (for example `.map_err(MyError::internal)`) or matches `CreateError::Constraint` needs updating.

- **Typed insert errors.** `CreateError` now has `UniqueViolation`, `ForeignKeyViolation`, `NotNullViolation`, and `CheckViolation` variants carrying the `table`, `constraint`, and `columns` the driver reported, plus `NotReturned` when a returning insert can't read its row back. Helpers: `is_unique_violation()`, `constraint()`, `columns()`, `database_error()`. `DatabaseError::constraint_details()` exposes the same details for any classified error. See [Handle insert errors](/en-us/docs/orm/orm/insert#handle-insert-errors).
- **Reads inside transactions.** `find_first` and `find_many` accept the transaction context (`.execute(tx)`), including `.includes(...)`, `.pluck(...)`, and `.transform(...)`. They run on the transaction's connection and see its uncommitted writes. See [Transactions](/en-us/docs/orm/orm/transactions#read-inside-a-transaction).
- **Test ambient.** `create_test_ambient()` returns a client over a fresh in-memory SQLite database with the whole schema already created (whatever `config.database` is), one isolated database per call. `setup_test_methods::<M>(&client)` installs `on_insert`/`on_update`/`on_delete`/`on_find` callbacks for one model that receive the result, the compiled query, and the error of every operation on it (inside transactions and `.includes(...)` too); `remove_test_methods::<M>(&client)` removes them. See [Testing](/en-us/docs/orm/orm/testing).
- **Generated `connect()` uses the test ambient under `cfg(test)`.** `dinoco/mod.rs` now also exports `connect_test()` (the test ambient for the crate's schema and workspace) and `connect_database()` (the real connection). `connect()` returns `connect_test()` in the crate's own tests and `connect_database()` everywhere else. Regenerate models with `dinoco models generate` to get them. See [Testing](/en-us/docs/orm/orm/testing#the-generated-connect-in-tests).
- **Saved schema per workspace.** `migrate generate` and `models generate` with a workspace now copy `schema.dinoco` and every file it imports into `dinoco/migrations/<workspace>/schema/`, keeping the folder layout. See [Saved schema per workspace](/en-us/docs/orm/guide/configuration#saved-schema-per-workspace).
- Writes inside a transaction now report their real affected-row count internally (they used to report `0`).
- MySQL error 1364 (a `NOT NULL` column with no value and no default) is now classified as a `NotNullViolation`.

## v2.0.3

> [!WARNING]
> `config.custom_derives` was **removed**. A schema that still declares it fails to compile with a pointer to code transforms. Replace each entry with a hook in `dinoco/transform.rs`; see [Migrate from custom_derives](/en-us/docs/orm/guide/transforms-recipes#migrate-from-custom-derives).

- **Manual migrations.** New `config.migration_engine = "automatic" | "manual"` (default `automatic`, so existing projects are unchanged). With `manual`, each migration is a Rust type implementing `DinocoMigration` (`#[dinoco(migration)]`, `up`/`down` on a `DinocoManager` that exposes every operation the automatic engine supports), registered in `dinoco/migrations/mod.rs`. `dinoco migrate generate <name>` scaffolds one, `migrate run` applies pending ones, and the new `migrate rollback` and `migrate status` revert and list them. `dinoco init --migration-engine manual` starts a project this way. Removed in v2.0.8.
- **Code transforms.** `dinoco/transform.rs` exposes a `transformer()` implementing `DinocoTransformer`, applied by `models generate` and `migrate generate`. It can add derives, attributes (on types, fields, variants, and relations), imports, inherent methods, and trait impls, for both structs and enums. See [Code transforms](/en-us/docs/orm/guide/transforms).
- **`where_complex(...)` on `count::<M>()`.** `count` now accepts the same `AND`/`OR` composition as the find builders (`exists::<M>()` already did). See [Count](/en-us/docs/orm/orm/count#complex-filters).
- **Tooling.** The VS Code extension knows `migration_engine` (completion, hover, validation).
- **Docs for AI and search.** The site now serves an MCP server (`/mcp`), `llms.txt`/`llms-full.txt`, richer page metadata (per-page titles, hreflang, Open Graph images, JSON-LD), and a sitemap with alternates. See [Use with AI](/en-us/docs/orm/guide/ai).

## v2.0.2

> [!NOTE]
> A point release: several additive query-builder capabilities. The schema language and generated code shape are unchanged outside the new opt-in `config.query_mode` setting — existing projects keep compiling.

- **`exists::<M>()`.** Checks whether any row matches, compiling to `SELECT EXISTS(...)` and returning a plain `bool` without materializing a row. See [Query overview](/en-us/docs/orm/orm/find#checking-existence).
- **`.pluck(...)`.** Projects onto a single column and returns its raw values directly (`Vec<T>`/`Option<T>`/`T`, depending on the builder) instead of a full row model. Available on `find_many`, `find_first`, `update`, `update_many`, `insert_into`, `insert_many`, `delete`, and `delete_many`. See [Find many](/en-us/docs/orm/orm/find-many#pluck-a-single-column), [Update](/en-us/docs/orm/orm/update#pluck-a-single-column-back), [Insert](/en-us/docs/orm/orm/insert#pluck-a-single-column-back), and [Delete](/en-us/docs/orm/orm/delete#pluck-a-single-column-back).
- **`.transform(...)`.** Applies a plain Rust closure to the already-fetched row(s) — a post-query mapping, not a SQL projection. Available on `find_many`, `find_first`, and after `.returning::<S>()` on `update`, `update_many`, `insert_into`, `insert_many`, `delete`, and `delete_many`. See [Find many](/en-us/docs/orm/orm/find-many#transform-results).
- **`.connect_batch(...)`/`.disconnect_batch(...)`.** Link or unlink several many-to-many targets in one round trip instead of one `.connect(...)`/`.disconnect(...)` call per value — collapses into a single multi-row `INSERT`/IN-list `DELETE`. See [Relations](/en-us/docs/orm/guide/relations#connectdisconnect-several-endpoints-at-once).
- **`find_batch(...)`.** Runs a tuple of 2 to 8 independent `find_many`/`find_first` builders and returns a matching tuple of results. The new `QueryMode` (`BatchQuery`, the default; or `SingleQuery`, which combines every item into one JSON-aggregated round trip) is configurable per client via `.with_query_mode(...)` or from `schema.dinoco` via `config.query_mode`. Removed in v2.0.8.

## v2.0.1

> [!NOTE]
> A point release: one additive query-builder capability plus the tooling work below. The schema language and the CLI are unchanged, and no generated code changes shape — existing projects keep compiling.

- **Filter implicit many-to-many by the other side.** The generated virtual `Option<Id>` keys (`system.business_id`) now work as `where_(...)` inputs, not just as `connect`/`disconnect`/insert targets. `find_many::<System>().where_(|system| system.business_id.eq(&business_id))` returns only the systems linked to that business, compiled as a membership subquery over the pivot. The whole `Field` filter surface applies to the pivot's target column — `eq`/`neq`, `gt`/`gte`/`lt`/`lte`, `batch`/`not_in`, `null`/`not_null`, `like`/`starts_with`/`ends_with`, `between` — it composes with scalar filters and `where_complex`, and `count::<T>()` honours it. See [relations](/en-us/docs/orm/guide/relations#filter-by-the-other-side).
- **Configurable formatter.** The VS Code extension's formatter now accepts `dinoco.formatter.maxWidth`, `dinoco.formatter.useTabs`, `dinoco.formatter.useSpaces`, `dinoco.formatter.indentSize`, and `dinoco.formatter.removeComments`. `useTabs`/`useSpaces` are mutually exclusive and kept in sync automatically. See [VS Code extension](/en-us/docs/orm/tooling/vscode#formatting).
- **Real semantic highlighting.** The language server now emits semantic tokens derived from the same index used for hover and completion, so a model name is colored differently depending on whether it's a declaration or a reference — something a regex-based grammar can't do reliably.
- **Sharper syntax highlighting.** `//` comments, the `Restrict`/`NoAction` referential actions, and the core field attributes (`@id`, `@unique`, `@relation`, `@default`, `@index`, `@fulltext`) all get their own scopes now, instead of falling back to generic ones.

## v1.3.3

- Fixed repeated relations in nested `includes` trees: an entity reached through two different relation paths is now hydrated independently on every adapter. See [includes](/en-us/docs/orm/orm/includes).
- Clarified nullable-field filters: `field.null()` / `field.not_null()` generate `IS NULL` / `IS NOT NULL`; an untyped `None` passed to `.eq(...)` isn't supported, since Rust can't infer the inner type there. See [filters](/en-us/docs/orm/orm/filters).
- Every singular relation navigation field is now required to be optional (`fee Fee?`) in the schema, independently of whether its local foreign key is required. See [relations](/en-us/docs/orm/guide/relations).
- The compiler and language server support circular imports safely — each file is parsed and consolidated once, so bidirectional relations can live in separate files. See [schema organization](/en-us/docs/orm/guide/schema-organization).
- Added database-side atomic numeric updates (`increment`/`decrement`/`multiply`/`divide`) and typed atomic-mutation/transaction errors to `find_and_update`. See [find and update](/en-us/docs/orm/orm/find-and-update) and [transactions](/en-us/docs/orm/orm/transactions).
- Added `config.imports` for loading whole child schema files without repeating every symbol, alongside the existing named `import { ... } from "..."`. See [schema organization](/en-us/docs/orm/guide/schema-organization).
- Added `config.custom_derives` to apply extra Rust derives to generated enums and model structs.
- `Enum?` fields now compile as `Option<Enum>` end to end, including defaults and `NULL` decoding.
- Named relations are fully supported for multiple foreign keys targeting the same model. See [relations](/en-us/docs/orm/guide/relations).
- Generated enums derive `Clone, Copy, PartialEq`; generated models derive `Clone`, and `Copy` when every field is copyable.
- Bidirectional enum ↔ string conversion (`.to_string()` / `TryFrom<&str>` / `FromStr`) using the schema's original values.
- Implicit many-to-many relations no longer generate a public pivot entity — instead, each side gets a write-only virtual foreign key (`business.system_id`) used to `connect`/`disconnect` or to link a row during insert. See [relations](/en-us/docs/orm/guide/relations#implicit-many-to-many).
- Added named, per-environment database configurations under `config.workspace`, selected with `--workspace`/`-w`. See [configuration](/en-us/docs/orm/guide/configuration#workspaces).
- Added opt-in embedded SQLite migrations via `dinoco::migrate(&client)`, for applications that want to apply migrations from the binary instead of the CLI.
- Generated enums and models derive `serde::Serialize`/`Deserialize` through Dinoco's own re-export.
- Verified `Send`-future compatibility for every builder and the transaction context, for multithreaded frameworks like Axum.
- Added `@index`, `@@indexes([...])`, and `@@uniques([...])` for explicit and composite indexes; every primary key and foreign key is indexed automatically. See [indexes and constraints](/en-us/docs/orm/guide/indexes).
- Added `@fulltext` and `@@fulltexts([...])` full-text search, with a native index on PostgreSQL and MySQL and a portable fallback on SQLite. See [full-text search](/en-us/docs/orm/orm/full-text-search).
- Added the closure transaction API (`dinoco::transaction(&client, |tx| async move { ... })`) with automatic commit/rollback and typed errors. See [transactions](/en-us/docs/orm/orm/transactions).
- Added `where_complex` for explicit `AND`/`OR`/`NOT` grouping. See [where complex](/en-us/docs/orm/orm/where-complex).

## v1.2.0

- Generated enums can be passed by value or reference to every filter and query builder.
- `DateTime<Utc>`, `NaiveDate`, and `serde_json::Value` accept both owned and borrowed values in filters and updates; date/datetime fields gained `.between(...)`.
- Fixed PostgreSQL `DateTime<Utc>` serialization to match the actual column type (`TIMESTAMP` vs. `TIMESTAMPTZ`).
- Added an upgrade path from the legacy migration model: `dinoco migrate generate` imports existing history and legacy tables (including case-sensitive identifiers) without deleting data.
- Fixed enum handling in `find_and_update`/`update`/`update_many` to use each database's native enum support.
- `migrate generate` now shows the detected changes and asks for confirmation before creating or applying a migration.

## Earlier releases

The v1.1 series introduced the workspace, runtime migration, Serde, transaction, relation, index, and query-builder foundations that the releases above build on.
