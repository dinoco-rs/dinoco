# Dinoco v2.0.4

This page tracks what changed release over release. Each entry links to the page that documents the feature in depth — treat this as a changelog, not the primary reference.

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

- **Manual migrations.** New `config.migration_engine = "automatic" | "manual"` (default `automatic`, so existing projects are unchanged). With `manual`, each migration is a Rust type implementing `DinocoMigration` (`#[dinoco(migration)]`, `up`/`down` on a `DinocoManager` that exposes every operation the automatic engine supports), registered in `dinoco/migrations/mod.rs`. `dinoco migrate generate <name>` scaffolds one, `migrate run` applies pending ones, and the new `migrate rollback` and `migrate status` revert and list them. `dinoco init --migration-engine manual` starts a project this way. See [Manual migrations](/en-us/docs/orm/guide/manual-migrations).
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
- **`find_batch(...)`.** Runs a tuple of 2 to 8 independent `find_many`/`find_first` builders and returns a matching tuple of results. The new `QueryMode` (`BatchQuery`, the default; or `SingleQuery`, which combines every item into one JSON-aggregated round trip) is configurable per client via `.with_query_mode(...)` or from `schema.dinoco` via `config.query_mode`. See [Query overview](/en-us/docs/orm/orm/find#batch-several-queries-together) and [Configuration](/en-us/docs/orm/guide/configuration#find_batch-execution-mode).

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
