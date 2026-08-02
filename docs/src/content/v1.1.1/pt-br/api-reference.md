# Referência da API

Esta página é um índice compacto. Os guias específicos explicam comportamento, riscos e exemplos completos.

## Referência do schema

```dinoco
config {
    database = "postgresql" # postgresql | mysql | sqlite
    connection = "direct"  # direct | pgbouncer
    database_url = env("DATABASE_URL")
    read_replicas = [env("DATABASE_REPLICA_URL")]
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

Fields: `@id`, `@unique`, `@index`, `@index(map: "...")`, `@fulltext`, `@default(...)` e `@relation(...)`.

Model: `@@ids([...])`, `@@uniques([...])`, `@@indexes([...])`, `@@fulltexts([...])` e `@@table_name("...")`. Todo model exige exatamente uma declaração de primary key: um `@id` ou um `@@ids`, nunca os dois. Arrays compostos preservam a ordem dos fields.

Toda primary key e foreign key recebe um índice comum automático; a constraint da primary key fornece seu índice sem um `CREATE INDEX` redundante. Full-text só aceita String, e um field não pode participar ao mesmo tempo de uma declaração comum e uma full-text. Defaults: `autoincrement()`, `uuid()`, `snowflake()` e `now()`. Ações: `Cascade`, `Restrict`, `NoAction`, `SetNull` e `SetDefault`.

## API gerada da entity

`#[derive(Entity)]` implementa metadados, conversões nativas de row, helpers `Where`, `OrderBy`, `Update`, `Include` e `Count`, suporte a insert e `new()`.

Uma projeção usa:

```rust
#[derive(Debug, EntityExtend)]
#[extend(User)]
pub struct UserSummary {
    pub id: dinoco::Uuid,
    pub email: String,
}
```

## Methods de leitura

| Builder | Methods | Retorno |
| --- | --- | --- |
| `find_first::<M>()` | `where_`, `where_complex`, `select`, `includes`, `order_by`, `read_in_primary` | `Option<M>` ou `Option<S>` |
| `find_many::<M>()` | os anteriores, `take`, `skip` | `Vec<M>` ou `Vec<S>` |
| `count::<M>()` | `where_`, `includes` | `M::Count` |

Builders de include aceitam `where_`, `where_complex`, `select`, `includes`, `order_by`, `take` e `skip`.
Builders de relações do count aceitam filtros tipados com `where_` e preenchem campos `Option<i64>` em `M::Count`.

## Methods de escrita

| Builder | Obrigatório | Opcional | Retorno |
| --- | --- | --- | --- |
| `insert_into::<M>()` | `values` | `returning::<S>` | `()` ou `S` |
| `insert_many::<M>()` | `values` | `returning::<S>` | `()` ou `Vec<S>` |
| `update::<M>()` | `update` | `where_`, `returning::<S>` | `()` ou `Vec<S>` |
| `update_many::<M>()` | `update` | `where_`, `returning::<S>` | `()` ou `Vec<S>` |
| `find_and_update::<M>()` | `update` | `where_`, `where_complex` | `M` atualizado |
| `delete::<M>()` | `where_` | `returning::<S>` | `()` ou `Vec<S>` |
| `delete_many::<M>()` | nenhum | `where_`, `returning::<S>` | `()` ou `Vec<S>` |

Updates escalares usam `.set(value)`. Entities pivô many-to-many geradas ou explícitas usam `.connect(value)` e `.disconnect(value)` com filtros `eq` ou `batch`.

## Transactions

`Transaction::new()` cria uma lista heterogênea de builders. Adicione operações com `push`, execute com `transactions(transaction).execute(&client).await?` e leia cada retorno por índice usando `TransactionResults::get::<T>` ou `take::<T>`. O macro `transaction![...]` oferece a forma compacta. Toda a batch usa uma conexão primary e faz rollback no primeiro erro.

## Methods de filtro

Todos os scalars: `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `batch`, `null` e `not_null`. Strings: `like`, `starts_with`, `ends_with`. Strings marcadas com `@fulltext` ou incluídas em `@@fulltexts`: `fulltext`. Inteiros e floats: `between`. Ordenação: `asc` e `desc`.

`fulltext` funciona em `find_first`, `find_many`, `find_and_update`, builders one/many de includes, árvores `where_complex` e finds dentro de transactions. Um membro de `@@fulltexts` pesquisa o grupo completo declarado. O method não existe nas outras Strings.

`where_complex(|x, m| ...)` oferece `m.and`, `m.or`, `m.or_many` e `m.not`. Quando usado, todos os `where_` do mesmo builder são ignorados.

## Construtores de adapters

```rust
PostgresAdapter::direct(url).await?
PostgresAdapter::direct_with_pool(url, min_connections, max_connections).await?
PgBouncerAdapter::new(url).await?
MySqlAdapter::new(url)
SqliteAdapter::new(path).await.map_err(anyhow::Error::msg)?
```

Envolva em `Backend::{Postgres, PgBouncer, Mysql, Sqlite}`, passe para `DinocoClient::new`, adicione réplicas com `.with_read_replicas(vec![...])` e habilite o logger SQL com `.with_logger(true)`.

## Tipos de valor

`DinocoValue` suporta null, integer, float, string, enum, boolean, bytes, JSON, UTC date-time e naive date.

Fields gerados usam `String`, `bool`, `i64`, `f64`, `serde_json::Value`, `chrono::DateTime<chrono::Utc>` e `chrono::NaiveDate`. UUID e Snowflake gerados usam `dinoco::Uuid` e `dinoco::Snowflake`.
