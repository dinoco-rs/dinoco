# API reference

This page is a compact index. The focused guides explain behavior, tradeoffs, and complete examples.

## Schema reference

```dinoco
config {
    database = "postgresql" # postgresql | mysql | sqlite
    connection = "direct"  # direct | pgbouncer (PostgreSQL)
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

Scalars: `String`, `Boolean`, `Integer`, `Float`, `DateTime`, `Date`, and `Json`. Add `?` for optional values and `[]` for relation lists.

Field attributes: `@id`, `@unique`, `@index`, `@index(map: "...")`, `@fulltext`, `@default(...)`, and `@relation(...)`.

Model attributes: `@@ids([...])`, `@@uniques([...])`, `@@indexes([...])`, `@@fulltexts([...])`, and `@@table_name("...")`. Every model requires exactly one primary-key declaration: one `@id` or one `@@ids`, never both. Composite arrays preserve field order.

Every primary key and foreign key receives an automatic standard index; the primary-key constraint supplies its index without a redundant `CREATE INDEX`. Full-text fields must be String, and a field cannot belong to both a standard and a full-text declaration.

Default functions: `autoincrement()`, `uuid()`, `snowflake()`, and `now()`. Referential actions: `Cascade`, `Restrict`, `NoAction`, `SetNull`, and `SetDefault`.

## Generated entity API

`#[derive(Entity)]` implements table metadata, native adapter row conversions, typed `Where`, `OrderBy`, `Update`, `Include`, and `Count` helpers, insertion metadata, and:

```rust
pub fn new(required_field: RequiredType, ...) -> Self
```

`#[derive(EntityExtend)]` creates a projection:

```rust
#[derive(Debug, EntityExtend)]
#[extend(User)]
pub struct UserSummary {
    pub id: dinoco::Uuid,
    pub email: String,
}
```

## Read methods

| Builder | Chainable methods | Return from `execute` |
| --- | --- | --- |
| `find_first::<M>()` | `where_`, `where_complex`, `select`, `includes`, `order_by`, `read_in_primary` | `Option<M>` or `Option<S>` |
| `find_many::<M>()` | `where_`, `where_complex`, `select`, `includes`, `order_by`, `take`, `skip`, `read_in_primary` | `Vec<M>` or `Vec<S>` |
| `count::<M>()` | `where_`, `includes` | `M::Count` |

Include builders support `where_`, `where_complex`, `select`, `includes`, `order_by`, `take`, and `skip`.
Count relation builders support typed `where_` filters and populate `Option<i64>` fields in `M::Count`.

## Write methods

| Builder | Required chain | Optional chain | Return |
| --- | --- | --- | --- |
| `insert_into::<M>()` | `values` | `returning::<S>` | `()` or `S` |
| `insert_many::<M>()` | `values` | `returning::<S>` | `()` or `Vec<S>` |
| `update::<M>()` | `update` | `where_`, `returning::<S>` | `()` or `Vec<S>` |
| `update_many::<M>()` | `update` | `where_`, `returning::<S>` | `()` or `Vec<S>` |
| `find_and_update::<M>()` | `update` | `where_`, `where_complex` | updated `M` |
| `delete::<M>()` | `where_` | more `where_`, `returning::<S>` | `()` or `Vec<S>` |
| `delete_many::<M>()` | none | `where_`, `returning::<S>` | `()` or `Vec<S>` |

Scalar updates use `.update(|x| x.field.set(value))`.

## Implicit many-to-many writes

Implicit many-to-many relations expose a write-only virtual `Option<Id>` on both endpoints. A populated field on `insert_into` creates the endpoint and then its pivot link:

```rust
let mut tag = Tag::new("documentation".to_string());
tag.task_id = Some(task.id.clone());

dinoco::insert_into::<Tag>().values(&tag).execute(&client).await?;
```

`insert_many` evaluates the virtual field of every item, creating one pivot row for each `Some(id)` and no link for `None`:

```rust
for tag in &mut tags {
    tag.task_id = Some(task.id.clone());
}

dinoco::insert_many::<Tag>().values(&tags).execute(&client).await?;
```

For endpoints that already exist, use `.connect(value)` and `.disconnect(value)` in `update`, `update_many`, or `find_and_update`. Virtual fields are excluded from endpoint `INSERT` and `SELECT` column lists and remain `None` in returned entities. Both forms work inside transaction batches; transactional virtual-key inserts require an endpoint ID known before execution and therefore reject a populated key with an `autoincrement()` endpoint ID.

## Transactions

`Transaction::new()` creates a heterogeneous builder list. Add operations with `push`, execute with `transactions(transaction).execute(&client).await?`, and read each indexed return value through `TransactionResults::get::<T>` or `take::<T>`. The `transaction![...]` macro is the compact form. The entire batch, including implicit pivot writes from `connect`, `disconnect`, `insert_into`, and `insert_many`, uses one primary connection and rolls back on the first error.

## Filter methods

All scalar fields: `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `batch`, `null`, and `not_null`.

String fields: `like`, `starts_with`, and `ends_with`.

String fields marked with `@fulltext` or included in `@@fulltexts`: `fulltext`.

`fulltext` works in `find_first`, `find_many`, `find_and_update`, one/many include builders, `where_complex` trees, and transactional finds. A member of `@@fulltexts` searches the complete declared field group. The method does not exist on other Strings.

Integer and float fields: `between`.

Ordering fields: `asc` and `desc`.

`where_complex(|x, m| ...)` provides `m.and`, `m.or`, `m.or_many`, and `m.not`. When used, every `where_` on the same builder is ignored.

## Adapter constructors

```rust
PostgresAdapter::direct(url).await?
PostgresAdapter::direct_with_pool(url, min_connections, max_connections).await?
PgBouncerAdapter::new(url).await?
MySqlAdapter::new(url)
SqliteAdapter::new(path).await.map_err(anyhow::Error::msg)?
```

Wrap one in `Backend::{Postgres, PgBouncer, Mysql, Sqlite}` and pass it to `DinocoClient::new`. Attach replica backends with `.with_read_replicas(vec![...])` and opt into SQL logging with `.with_logger(true)`.

## Value types

The runtime parameter layer supports null, integer, float, string, enum, boolean, bytes, JSON, UTC date-time, and naive date values through `DinocoValue`.

Generated fields use `String`, `bool`, `i64`, `f64`, `serde_json::Value`, `chrono::DateTime<chrono::Utc>`, and `chrono::NaiveDate`. Generated UUID and Snowflake IDs use `dinoco::Uuid` and `dinoco::Snowflake` respectively.
