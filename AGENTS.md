# AGENTS.md

Dinoco is a schema-driven Rust ORM for SQLite, PostgreSQL and MySQL. A `.dinoco` schema is compiled, turned into Rust models, and queried through typed builders. The repository also ships the `dinoco` CLI (migrations and code generation), a VS Code extension, and the documentation site.

## Project structure

```
.
├── Cargo.toml              Workspace (resolver 3): every crate below, tests, examples, benchmarks
├── crates/
│   ├── dinoco/             Public ORM crate
│   │   └── src/
│   │       ├── methods/    One builder per operation: find_many, find_first, find_and_update, count,
│   │       │               exists, insert_into, insert_many, update, update_many, delete, delete_many
│   │       ├── fields.rs   Typed filter fields and relation filters (where_, none, every, *_complex, exists)
│   │       ├── relation.rs Include loaders (HasMany / BelongsTo), per-parent take/skip
│   │       ├── count.rs, insert.rs, update.rs, read.rs, projection.rs, order_by.rs
│   │       ├── transaction.rs, hooks.rs, error.rs, testing.rs (TestAmbient)
│   │       └── main.rs     `dinoco` binary, forwards to dinoco_cli
│   ├── dinoco_engine/      Query model and execution
│   │   └── src/
│   │       ├── query.rs    FindQuery, FindWhere, RelationMatch, WhereComplex, ...
│   │       ├── value.rs    DinocoValue
│   │       ├── decode.rs, pluck.rs, traits.rs, transaction.rs, hooks.rs, error.rs
│   │       └── backends/   sql.rs (shared single-pass SQL writer, `SqlWriter`) and one module per
│   │                       dialect: sqlite/, postgres/, mysql/ (adapter + migration compiler)
│   ├── dinoco_derives/     Proc macros: Entity, EntityExtend, Extend, DinocoEnum
│   ├── dinoco_compiler/    `.dinoco` schema parser (parser.rs -> ast.rs) and resolver/validation
│   ├── dinoco_codegen/     Rust model generation from a compiled schema, transforms
│   ├── dinoco_formatter/   Canonical formatter for `.dinoco` files
│   └── dinoco_cli/         `dinoco` CLI: commands/ (init, migrate, models, review), db.rs (migration
│                           state), ui.rs, prompt.rs, error.rs, runner.rs
├── vscode/                 Language server in Rust (server, completion, diagnostics, document,
│                           workspace) + TypeScript client (src/extension.ts); prebuilt binaries in bin/
├── tests/                  `dinoco_tests` crate: integration suites for every crate (see Tests)
├── benchmarks/             `dinoco_benchmarks` crate (see Benchmarks)
├── docs/                   Next.js documentation site (see Documentation)
├── examples/               axum, actix-web and tower services exposing the same API on SQLite
├── scripts/publish-crates.js  Publishes the crates in dependency order (`--plan`, `--dry-run`)
├── readme.md, crates.md    Repository README and crates.io README; keep them in sync
└── rustfmt.toml            Edition 2024, max_width 120
```

How a query flows through the crates: a schema is parsed and validated by `dinoco_compiler`, `dinoco_codegen` writes models that derive `Entity` (`dinoco_derives`), the builders in `dinoco` produce `dinoco_engine` query structs (`FindQuery`, `FindWhere`, ...), the dialect's `DinocoSqlCompiler` writes SQL through `backends/sql.rs`, and the adapter runs it and decodes rows by column position.

Ignore these when reading the tree: `target/`, the empty `file:` directories (left by SQLite URLs, gitignored), `vscode/bin/` and `vscode/*.vsix` (build outputs).

### Tests

- Suites under `tests/tests/<crate>/<topic>.rs` must be registered as a `[[test]]` in `tests/Cargo.toml`, named `<crate>_<topic>` (for example `dinoco_relation_filters`). Files directly under `tests/tests/` are discovered automatically and named after the file.
- Shared helpers (`column`, `primary`, `nullable`, `create_table`, ...) live in `tests/src/lib.rs`. Schema fixtures live in `tests/fixtures/`.
- Database suites run every scenario on all three backends through an `on_every_backend!` macro (see `tests/tests/dinoco/relation_filters.rs`). PostgreSQL and MySQL come from local servers: `postgres://postgres:postgres@localhost:5432/postgres` and `mysql://root:root@localhost:3306/mysql` (Podman containers `postgres` and `mysql`).
- `version_consistency` checks that every manifest, lockfile entry, README and docs directory agrees on the release version.

