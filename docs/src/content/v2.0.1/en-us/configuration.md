# Configuration

The `config` block tells the CLI and the code generator which SQL dialect to target and how to reach the database. It's the first thing Dinoco reads, and it's the only place in a schema where secrets-adjacent values (connection strings, replica URLs) are allowed to appear — and even there, only as references to environment variables, never as literals.

> [!TIP]
> Keep `config` at the top of `dinoco/schema.dinoco`. A reader opening the file for the first time should be able to tell what database it talks to before they get to the first `model`.

## Configuration block

```dinoco
config {
    database       = "postgresql"
    connection     = "direct"
    database_url   = env("DATABASE_URL")
    read_replicas  = [env("DATABASE_REPLICA_1"), env("DATABASE_REPLICA_2")]
}
```

- `database` — one of `"postgresql"`, `"mysql"`, or `"sqlite"`. This is the only setting that changes which SQL dialect gets generated.
- `connection` — only meaningful for PostgreSQL; `"direct"` or `"pgbouncer"`. Defaults to `"direct"` when omitted.
- `database_url` — always `env("NAME")`, never a literal string. See [Environment variables](#environment-variables) below.
- `read_replicas` — an optional array of `env("NAME")` entries. See [Read replicas](#read-replicas).

## Schema file imports

A large schema doesn't have to live in one file. The root `dinoco/schema.dinoco` can pull in whole child files without re-listing every model and enum they declare:

```dinoco
config {
    imports      = ["entities/accounts.dinoco", "entities/businesses.dinoco"]
    database     = "postgresql"
    database_url = env("DATABASE_URL")
}
```

Every model and enum declared directly in a listed file becomes visible to `schema.dinoco` as if it had been declared there. A few rules keep this predictable:

- Paths are relative to the main schema file and must resolve to real `.dinoco` files.
- The same path can't appear twice, and import cycles are rejected.
- `imports` is a config-level setting: with workspaces, it lives directly inside `config`, not inside an individual `workspace { ... }` block.
- Only the **root** `schema.dinoco` can use `config.imports`. A child file that itself needs something from elsewhere uses an explicit named import instead:

```dinoco
import { AccountStatus } from "../enums.dinoco"

model Account {
    id     String        @id @default(uuid())
    status AccountStatus
}
```

> [!NOTE]
> A child file only sees its own declarations plus whatever it explicitly imports — it does **not** inherit the root file's `config.imports` scope. This is deliberate: it keeps the entry point compact while keeping every dependency between child files visible at its point of use, instead of implicit through a shared global scope. See [Schema organization](/en-us/docs/orm/guide/schema-organization) for multi-file project layouts, scoping rules, and a complete example.

## Custom derives

`custom_derives` attaches additional Rust derive macros to every generated enum or model struct, project-wide:

```dinoco
config {
    database       = "sqlite"
    database_url   = env("DATABASE_URL")
    custom_derives = [
        {
            into   = "enum"
            derive = "ZodSchema"
            import = "use zod_rs::prelude::*;"
        },
        {
            into   = "struct"
            derive = "Validate"
            import = "use validator::Validate;"
        }
    ]
}
```

Each entry needs all three fields: `into` is `"enum"` or `"struct"`, `derive` is a valid Rust path, and `import` is a single-line `use ...;` statement bringing that path into scope. The crate providing the derive is still your application's dependency to add — Dinoco only wires the annotation and the `use` statement into the generated code. Keep `custom_derives` at the top level of `config`, not inside a workspace. See [Schema organization](/en-us/docs/orm/guide/schema-organization#custom-derives) for exactly what codegen emits.

## Workspaces

Reach for `workspace` when one schema needs to run against more than one database configuration — typically a local SQLite database for development and a real PostgreSQL instance in production:

```dinoco
config {
    workspace {
        dev {
            database     = "sqlite"
            database_url = env("DEV_DATABASE_URL")
        }

        prod {
            database     = "postgresql"
            connection   = "pgbouncer"
            database_url = env("PROD_DATABASE_URL")
        }
    }
}
```

Each named workspace is a **complete**, independent configuration — including its own optional `read_replicas` — and every workspace must declare at least `database` and `database_url`.

> [!WARNING]
> A `config` block is either flat database settings **or** a `workspace` block, never both. Mixing top-level `database`/`database_url` with a `workspace { ... }` is rejected by the compiler.

Select a workspace with `--workspace dev`/`-w dev` on `migrate generate`, `migrate run`, and `models generate`. Omit the flag and the CLI prompts you interactively. Each workspace's migrations live in their own directory, `dinoco/migrations/<workspace>/`, so `dev` and `prod` histories never collide.

## Environment variables

`database_url`, every entry of `read_replicas`, and `snowflake_node_id` accept **only** `env("NAME")` — a literal string in any of these positions is a compile error, not a warning. This isn't just a style preference: it's what keeps a schema safe to commit to version control.

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/app"
export DATABASE_REPLICA_1="postgres://reader:secret@replica-1:5432/app"
```

The named variable is resolved twice, independently: once by the CLI when it connects to plan or apply a migration, and once by the generated `dinoco::connect()` function at runtime. Both need the variable set in their respective environment.

## PostgreSQL

Use `connection = "direct"` for a regular PostgreSQL connection string:

```dinoco
config {
    database     = "postgresql"
    connection   = "direct"
    database_url = env("DATABASE_URL")
}
```

Use `connection = "pgbouncer"` when `DATABASE_URL` actually points at a PgBouncer endpoint rather than PostgreSQL directly:

```dinoco
config {
    database     = "postgresql"
    connection   = "pgbouncer"
    database_url = env("DATABASE_URL")
}
```

Both modes share the same SQL compiler and the same generated query API — the difference is entirely in connection and statement handling, never in schema syntax or the shape of generated code.

## Query logging and Direct pool

`with_logger` makes the generated client print the SQL and bound parameters for every query it runs. It defaults to `false`, and can be set either at the top level or inside an individual workspace:

```dinoco
config {
    database       = "postgresql"
    connection     = "direct"
    database_url   = env("DATABASE_URL")
    with_logger    = true
    min_connection = 2
    max_connection = 10
}
```

`min_connection` and `max_connection` only apply to PostgreSQL Direct. They default to `2` and `10`, must both be positive integers, and `min_connection` can't exceed `max_connection`. Dinoco opens the configured minimum eagerly at startup and never grows the pool past the configured maximum.

> [!WARNING]
> Logged query parameters can contain application data — user emails, tokens embedded in a filter, anything a query touches. Only enable `with_logger` in environments where that's acceptable, and never leave it on by default in production.

## MySQL

MySQL has a single connection mode — there's no Direct/PgBouncer distinction to make:

```dinoco
config {
    database     = "mysql"
    database_url = env("DATABASE_URL")
}
```

A typical connection string looks like `mysql://user:password@localhost:3306/database`.

## SQLite

For SQLite, `DATABASE_URL` is a file path rather than a network address. Relative paths resolve from the Dinoco project folder, which is a convenient way to keep the database file next to the schema during development:

```dinoco
config {
    database     = "sqlite"
    database_url = env("DATABASE_URL")
}
```

```bash
export DATABASE_URL="database.sqlite"
```

## Read replicas

```dinoco
read_replicas = [env("DATABASE_REPLICA_1"), env("DATABASE_REPLICA_2")]
```

The generated `connect()` resolves and constructs an adapter for every replica the active workspace declares. At runtime, `find_first`/`find_many` reads alternate across them round-robin; when the list is empty, reads simply go to the primary. Two things never touch a replica, by design:

- **Writes.** Every insert, update, and delete always executes on the primary.
- **A read that opts out.** Call `.read_in_primary()` on `find_first`/`find_many` when a read needs to observe a write that just happened — replication lag would otherwise make that read unreliable. `find_and_update` is itself a write, so it always runs on the primary regardless.

> [!NOTE]
> CLI migration commands never use replicas. `migrate generate` and `migrate run` connect only to the active workspace's primary `database_url`; replicas are expected to catch up on their own through the database's own replication mechanism.

## Snowflake IDs

A schema that uses `@default(snowflake())` anywhere must also declare where the node ID comes from:

```dinoco
config {
    database          = "postgresql"
    database_url      = env("DATABASE_URL")
    snowflake_node_id = env("SNOWFLAKE_NODE_ID")
}
```

```bash
export SNOWFLAKE_NODE_ID="7"
```

> [!DANGER]
> Every concurrently running process that generates Snowflakes must use a **distinct** node ID. Two processes sharing one node ID can generate colliding IDs under load — this is the one configuration mistake in this page that silently corrupts data instead of failing loudly.
