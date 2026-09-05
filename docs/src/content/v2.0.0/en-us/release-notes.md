# Dinoco v2.0.0

This page tracks what changed release over release. Each entry links to the page that documents the feature in depth — treat this as a changelog, not the primary reference.

## v2.0.0

> [!NOTE]
> This is a tooling-focused release. The schema language, the generated client API, and the CLI are unchanged from v1.3.3 — everything below is about the editing experience.

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
