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

At runtime, reads alternate between configured replicas. When the list is empty, reads use the primary. Writes always use the primary. Use `.read_in_primary()` on `find_first` or `find_many` when a read must observe a recent write immediately.

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
