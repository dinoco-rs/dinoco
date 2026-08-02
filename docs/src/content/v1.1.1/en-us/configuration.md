# Configuration

The `config` block tells the CLI and code generator which SQL dialect and connection strategy the project uses. Keep it at the top of `dinoco/schema.dinoco` so a reader can understand the project before reading its models.

## Configuration block

```dinoco
config {
    database = "postgresql"
    connection = "direct"
    database_url = env("DATABASE_URL")
    read_replicas = [env("DATABASE_REPLICA_1"), env("DATABASE_REPLICA_2")]
}
```

`database` accepts `postgresql`, `mysql`, or `sqlite`. `connection` is relevant to PostgreSQL and accepts `direct` or `pgbouncer`; when omitted, it defaults to Direct.

## Workspaces

Use `workspace` when the same model schema needs more than one database configuration:

```dinoco
config {
    workspace {
        dev {
            database = "sqlite"
            database_url = env("DEV_DATABASE_URL")
        }

        prod {
            database = "postgresql"
            connection = "pgbouncer"
            database_url = env("PROD_DATABASE_URL")
        }
    }
}
```

Each workspace contains a complete configuration, including its own optional `read_replicas`. Do not mix top-level database properties with a `workspace` block. Select one with `--workspace name` or `-w name`; when the option is omitted, the CLI prompts for a workspace. Migration files are isolated under `dinoco/migrations/<workspace>/`.

The `workspace` block must contain at least one named workspace, and each named workspace must declare both `database` and `database_url`.

## Environment variables

`database_url`, every `read_replicas` item, and `snowflake_node_id` accept only `env("NAME")`. A literal URL is rejected during compilation. This keeps credentials out of a committed schema.

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/app"
export DATABASE_REPLICA_1="postgres://reader:secret@replica-1:5432/app"
```

An environment variable is resolved by the CLI when it connects and by the generated `dinoco::connect()` function at runtime.

## PostgreSQL

Use Direct for a regular PostgreSQL URL and Dinoco's pooled PostgreSQL adapter:

```dinoco
config {
    database = "postgresql"
    connection = "direct"
    database_url = env("DATABASE_URL")
    read_replicas = []
}
```

Choose PgBouncer when your URL points to a PgBouncer endpoint:

```dinoco
config {
    database = "postgresql"
    connection = "pgbouncer"
    database_url = env("DATABASE_URL")
    read_replicas = []
}
```

Both modes use the PostgreSQL SQL compiler. The difference is connection and statement handling, not schema syntax.

## Query logging and Direct pool

`with_logger` enables SQL and parameter output from generated clients. It defaults to `false` and can be declared in a regular config or inside each workspace:

```dinoco
config {
    database = "postgresql"
    connection = "direct"
    database_url = env("DATABASE_URL")
    with_logger = true
    min_connection = 2
    max_connection = 10
}
```

`min_connection` and `max_connection` are available only for PostgreSQL Direct. Their defaults are `2` and `10`; both must be positive integers and the minimum cannot exceed the maximum. Dinoco eagerly opens the configured minimum and caps the pool at the configured maximum. Query parameters may contain application data, so enable logging only where that output is appropriate.

## MySQL

MySQL has one connection mode:

```dinoco
config {
    database = "mysql"
    database_url = env("DATABASE_URL")
    read_replicas = []
}
```

A typical URL is `mysql://user:password@localhost:3306/database`.

## SQLite

For SQLite, `DATABASE_URL` is a file path. Relative paths are resolved from the Dinoco project folder, so this keeps the database alongside the schema when desired.

```dinoco
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
    read_replicas = []
}
```

```bash
export DATABASE_URL="database.sqlite"
```

## Read replicas

Declare zero or more replica URLs through environment variables:

```dinoco
read_replicas = [env("DATABASE_REPLICA_1"), env("DATABASE_REPLICA_2")]
```

The generated `connect()` resolves and constructs every replica adapter declared by the selected workspace. At runtime, finds alternate between configured replicas. When the list is empty, reads use the primary. Writes always use the primary. Use `.read_in_primary()` on `find_first` or `find_many` when a read must observe a recent write immediately. `find_and_update` is a write and always runs on the primary.

CLI migration commands never use replicas. `migrate generate` and `migrate run` connect only to the selected workspace's primary `database_url`; replicas are expected to follow the primary through database-level replication.

## Snowflake IDs

A schema using `@default(snowflake())` must also declare a node ID:

```dinoco
config {
    database = "postgresql"
    database_url = env("DATABASE_URL")
    read_replicas = []
    snowflake_node_id = env("SNOWFLAKE_NODE_ID")
}
```

```bash
export SNOWFLAKE_NODE_ID="7"
```

Give each concurrently running node a distinct ID. Reusing one node ID across processes can compromise the uniqueness guarantee of generated Snowflakes.
