# Clients e adapters

`DinocoClient` possui um `Backend` primary e, opcionalmente, vários backends de réplica. Os builders recebem `&DinocoClient`, então as queries não tomam a posse do client.

## Conexão gerada

`dinoco/mod.rs` exporta `connect()`, que lê a variável configurada em `database_url` e cria o adapter primary:

```rust
mod dinoco;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = dinoco::connect().await?;
    Ok(())
}
```

Construa o client manualmente para controlar adapters ou adicionar réplicas.

## PostgreSQL Direct

```rust
use dinoco_engine::{Backend, DinocoClient, PostgresAdapter};

let adapter = PostgresAdapter::direct(
    "postgres://postgres:postgres@localhost:5432/app"
).await?;
let client = DinocoClient::new(Backend::Postgres(adapter));
```

Direct usa o pool PostgreSQL e tem construtor async.

## PostgreSQL com PgBouncer

```rust
use dinoco_engine::{Backend, DinocoClient, PgBouncerAdapter};

let adapter = PgBouncerAdapter::new(
    "postgres://app:secret@pgbouncer:6432/app"
).await?;
let client = DinocoClient::new(Backend::PgBouncer(adapter));
```

Direct e PgBouncer compartilham o mesmo `SqlCompiler` PostgreSQL.

## MySQL

```rust
use dinoco_engine::{Backend, DinocoClient, MySqlAdapter};

let adapter = MySqlAdapter::new(
    "mysql://app:secret@localhost:3306/app"
);
let client = DinocoClient::new(Backend::Mysql(adapter));
```

## SQLite

```rust
use dinoco_engine::{Backend, DinocoClient, SqliteAdapter};

let adapter = SqliteAdapter::new("dinoco/database.sqlite")
    .await
    .map_err(anyhow::Error::msg)?;
let client = DinocoClient::new(Backend::Sqlite(adapter));
```

## Réplicas de leitura

```rust
let primary = PostgresAdapter::direct(primary_url).await?;
let replica_a = PostgresAdapter::direct(replica_a_url).await?;
let replica_b = PostgresAdapter::direct(replica_b_url).await?;

let client = DinocoClient::new(Backend::Postgres(primary))
    .with_read_réplicas(vec![
        Backend::Postgres(replica_a),
        Backend::Postgres(replica_b),
    ]);
```

As leituras alternam entre réplicas com um índice round-robin lock-free. Um vetor vazio faz as leituras usarem o primary.

## Regras de execução

- Insert, update e delete sempre usam o primary.
- Finds, includes e counts usam o caminho de leitura.
- `.read_in_primary()` força um find e seus includes no primary.
- Cada adapter converte sua row nativa diretamente.

Crie o client uma vez e reutilize-o durante a aplicação. Recriar pools por request adiciona conexão ao hot path.