### Benchmarks

- `benches/runtime.rs` (Criterion): `compile/*` measures SQL compilation alone, `<dialect>/*` measures end-to-end operations through the public API, and `<dialect>/raw_driver_*` is the bare-driver floor that Dinoco's overhead is compared against.
- `src/bin/report.rs`: type layouts, **future sizes**, heap allocations and bytes per operation, and peak live heap, counted by a global allocator.
- `src/bin/sql_corpus.rs`: prints the SQL and parameters for a broad corpus of queries and migrations in every dialect. Diff two revisions to prove a compiler change is byte-for-byte neutral.
- Environment: `DINOCO_BENCH_DIALECTS=sqlite,postgres,mysql`, `DINOCO_BENCH_POSTGRES_URL`, `DINOCO_BENCH_MYSQL_URL` (they default to the test servers). Unreachable servers are skipped and reported.

### Documentation

- One version tree only, renamed in place on each release: `docs/src/content/v<X>/{en-us,pt-br}/*.md`, navigation in `docs/src/jsons/versions/v<X>/{en-us,pt-br}.json` + `index.ts`, registered in `docs/src/jsons/versions.ts`.
- Routes are `/<locale>/docs/orm/<group>/<shortName>`, with groups `guide`, `orm`, `tooling` and `reference`.
- Every navigation item's `inPage` must list the page's `##`–`######` headings exactly. Anchors drop accents (`docs/src/lib/to-anchor-id.ts`).
- `release-notes.md` starts with `# Dinoco v<X>` and has one `## v<X>` section per release, newest first.

## Adding a feature

Every new feature or behavior change goes through all of these steps. None is optional.

1. **Implement it** following the performance rules below.
2. **Document it** in both `en-us` and `pt-br`:
   - the page that owns the feature, with examples;
   - `api-reference.md` and any summary that lists the same API (for example `filters.md`);
   - an entry in `release-notes.md` under the current version's section;
   - `inPage` in the navigation JSONs if headings were added or renamed.
3. **Test everything that can be tested.** Cover every backend for database behavior, and every relation shape when relations are involved. Cover edge cases: empty input, `NULL`, no related rows, nesting, combining with `where_complex`, use inside transactions, and every builder that accepts the new API. Also cover error paths and the generated types. Register new test files in `tests/Cargo.toml`.
4. **Run the benchmarks** before and after the change (see Commands), and report the numbers:
   - `sql_corpus` diff for anything that touches SQL compilation; output must be identical unless the change is meant to alter SQL;
   - `report` for allocations and future sizes of the affected operations;
   - Criterion `runtime` for timings;
   - add a benchmark case when the feature introduces a new hot path.
5. **Validate**:
   - docs with the existing scripts;
   - **only the tests you added or affected**, not the whole suite;
   - `rustfmt --check` on the files you touched;
   - `cargo check --workspace --all-targets` with no new warnings.

## Performance and memory rules

Performance and low memory use are requirements, for async code (futures) and sync code alike.

**Allocate less**
- Write SQL in one pass into one buffer (`SqlWriter` in `backends/sql.rs`). Don't build intermediate `String`s, and avoid `format!`/`to_string` on hot paths.
- Move values instead of cloning them. Borrow where possible: table and field names are `&'static str`.
- Pre-size collections with `with_capacity` when the length is known, and avoid intermediate `Vec`/`HashMap`s that only get iterated once.
- Decode rows by column position, never by name lookup.

**Keep futures small**
- Every local held across an `.await` lives in the future's state. Don't keep large values alive across awaits, and drop or scope them before awaiting.
- Move rare or heavy branches into a separately boxed future so their state stays out of the common path. Example: relation updates in `crates/dinoco/src/methods/update.rs`:
  ```rust
  // Relation updates are rare; awaiting them boxed keeps their state out of
  // every update's future.
  if !connects.is_empty() || !disconnects.is_empty() {
      Box::pin(async move { /* ... */ }).await?;
  }
  ```
