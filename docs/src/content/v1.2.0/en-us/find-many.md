# Find many

`find_many` returns every matching row as `Vec<M>`. No matches produce an empty vector.

## Basic query

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.active.eq(true))
    .execute(&client)
    .await?;
```

Repeated `where_` calls are combined with `AND`. Use [Where complex](/v1.2.0/orm/where-complex) when precedence requires explicit `AND`, `OR`, and `NOT` groups.

## Order results

```rust
let accounts = dinoco::find_many::<Account>()
    .order_by(|account| account.created_at.desc())
    .execute(&client)
    .await?;
```

The builder accepts one typed ordering with `asc()` or `desc()`.

## Pagination

`take` limits the number of rows and `skip` sets the offset:

```rust
let page = dinoco::find_many::<Account>()
    .order_by(|account| account.id.asc())
    .take(25)
    .skip(50)
    .execute(&client)
    .await?;
```

Always pair offset pagination with stable ordering.

## Full-text search

Fields marked with `@fulltext` expose the method:

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.biography.fulltext("rust database"))
    .execute(&client)
    .await?;
```

See [Full-text search](/v1.2.0/orm/full-text-search) for adapter differences.

## Select and include

```rust
let accounts = dinoco::find_many::<Account>()
    .select::<AccountSummary>()
    .execute(&client)
    .await?;
```

The return becomes `Vec<AccountSummary>`. `includes(...)` can load relations, filter children, and paginate per parent. See [Select](/v1.2.0/orm/select) and [Includes](/v1.2.0/orm/includes).

## Read from primary

`read_in_primary()` bypasses replicas for this query and all its includes. Use it for reads that depend on a recent write.

## Use in a transaction

`find_many` can be added to a transaction batch. Its result remains `Vec<M>` or `Vec<S>`. Includes inside a batch are not supported yet.
