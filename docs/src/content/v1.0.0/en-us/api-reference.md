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

Field attributes: `@id`, `@unique`, `@default(...)`, and `@relation(...)`. Model attributes: `@@ids([...])` and `@@table_name("...")`.

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
    pub id: String,
    pub email: String,
}
```

## Read methods

| Builder | Chainable methods | Return from `execute` |
| --- | --- | --- |
| `find_first::<M>()` | `where_`, `select`, `includes`, `order_by`, `read_in_primary` | `Option<M>` or `Option<S>` |
| `find_many::<M>()` | `where_`, `select`, `includes`, `order_by`, `take`, `skip`, `read_in_primary` | `Vec<M>` or `Vec<S>` |
| `count::<M>()` | `where_`, `includes` | `M::Count` |

Include builders support `where_`, `select`, `includes`, `order_by`, `take`, and `skip`.
Count relation builders support typed `where_` filters and populate `Option<i64>` fields in `M::Count`.

## Write methods

| Builder | Required chain | Optional chain | Return |
| --- | --- | --- | --- |
| `insert_into::<M>()` | `values` | `returning::<S>` | `()` or `S` |
| `insert_many::<M>()` | `values` | `returning::<S>` | `()` or `Vec<S>` |
| `update::<M>()` | `update` | `where_`, `returning::<S>` | `()` or `Vec<S>` |
| `update_many::<M>()` | `update` | `where_`, `returning::<S>` | `()` or `Vec<S>` |
| `find_and_update::<M>()` | `update` | `where_` | updated `M` |
| `delete::<M>()` | `where_` | more `where_`, `returning::<S>` | `()` or `Vec<S>` |
| `delete_many::<M>()` | none | `where_`, `returning::<S>` | `()` or `Vec<S>` |

Scalar updates use `.update(|x| x.field.set(value))`. Explicit many-to-many pivots use `.connect(value)` and `.disconnect(value)` with `eq` or `batch` key filters.

## Filter methods

All scalar fields: `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `batch`, `null`, and `not_null`.

String fields: `like`, `starts_with`, and `ends_with`.

Integer and float fields: `between`.

Ordering fields: `asc` and `desc`.

## Adapter constructors

```rust
PostgresAdapter::direct(url).await?
PgBouncerAdapter::new(url).await?
MySqlAdapter::new(url)
SqliteAdapter::new(path).await.map_err(anyhow::Error::msg)?
```

Wrap one in `Backend::{Postgres, PgBouncer, Mysql, Sqlite}` and pass it to `DinocoClient::new`. Attach replica backends with `.with_read_replicas(vec![...])`.

## Value types

The runtime parameter layer supports null, integer, float, string, enum, boolean, bytes, JSON, UTC date-time, and naive date values through `DinocoValue`.

Generated fields use `String`, `bool`, `i64`, `f64`, `serde_json::Value`, `chrono::DateTime<chrono::Utc>`, and `chrono::NaiveDate`. Generated UUID and Snowflake IDs use `dinoco::Uuid` and `dinoco::Snowflake` respectively.
