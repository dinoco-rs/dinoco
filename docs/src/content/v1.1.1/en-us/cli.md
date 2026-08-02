# CLI reference

The `dinoco` binary is the project workflow entry point. Run commands from the Cargo project root; paths such as `dinoco/schema.dinoco` are resolved from the current directory.

## dinoco init

```bash
dinoco init
```

The interactive command asks for PostgreSQL, MySQL, or SQLite. PostgreSQL adds a second prompt for Direct or PgBouncer. It creates `dinoco/migrations/` and a formatted `dinoco/schema.dinoco` with `database_url = env("DATABASE_URL")`.

If the schema already exists, the command preserves it and prints a warning instead of overwriting work.

For automated setup, the prompts also understand:

```bash
DINOCO_CLI_INIT_DATABASE=postgresql \
DINOCO_CLI_INIT_POSTGRES_CONNECTION=direct \
dinoco init
```

## dinoco migrate generate

```bash
dinoco migrate generate
```

This is the complete development command: compile and validate the schema, inspect the database, plan and confirm changes, generate and apply the migration, then generate Rust models.

With workspaces, use `dinoco migrate generate --workspace dev` or `dinoco migrate generate -w dev`. The migration is written under `dinoco/migrations/dev/`. When the name is omitted, the CLI prompts for a workspace.

Required environment:

- `DATABASE_URL` (or the variable named in the schema).
- `SNOWFLAKE_NODE_ID` when the schema uses `snowflake()`.

PostgreSQL/MySQL dialect validation uses isolated tables with the reserved `dinoco_migration_test_` prefix in the primary database. It does not require a separate shadow URL.

## dinoco migrate run

```bash
dinoco migrate run
```

Applies every pending `up.sql` in directory order and records it in `dinoco_migrations`. Use this in deployment after the migration files have passed review.

Use `--workspace name` or `-w name` to apply only that workspace's migrations.

## dinoco models generate

```bash
dinoco models generate
```

Compiles and validates the local schema, then recreates the Rust module tree without connecting to the database or creating a migration. Use it after switching branches or while iterating on application code that needs the latest generated types.

This command also accepts `--workspace name` or `-w name`. When the selected workspace changes, code generation removes the previously generated tree before recreating it with the selected configuration.

## Recommended workflow

```bash
# One time
dinoco init

# After editing the schema
dinoco migrate generate

# On another environment
dinoco migrate run

# When only generated Rust is stale
dinoco models generate
```

The CLI loads a local `.env` file when present. Keep secrets out of `schema.dinoco` and version-control a safe `.env.example`, not the real `.env`.
