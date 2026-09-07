# Referência da API

Esta página é um índice compacto da sintaxe do schema e da superfície de API gerada — ela prioriza completude e escaneabilidade em vez de explicação. Para comportamento, tradeoffs e exemplos trabalhados, siga os links para o guia específico de cada tópico.

## Referência do schema

```dinoco
config {
    database          = "postgresql" # postgresql | mysql | sqlite
    connection        = "direct"     # direct | pgbouncer (só PostgreSQL)
    database_url      = env("DATABASE_URL")
    read_replicas     = [env("DATABASE_REPLICA_URL")]
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

**Scalars:** `String`, `Boolean`, `Integer`, `Float`, `DateTime`, `Date`, `Json`. Adicione `?` para um scalar ou field de navegação opcional, `[]` para uma lista de relação. Fields de navegação de relação singular sempre exigem `?`, independentemente de sua foreign key local ser obrigatória ou opcional. Veja [Models e fields](/pt-br/docs/orm/guide/models) e [Relações](/pt-br/docs/orm/guide/relations).

**Atributos de field:** `@id`, `@unique`, `@index`, `@index(map: "...")`, `@fulltext`, `@default(...)`, `@relation(...)`.

**Atributos de model:** `@@ids([...])`, `@@uniques([...])`, `@@indexes([...])`, `@@fulltexts([...])`, `@@table_name("...")`. Todo model exige exatamente uma declaração de primary key — um `@id` ou um `@@ids`, nunca os dois. Arrays compostos preservam a ordem de field declarada. Veja [Índices e constraints](/pt-br/docs/orm/guide/indexes).

Toda primary key e foreign key ganha um índice comum automático; a própria constraint da primary key fornece seu índice sem um `CREATE INDEX` redundante. Um field full-text precisa ser `String`/`String?`, e um field nunca pode pertencer ao mesmo tempo a uma declaração comum e uma full-text.

**Funções de default:** `autoincrement()`, `uuid()`, `snowflake()`, `now()`. Veja [Defaults e enums](/pt-br/docs/orm/guide/defaults-enums).

**Ações referenciais:** `Cascade`, `Restrict`, `NoAction`, `SetNull`, `SetDefault`.

## API gerada da entity

`#[derive(Entity)]` implementa metadados de tabela, conversão nativa de row por adapter, helpers tipados `Where`/`OrderBy`/`Update`/`Include`/`Count`, metadados de inserção, e:

```rust
pub fn new(required_field: RequiredType, ...) -> Self
```

`#[derive(EntityExtend)]` constrói uma projeção em vez de uma entity completa:

```rust
#[derive(Debug, EntityExtend)]
#[extend(User)]
pub struct UserSummary {
    pub id: dinoco::Uuid,
    pub email: String,
}
```

## Methods de leitura

| Builder | Methods encadeáveis | Retorno de `execute` |
| --- | --- | --- |
| `find_first::<M>()` | `where_`, `where_complex`, `select`, `includes`, `order_by`, `read_in_primary` | `Option<M>` ou `Option<S>` |
| `find_many::<M>()` | `where_`, `where_complex`, `select`, `includes`, `order_by`, `take`, `skip`, `read_in_primary` | `Vec<M>` ou `Vec<S>` |
| `count::<M>()` | `where_`, `includes` | `M::Count` |

Builders de include suportam `where_`, `where_complex`, `select`, `includes`, `order_by`, `take` e `skip`. Builders de relação do count suportam filtros tipados com `where_` e preenchem fields `Option<i64>` em `M::Count`.

## Methods de escrita

| Builder | Cadeia obrigatória | Cadeia opcional | Retorno |
| --- | --- | --- | --- |
| `insert_into::<M>()` | `values` | `returning::<S>` | `()` ou `S` |
| `insert_many::<M>()` | `values` | `returning::<S>` | `()` ou `Vec<S>` |
| `update::<M>()` | `update` | `where_`, `returning::<S>` | `()` ou `Vec<S>` |
| `update_many::<M>()` | `update` | `where_`, `returning::<S>` | `()` ou `Vec<S>` |
| `find_and_update::<M>()` | `update` | `where_`, `where_complex` | `M` atualizado |
| `delete::<M>()` | `where_` | mais `where_`, `returning::<S>` | `()` ou `Vec<S>` |
| `delete_many::<M>()` | nenhuma | `where_`, `returning::<S>` | `()` ou `Vec<S>` |

Updates de field escalar sempre passam por `.update(|x| x.field.set(value))`.

