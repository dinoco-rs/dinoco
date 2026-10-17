# CLI reference

The `dinoco` binary is the entry point for every project workflow, whether you run it in a terminal, in CI, or in a deploy pipeline. Run it from the Cargo project root: paths like `dinoco/schema.dinoco` are always resolved relative to the current directory.

## Global options

These options work with every command:

| Option | Effect |
| --- | --- |
| `--non-interactive` | Never prompt. A command that needs a decision stops with exit code `2` and names the option that provides it. |
| `--no-color` | Disable colored output. The `NO_COLOR` environment variable does the same. |
| `-v`, `--version` | Print the version. |
| `-h`, `--help` | Print help for the command, with examples. |

## Interactive and non-interactive use

The CLI only asks questions when standard input and standard error are both terminals, `--non-interactive` was not passed, and the `CI` environment variable is not set. Otherwise it never waits for input. It takes every decision from a command-line option, and when an option is missing it stops with exit code `2` and an error that names the option to pass:

```text
error: This migration needs approval.

help: Review the plan above, then pass --yes to approve it.
```

Answers written to a pipe (`echo y | dinoco migrate generate`) are not read. Pass the matching option instead.

The CLI never picks a workspace, a rename, or an enum replacement on your behalf. When one of those is needed and nobody can be asked, the command fails before it writes or applies anything.

## Output, colors, and errors

Results go to standard output: section headings, aligned fields, and planned changes marked `+` (addition), `~` (modification), and `-` (removal). Warnings, errors, and progress go to standard error, so redirecting standard output captures only the results.

Colors are used only when the stream is a terminal. `NO_COLOR` and `--no-color` turn them off, and `CLICOLOR_FORCE=1` turns them on for redirected output. Redirected output contains no ANSI escape codes unless you force them.

Errors follow one shape: what failed, the facts it failed on, why (when that is known), and what to do next.

```text
error: Unable to connect to the database.

  Database:  PostgreSQL
  Location:  postgresql://***@127.0.0.1:5432/app

Caused by:
  Failed to create the configured minimum PostgreSQL connections
  Error occurred while creating a new object: error connecting to server
  error connecting to server
  Connection refused (os error 61)

help: Check that the database is running and reachable from this machine.
help: Check that the connection URL and its credentials are correct.
```

The lines under `Caused by:` come from the database driver, unchanged except for hidden credentials.

Connection URLs are always printed with their credentials hidden: the `user:password@` part and password-like query parameters (`password`, `pass`, `pwd`, `sslpassword`, `token`, `secret`) are replaced by `***`.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Success, including "no changes were found". |
| `1` | The operation failed: a connection, schema, SQL, or file error. |
| `2` | The command line is invalid, or a decision is required and no option provided it. |
| `130` | You cancelled a prompt (Esc or Ctrl+C). Nothing was changed. |

## dinoco init

```bash
dinoco init
dinoco init --database postgresql --connection direct
```

Creates `dinoco/migrations/` and a formatted starter `dinoco/schema.dinoco` with `database_url = env("DATABASE_URL")` already wired up.

| Option | Values |
| --- | --- |
| `--database` | `postgresql`, `mysql`, or `sqlite`. Asked for when omitted; required without a terminal. |
| `--connection` | `direct` or `pgbouncer`. PostgreSQL only. Asked for when omitted; `direct` without a terminal. |

> [!NOTE]
> If a schema already exists at that path, `init` leaves it untouched and says so instead of overwriting your work. Re-running it is always safe.

The `DINOCO_CLI_INIT_DATABASE` and `DINOCO_CLI_INIT_POSTGRES_CONNECTION` environment variables still answer the same two questions, for scripts written for earlier releases.

## dinoco migrate generate

```bash
dinoco migrate generate
```

This is the complete development-loop command. It compiles and validates the schema, inspects the live database, shows the plan, asks for approval, writes and applies the migration, and regenerates the Rust models.

The output starts with the **Target**: the workspace (when there is one), the database, its location with credentials hidden, and the migrations directory. Every later step uses exactly that target. Then come the **Migration Plan**, with one line per change and a count by kind, the **Data Impact** of destructive changes, and the **Actions** that approval will run. Generating the files and applying them are listed as separate steps.

In a terminal, the approval prompt offers to proceed, to show the migration SQL first, or to cancel. When the plan contains destructive changes, the prompt starts on **Cancel**, so pressing Enter alone never approves data loss.

### Options

