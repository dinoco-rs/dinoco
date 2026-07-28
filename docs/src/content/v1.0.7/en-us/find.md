# Find records

Find builders are lazy. Chaining methods only describes a query; `.execute(&client).await` compiles it with the active adapter and performs I/O.

## Find the first record

```rust
use dinoco::find_first;
use crate::dinoco::User;

let user = find_first::<User>()
    .where_(|x| x.email.eq("ana@example.com"))
    .execute(&client)
    .await?;
```

The result is `Option<User>`. Use `if let`, `match`, or `ok_or_else` according to whether absence is expected:

```rust
let user = user.ok_or_else(|| anyhow::anyhow!("user was not found"))?;
```

You may add more than one `.where_`; all conditions are combined with `AND`.

## Find many records

```rust
use dinoco::find_many;

let users = find_many::<User>()
    .where_(|x| x.active.eq(true))
    .order_by(|x| x.created_at.desc())
    .execute(&client)
    .await?;
```

The result is `Vec<User>`. No matches produce an empty vector.

## Ordering and pagination

Both find builders support one typed `.order_by(...)` with `.asc()` or `.desc()`. `find_many` also supports `.take()` and `.skip()`:

```rust
let page = find_many::<User>()
    .order_by(|x| x.id.asc())
    .take(25)
    .skip(50)
    .execute(&client)
    .await?;
```

Always pair offset pagination with a stable order. Without an order, the database is free to return rows in a different sequence between requests.

## Read from the primary

When replicas are configured, ordinary finds alternate between them. Force a consistency-sensitive query to the primary after a write:

```rust
let user = find_first::<User>()
    .where_(|x| x.id.eq(&user_id))
    .read_in_primary()
    .execute(&client)
    .await?;
```

The flag also follows relation includes, preventing a primary parent row from being combined with stale relation data from a replica.

## Return values

| Builder | Default return |
| --- | --- |
| `find_first::<M>()` | `anyhow::Result<Option<M>>` |
| `find_many::<M>()` | `anyhow::Result<Vec<M>>` |
| `find_first::<M>().select::<S>()` | `anyhow::Result<Option<S>>` |
| `find_many::<M>().select::<S>()` | `anyhow::Result<Vec<S>>` |

Database, decoding, and relation-loading failures are errors. Finding no row is not.