> [!WARNING]
> `delete` impõe seu `where_` no nível de tipo — o builder não tem `.execute()` até você chamar `.where_(...)` ao menos uma vez. `delete_many` e `update_many` não têm essa proteção; uma chamada sem filtro em qualquer um dos dois afeta toda linha da tabela, e compila normalmente.

## Writes many-to-many implícitos

Relações many-to-many implícitas expõem um `Option<Id>` virtual write-only nos dois endpoints. Um field preenchido passado para `insert_into` cria o endpoint, depois seu vínculo na pivô, nessa ordem:

```rust
let mut tag = Tag::new("documentation".to_string());
tag.task_id = Some(task.id.clone());

dinoco::insert_into::<Tag>().values(&tag).execute(&client).await?;
```

`insert_many` avalia o field virtual de forma independente em cada item — `Some(id)` cria uma linha de pivô para aquele item, `None` não cria nenhuma:

```rust
for tag in &mut tags {
    tag.task_id = Some(task.id.clone());
}

dinoco::insert_many::<Tag>().values(&tags).execute(&client).await?;
```

Para endpoints que já existem, use `.connect(value)`/`.disconnect(value)` a partir de `update`, `update_many` ou `find_and_update` em vez disso. O field virtual é excluído das listas de colunas de `INSERT`/`SELECT` reais do endpoint, e sempre volta `None` na leitura. As duas formas rodam dentro do contexto de transaction por closure como qualquer outra mutation. Veja [Relações](/pt-br/docs/orm/guide/relations#many-to-many-implicito).

## Transactions

`transaction(&client, |tx| async move { ... }).await` abre uma transaction nativa presa a uma única conexão física com a primary. Rode toda mutation dentro dela com `.execute(tx)`. Um `Ok(value)` retornado pela closure faz commit e produz esse valor; qualquer erro desfaz tudo. `TransactionError` diferencia falhas de create, update, delete, atomic update, commit e rollback, mantendo o erro original do driver acessível por baixo. Veja [Transactions](/pt-br/docs/orm/orm/transactions).

## Methods de filtro

| Tipo de field | Methods |
| --- | --- |
| Todo field escalar | `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `batch`, `null`, `not_null` |
| `String`/`Option<String>` | `like`, `starts_with`, `ends_with` |
| Membro de `@fulltext` ou `@@fulltexts` | `fulltext` |
| `Integer`/`Float` (filtro) | `between` |
| `Integer`/`Float` (update, opcionais inclusos) | `increment`, `decrement`, `multiply`, `divide` |
| Ordenação | `asc`, `desc` |

`fulltext` funciona em `find_first`, `find_many`, `find_and_update`, builders de include one/many, e dentro de árvores `where_complex` — inclusive quando `find_and_update` roda por um contexto de transaction. Chamá-lo em qualquer membro de um grupo `@@fulltexts` pesquisa o grupo declarado inteiro, não só aquele field. O method simplesmente não existe numa `String` sem `@fulltext`.

`where_complex(|x, m| ...)` fornece `m.and`, `m.or`, `m.or_many` e `m.not`. No momento em que é usado, todo `where_` comum nesse mesmo builder é ignorado. Veja [Filtros](/pt-br/docs/orm/orm/filters) e [Where complex](/pt-br/docs/orm/orm/where-complex).

## Construtores de adapters

```rust
PostgresAdapter::direct(url).await?
PostgresAdapter::direct_with_pool(url, min_connections, max_connections).await?
PgBouncerAdapter::new(url).await?
MySqlAdapter::new(url)
SqliteAdapter::new(path).await.map_err(anyhow::Error::msg)?
```

Envolva em `Backend::{Postgres, PgBouncer, Mysql, Sqlite}` e passe para `DinocoClient::new`. Anexe réplicas com `.with_read_replicas(vec![...])`, e opte pelo log de queries SQL com `.with_logger(true)`. Veja [Clients e adapters](/pt-br/docs/orm/orm/clients-adapters).

## Tipos de valor

A camada de parâmetros do runtime, `DinocoValue`, suporta valores null, integer, float, string, enum, boolean, bytes, JSON, UTC date-time e naive date.

Fields gerados aparecem como `String`, `bool`, `i64`, `f64`, `serde_json::Value`, `chrono::DateTime<chrono::Utc>` e `chrono::NaiveDate`. Identificadores UUID e Snowflake gerados usam `dinoco::Uuid` e `dinoco::Snowflake` respectivamente, em vez de uma `String`/`i64` pura — veja [Models e fields](/pt-br/docs/orm/guide/models#tipos-escalares).
