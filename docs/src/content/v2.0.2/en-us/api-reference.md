# API reference

This page is a compact index of the schema syntax and generated API surface — it favors completeness and scannability over explanation. For behavior, tradeoffs, and worked examples, follow the links to the focused guide for each topic.

## Schema reference

```dinoco
config {
    database          = "postgresql" # postgresql | mysql | sqlite
    connection        = "direct"     # direct | pgbouncer (PostgreSQL only)
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

**Scalars:** `String`, `Boolean`, `Integer`, `Float`, `DateTime`, `Date`, `Json`. Append `?` for an optional scalar or navigation field, `[]` for a relation list. Singular relation navigation fields always require `?`, independently of whether their local foreign key is required or optional. See [Models and fields](/en-us/docs/orm/guide/models) and [Relations](/en-us/docs/orm/guide/relations).

**Field attributes:** `@id`, `@unique`, `@index`, `@index(map: "...")`, `@fulltext`, `@default(...)`, `@relation(...)`.

**Model attributes:** `@@ids([...])`, `@@uniques([...])`, `@@indexes([...])`, `@@fulltexts([...])`, `@@table_name("...")`. Every model requires exactly one primary-key declaration — one `@id` or one `@@ids`, never both. Composite arrays preserve their declared field order. See [Indexes and constraints](/en-us/docs/orm/guide/indexes).

Every primary key and foreign key gets an automatic standard index; the primary key's own constraint supplies its index without a redundant `CREATE INDEX`. A full-text field must be `String`/`String?`, and a field can never belong to both a standard and a full-text declaration at once.

**Default functions:** `autoincrement()`, `uuid()`, `snowflake()`, `now()`. See [Defaults and enums](/en-us/docs/orm/guide/defaults-enums).

**Referential actions:** `Cascade`, `Restrict`, `NoAction`, `SetNull`, `SetDefault`.

## Generated entity API

`#[derive(Entity)]` implements table metadata, native per-adapter row conversion, typed `Where`/`OrderBy`/`Update`/`Include`/`Count` helpers, insertion metadata, and:

```rust
pub fn new(required_field: RequiredType, ...) -> Self
```

`#[derive(EntityExtend)]` builds a projection instead of a full entity:

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

Include builders support `where_`, `where_complex`, `select`, `includes`, `order_by`, `take`, and `skip`. Count relation builders support typed `where_` filters and populate `Option<i64>` fields on `M::Count`.

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

Scalar field updates always go through `.update(|x| x.field.set(value))`.

> [!WARNING]
> `delete` enforces its `where_` at the type level — the builder has no `.execute()` until you call `.where_(...)` at least once. `delete_many` and `update_many` don't have that protection; an unfiltered call to either affects every row in the table, and it compiles.

## Implicit many-to-many writes

Implicit many-to-many relations expose a write-only virtual `Option<Id>` on both endpoints. A populated field passed to `insert_into` creates the endpoint, then its pivot link, in that order:

```rust
let mut tag = Tag::new("documentation".to_string());
tag.task_id = Some(task.id.clone());

dinoco::insert_into::<Tag>().values(&tag).execute(&client).await?;
```

`insert_many` evaluates the virtual field independently on every item — `Some(id)` creates a pivot row for that item, `None` creates none:

```rust
for tag in &mut tags {
    tag.task_id = Some(task.id.clone());
}

dinoco::insert_many::<Tag>().values(&tags).execute(&client).await?;
```

For endpoints that already exist, use `.connect(value)`/`.disconnect(value)` from `update`, `update_many`, or `find_and_update` instead. The virtual field is excluded from the endpoint's real `INSERT`/`SELECT` column lists and always reads back `None`. Both forms run inside the closure transaction context just like any other mutation. See [Relations](/en-us/docs/orm/guide/relations#implicit-many-to-many).

## Transactions

`transaction(&client, |tx| async move { ... }).await` opens one native transaction pinned to one physical primary connection. Run every mutation inside with `.execute(tx)`. An `Ok(value)` returned by the closure commits and yields that value; any error rolls back everything. `TransactionError` distinguishes create, update, delete, atomic-update, commit, and rollback failures while still preserving the original driver error underneath. See [Transactions](/en-us/docs/orm/orm/transactions).

## Filter methods

| Field kind | Methods |
| --- | --- |
| Every scalar field | `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `batch`, `null`, `not_null` |
| `String`/`Option<String>` | `like`, `starts_with`, `ends_with` |
| `@fulltext` or `@@fulltexts` member | `fulltext` |
| `Integer`/`Float` (filter) | `between` |
| `Integer`/`Float` (update, optional included) | `increment`, `decrement`, `multiply`, `divide` |
| Ordering | `asc`, `desc` |

`fulltext` works in `find_first`, `find_many`, `find_and_update`, one/many include builders, and inside `where_complex` trees — including when `find_and_update` runs through a transaction context. Calling it on any member of an `@@fulltexts` group searches the whole declared group, not just that field. The method simply doesn't exist on a `String` without `@fulltext`.

`where_complex(|x, m| ...)` provides `m.and`, `m.or`, `m.or_many`, and `m.not`. The moment it's used, every plain `where_` on that same builder is ignored. See [Filters](/en-us/docs/orm/orm/filters) and [Where complex](/en-us/docs/orm/orm/where-complex).

## Adapter constructors

```rust
PostgresAdapter::direct(url).await?
PostgresAdapter::direct_with_pool(url, min_connections, max_connections).await?
PgBouncerAdapter::new(url).await?
MySqlAdapter::new(url)
SqliteAdapter::new(path).await.map_err(anyhow::Error::msg)?
```

Wrap one in `Backend::{Postgres, PgBouncer, Mysql, Sqlite}` and pass it to `DinocoClient::new`. Attach replicas with `.with_read_replicas(vec![...])`, and opt into SQL query logging with `.with_logger(true)`. See [Clients and adapters](/en-us/docs/orm/orm/clients-adapters).

## Value types

The runtime's parameter layer, `DinocoValue`, supports null, integer, float, string, enum, boolean, bytes, JSON, UTC date-time, and naive-date values.

Generated fields surface as `String`, `bool`, `i64`, `f64`, `serde_json::Value`, `chrono::DateTime<chrono::Utc>`, and `chrono::NaiveDate`. Generated UUID and Snowflake identifiers use `dinoco::Uuid` and `dinoco::Snowflake` respectively, rather than a bare `String`/`i64` — see [Models and fields](/en-us/docs/orm/guide/models#scalar-types).