- Type-erased and recursive loaders return `Pin<Box<dyn Future<Output = ...> + Send + 'a>>` (`IncludeLoaderFuture` in `relation.rs`, `CountLoaderFuture` in `count.rs`, `InsertNestedFuture` in `insert.rs`).
- Check `report`'s future-size section. A change must not grow an existing future without a measured reason.

**Box only when it pays off**
- Box rare paths, large or rarely used enum variants, and recursive futures.
- Never box the common hot path: that adds one allocation per call. Measure both versions when in doubt.

**Sync code**
- Same rules: no needless clones, iterators over collected intermediates, no allocation inside loops.
- Keep core types small (`FindWhere`, `DinocoValue`, query structs). `report` prints their layouts, so watch for size regressions.
- Keep builders cheap: `&'static str` names, `PhantomData` markers, and generic `FnOnce` callbacks instead of boxed closures.

## Commands

```sh
# Type-check everything
cargo check --workspace --all-targets

# Format check on touched files only (`cargo fmt --check` already fails on untouched files in examples/ and vscode/)
rustfmt --edition 2024 --check <files>

# Run one test target, optionally filtered by scenario
cargo test -p dinoco_tests --test dinoco_relation_filters one_to_many
cargo test -p dinoco_tests --test sqlite_methods

# Benchmarks: run each on the baseline revision (e.g. a git worktree of HEAD) and on your change, then compare
cargo run -p dinoco_benchmarks --release --bin sql_corpus > after.sql   # diff against the baseline output
cargo run -p dinoco_benchmarks --release --bin report
cargo bench -p dinoco_benchmarks --bench runtime -- --save-baseline before   # on the baseline
cargo bench -p dinoco_benchmarks --bench runtime -- --baseline before        # on the change
cargo bench -p dinoco_benchmarks --bench runtime -- compile/                 # compilation only

# Docs (must run from docs/, paths resolve from the working directory)
cd docs && node scripts/validate-content.mjs
# check-anchors.mjs is hardcoded to v2.0.0; run a patched copy for the current version
cd docs && sed 's/v2\.0\.0/v<X>/g' scripts/check-anchors.mjs > scripts/.check-anchors.mjs \
  && node scripts/.check-anchors.mjs; rm scripts/.check-anchors.mjs
```

- New links must not add anchor errors. Two broken anchors already exist, both in `update.md`.
- MySQL timings are noisy. Compare MySQL numbers only with interleaved A/B runs.

## Conventions

- Rust edition 2024, MSRV 1.95 (`rust-version` in every published crate). Default features of `dinoco`/`dinoco_engine` are `sqlite`, `postgres`, `mysql` and `cli`. Backend-specific code is gated by those features.
- Code, comments, doc comments and CLI output are in English. Docs are bilingual (`en-us` and `pt-br`).
- Match the surrounding code: naming, comment density, and doc comments on public API.
- Using `where_complex` on a builder discards every plain `where_` on that builder. This is documented behavior and must be kept.
- Don't commit unless asked.

## Releasing a version

A version bump touches all of these:
- every crate's `Cargo.toml`, including the `=<X>` pin on `dinoco_derives` in `crates/dinoco/Cargo.toml`;
- `tests/Cargo.toml` and `vscode/Cargo.toml`;
- `Cargo.lock` (refreshed by any cargo build);
- `vscode/package.json` and the root entries of `vscode/package-lock.json`;
- `readme.md` and `crates.md`;
- the docs directories, renamed with `git mv`;
- `versions.ts` (import name `v<X_Y_Z>` and fallback), `index.ts`, the navigation `contentPath`s and `validate-content.mjs`;
- version mentions in the introduction, quickstart, clients-adapters, vscode and cookbook pages;
- a new `## v<X>` section in the release notes;
- the constants in `tests/tests/version_consistency.rs`.

A blanket replace on the navigation JSONs also rewrites the previous version's heading in the release notes' `inPage`. Put it back.