| Option | Effect |
| --- | --- |
| `-w`, `--workspace <NAME>` | Workspace to use. See [Workspaces](#workspaces). |
| `--schema-from <NAME>` | Apply the schema saved for workspace `NAME` instead of `dinoco/schema.dinoco`. See [Workspaces](#workspaces). |
| `-y`, `--yes` | Approve the plan without asking. Plans with destructive changes need `--allow-destructive`. |
| `--allow-destructive` | Approve the plan, including changes that drop or rewrite data. Implies `--yes`. |
| `--create-only` | Write the migration files without applying them. |
| `--accept-renames` | Confirm every rename the planner inferred. |
| `--rename <MAPPING>` | Confirm one inferred rename: `TABLE.OLD=NEW` for a column, `OLD=NEW` for a table. Repeatable. |
| `--no-rename <MAPPING>` | Plan an inferred rename as a removal plus an addition instead. Same format. Repeatable. |
| `--replace-enum-value <MAPPING>` | Move rows off a removed enum value: `ENUM.VALUE=REPLACEMENT`. Repeatable. |
| `--repair-drift` | Approve repairing schema drift (changes made outside Dinoco migrations). |
| `--adopt-baseline` | Approve adopting existing tables that already match the schema as the migration baseline. |

A `--rename` that matches no inferred rename, a malformed mapping, or a replacement value that is not allowed fails with exit code `2`. A typo is never ignored.

See [Migrations](/en-us/docs/orm/tooling/migrations) for how renames and enum replacements are planned.

### Create the migration without applying it

```bash
dinoco migrate generate --create-only --yes
```

`--create-only` plans against the live database and writes `up.sql` and `down.sql`, but it does not apply the migration, record it, or regenerate the models. Review the SQL, then run `dinoco migrate run` to apply it and `dinoco models generate` to update the Rust code.

`--create-only` only works when the migration is the whole change. When the database first needs a drift repair, a baseline adoption, or an upgrade of migration history from an earlier release, those change the database, so the command stops with exit code `2` and asks you to run it without `--create-only`.

### In CI

```bash
dinoco migrate generate --non-interactive --yes \
  --rename user.username=display_name \
  --replace-enum-value Status.Processing=Pending
```

Required environment: whatever variable `database_url` names, plus `SNOWFLAKE_NODE_ID` (or whatever the schema names instead) if any field uses `snowflake()`. PostgreSQL and MySQL plans are computed from the schema without creating any table; SQLite builds the desired schema in a temporary database file that is deleted afterwards.

## dinoco migrate run

```bash
dinoco migrate run
```

Applies every pending `up.sql` in directory order and records each one in `dinoco_migrations`, then reports how many were applied, or that the database is already up to date. This is the command for a deploy pipeline, run after the generated migration files went through review. It never plans a new migration and never prompts, except to pick a workspace.

Use `--workspace name`/`-w name` to apply one workspace's pending migrations.

## dinoco models generate

```bash
dinoco models generate
```

Compiles and validates the schema, then rebuilds the generated Rust module tree, without connecting to a database or touching migrations. If `dinoco/transform.rs` exists, it is applied to the output (see [Code transforms](/en-us/docs/orm/guide/transforms)); the same happens in `migrate generate`. Use it after switching branches, or whenever only the generated code is stale.

This command also accepts `--workspace name`/`-w name`, and `--schema-from name` to generate the models from the schema saved for another workspace. Switching the workspace you generate for removes the previously generated tree first, then rebuilds it for the newly selected configuration, so two workspaces' generated code never mixes.

## Workspaces

When the schema defines workspaces, every command needs to know which one to use:

- `--workspace name` selects it. An unknown name fails with exit code `2` and lists the defined workspaces.
- When the schema defines a single workspace, it is used, and the **Target** section shows it.
- Otherwise the CLI asks which workspace to apply the migrations to, listing each one with the environment variable its database URL comes from. Without a terminal, the command fails with exit code `2` and asks for `--workspace`.

`migrate generate` and `models generate` then ask which schema to use, when more than one workspace is defined and another workspace already has a [saved schema](/en-us/docs/orm/guide/configuration#saved-schema-per-workspace): `dinoco/schema.dinoco`, the schema you edit, or the copy saved for another workspace, such as dev's when you target prod. A copy identical to `dinoco/schema.dinoco` is marked as such. The **Target** section shows the schema that was picked.

- `--schema-from name` answers that question: `dinoco migrate generate -w prod --schema-from dev` applies dev's schema to prod. A name that isn't a workspace, or a workspace without a saved schema yet, fails with exit code `2`.
- Without a terminal and without `--schema-from`, `dinoco/schema.dinoco` is used.
- When the schema comes from another workspace, it is saved as the selected workspace's copy once the command succeeds. Only the models and enums come from the copy: the database settings still come from the selected workspace in `dinoco/schema.dinoco`, and that file is never changed.

## Environment variables from earlier releases

These variables keep working, so existing scripts don't break. New scripts should use the options instead.

| Variable | Equivalent |
| --- | --- |
| `DINOCO_CLI_CONFIRM_MIGRATION=true` | Approves the plan as shown, destructive changes and inferred renames included, like `--allow-destructive --accept-renames`. Removed enum values that rows still use are left unchanged, with a warning. |
| `DINOCO_CLI_CONFIRM_DESTRUCTIVE=true` | The same, for plans with destructive changes. |
| `DINOCO_CLI_CONFIRM_DRIFT=true` | `--repair-drift --adopt-baseline`. |
| `DINOCO_CLI_INIT_DATABASE`, `DINOCO_CLI_INIT_POSTGRES_CONNECTION` | `init --database`, `init --connection`. |

## Recommended workflow

```bash
# One time, when setting up the project
dinoco init

# After every schema change
dinoco migrate generate

# When deploying to another environment
dinoco migrate run

# When another workspace should get the schema dev was generated with
dinoco migrate generate --workspace prod --schema-from dev

# When only the generated Rust code is stale
dinoco models generate
```

> [!TIP]
> The CLI loads a local `.env` file when one is present. Commit a safe `.env.example` documenting which variables a fresh clone needs, and never commit the real `.env` with credentials in it.
