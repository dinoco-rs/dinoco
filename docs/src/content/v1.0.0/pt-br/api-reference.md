# Referência da API

Esta página é um índice compacto. Os guias específicos explicam comportamento, riscos e exemplos completos.

## Referência do schema

```dinoco
config {
    database = "postgresql" # postgresql | mysql | sqlite
    connection = "direct"  # direct | pgbouncer
    database_url = env("DATABASE_URL")
    read_réplicas = [env("DATABASE_REPLICA_URL")]
    snowflake_node_id = env("SNOWFLAKE_NODE_ID")
}

enum Role {
    USER
    ADMIN
}

model User {
    id   Integer @id @default(autoincrement())
    role Role    @default(USER)
}
```

Scalars: `String`, `Boolean`, `Integer`, `Float`, `DateTime`, `Date` e `Json`. `?` torna opcional e `[]` representa lista de relação.

Fields: `@id`, `@unique`, `@default(...)` e `@relation(...)`. Model: `@@ids([...])` e `@@table_name("...")`. Defaults: `autoincrement()`, `uuid()`, `snowflake()` e `now()`. Ações: `Cascade`, `Restrict`, `NoAction`, `SetNull` e `SetDefault`.

## API gerada da entity

`#[derive(Entity)]` implementa metadados, conversões nativas de row, helpers `Where`, `OrderBy`, `Update`, `Include` e `Count`, suporte a insert e `new()`.

Uma projeção usa:

```rust
#[derive(Debug, EntityExtend)]
#[extend(User)]
pub struct UserSummary {
    pub id: String,
    pub email: String,
}
```

## Methods de leitura

| Builder | Methods | Retorno |
| --- | --- | --- |
| `find_first::<M>()` | `where_`, `select`, `includes`, `order_by`, `read_in_primary` | `Option<M>` ou `Option<S>` |
| `find_many::<M>()` | os anteriores, `take`, `skip` | `Vec<M>` ou `Vec<S>` |
| `count::<M>()` | `where_`, `includes`, `count` | `M::Count` |

Builders de include aceitam `where_`, `select`, `includes`, `order_by`, `take` e `skip`.

## Methods de escrita

| Builder | Obrigatório | Opcional | Retorno |
| --- | --- | --- | --- |
| `insert_into::<M>()` | `values` | `returning::<S>` | `()` ou `S` |
| `insert_many::<M>()` | `values` | `returning::<S>` | `()` ou `Vec<S>` |
| `update::<M>()` | `update` | `where_`, `returning::<S>` | `()` ou `Vec<S>` |
| `update_many::<M>()` | `update` | `where_`, `returning::<S>` | `()` ou `Vec<S>` |
| `find_and_update::<M>()` | `update` | `where_` | `M` atualizado |
| `delete::<M>()` | `where_` | `returning::<S>` | `()` ou `Vec<S>` |
| `delete_many::<M>()` | nenhum | `where_`, `returning::<S>` | `()` ou `Vec<S>` |

Updates escalares usam `.set(value)`. Pivots many-to-many explícitos usam `.connect(value)` e `.disconnect(value)` com filtros `eq` ou `batch`.

## Methods de filtro

Todos os scalars: `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `batch`, `null` e `not_null`. Strings: `like`, `starts_with`, `ends_with`. Inteiros e floats: `between`. Ordenação: `asc` e `desc`.

## Construtores de adapters

```rust
PostgresAdapter::direct(url).await?
PgBouncerAdapter::new(url).await?
MySqlAdapter::new(url)
SqliteAdapter::new(path).await.map_err(anyhow::Error::msg)?
```

Envolva em `Backend::{Postgres, PgBouncer, Mysql, Sqlite}`, passe para `DinocoClient::new` e adicione réplicas com `.with_read_réplicas(vec![...])`.

## Tipos de valor

`DinocoValue` suporta null, integer, float, string, enum, boolean, bytes, JSON, UTC date-time e naive date.

Fields gerados usam `String`, `bool`, `i64`, `f64`, `serde_json::Value`, `chrono::DateTime<chrono::Utc>` e `chrono::NaiveDate`. UUID e Snowflake gerados usam `dinoco::Uuid` e `dinoco::Snowflake`.
