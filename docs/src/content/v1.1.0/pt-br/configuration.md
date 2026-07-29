# Configuração

O bloco `config` informa à CLI e ao codegen qual dialeto SQL e estratégia de conexão o projeto usa.

## Bloco de configuração

```dinoco
config {
    database = "postgresql"
    connection = "direct"
    database_url = env("DATABASE_URL")
    read_replicas = [env("DATABASE_REPLICA_1")]
}
```

`database` aceita `postgresql`, `mysql` ou `sqlite`. Para PostgreSQL, `connection` aceita `direct` ou `pgbouncer` e assume Direct quando não informado.

## Variáveis de ambiente

`database_url`, cada item de `read_replicas` e `snowflake_node_id` aceitam somente `env("NOME")`. O compiler rejeita valores literais.

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/app"
export DATABASE_REPLICA_1="postgres://reader:secret@replica:5432/app"
```

O `database_url` é resolvido pela CLI e pelo `connect()` gerado. Para configurar réplicas no runtime, construa os adapters declarados e use `with_read_replicas`.

## PostgreSQL

Direct usa o pool PostgreSQL comum:

```dinoco
config {
    database = "postgresql"
    connection = "direct"
    database_url = env("DATABASE_URL")
    read_replicas = []
}
```

PgBouncer aponta para o endpoint do proxy:

```dinoco
connection = "pgbouncer"
```

Os dois usam o mesmo compiler PostgreSQL; muda a estratégia de conexão, não o schema.

## MySQL

```dinoco
config {
    database = "mysql"
    database_url = env("DATABASE_URL")
    read_replicas = []
}
```

Uma URL típica é `mysql://usuario:senha@localhost:3306/banco`.

## SQLite

```dinoco
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
    read_replicas = []
}
```

Para SQLite, a URL é o caminho do arquivo. Caminhos relativos partem da pasta do projeto Dinoco.

```bash
export DATABASE_URL="database.sqlite"
```

## Réplicas de leitura

O `DinocoClient` intercala finds entre os backends de réplica em round-robin. Sem réplicas, lê no primary. Writes sempre usam o primary, e `.read_in_primary()` força um find e seus includes a lerem nele.

## IDs Snowflake

`snowflake()` exige um node ID vindo do ambiente:

```dinoco
config {
    database = "postgresql"
    database_url = env("DATABASE_URL")
    read_replicas = []
    snowflake_node_id = env("SNOWFLAKE_NODE_ID")
}
```

Cada processo concorrente deve usar um ID distinto; repetir o mesmo node ID pode quebrar a garantia de unicidade.
