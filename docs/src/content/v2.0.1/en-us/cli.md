# CLI reference

The `dinoco` binary is the entry point for every project workflow — running it interactively locally, in CI, or in a deploy pipeline. Run it from the Cargo project root, since paths like `dinoco/schema.dinoco` are always resolved relative to the current directory.

## dinoco init

```bash
dinoco init
```

Walks you through picking PostgreSQL, MySQL, or SQLite; PostgreSQL adds a second prompt for Direct or PgBouncer. Creates `dinoco/migrations/` and a formatted starter `dinoco/schema.dinoco` with `database_url = env("DATABASE_URL")` already wired up.

> [!NOTE]
> If a schema already exists at that path, `init` leaves it untouched and prints a warning instead of overwriting your work — it's always safe to re-run.

For scripted setup — CI, a project template, a container entrypoint — the same prompts can be answered non-interactively:

```bash
DINOCO_CLI_INIT_DATABASE=postgresql \
DINOCO_CLI_INIT_POSTGRES_CONNECTION=direct \
dinoco init
```

## dinoco migrate generate

```bash
dinoco migrate generate
```

This is the complete development-loop command: compile and validate the schema, inspect the live database, plan the change, ask for confirmation, generate and apply the migration, and finally regenerate the Rust models — all in one call.

With workspaces, target one explicitly with `dinoco migrate generate --workspace dev` (or `-w dev`); the migration then lands under `dinoco/migrations/dev/`. Leave the flag off and the CLI prompts you to pick a workspace interactively.

Required environment for this command: whatever variable `database_url` names, plus `SNOWFLAKE_NODE_ID` (or whatever the schema names instead) if any field uses `snowflake()`. On PostgreSQL and MySQL, dialect validation happens in isolated tables reserved under the `dinoco_migration_test_` prefix, inside the same database — there's no separate shadow database to provision.

## dinoco migrate run

```bash
dinoco migrate run
```

Applies every pending `up.sql` in directory order and records each one in `dinoco_migrations`. This is the command you want in a deploy pipeline, run only after the generated migration files have already gone through review — it never plans a new migration on its own.

Use `--workspace name`/`-w name` to apply only that specific workspace's pending migrations.

## dinoco models generate

```bash
dinoco models generate
```

Compiles and validates the schema, then rebuilds the entire generated Rust module tree — without connecting to a database or touching migrations at all. Reach for this specifically after switching branches, or any time only the generated code is stale relative to a schema that hasn't actually changed the database.

This command also accepts `--workspace name`/`-w name`. Switching which workspace you generate for removes the previously generated tree first, then rebuilds it fresh for the newly selected configuration — the two workspaces' generated code never mixes.

## Recommended workflow

```bash
# One time, when setting up the project
dinoco init

# After every schema change
dinoco migrate generate

# When deploying to another environment
dinoco migrate run

# When only the generated Rust code is stale
dinoco models generate
```

> [!TIP]
> The CLI automatically loads a local `.env` file when one is present. Commit a safe `.env.example` documenting which variables a fresh clone needs to set — never commit the real `.env` with actual credentials in it.
