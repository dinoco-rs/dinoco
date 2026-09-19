# Find many

`find_many` returns every matching row as `Vec<M>`. Zero matches is a perfectly normal result here — you get back an empty vector, never an error.

## Basic query

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.active.eq(true))
    .execute(&client)
    .await?;
```

Repeated `where_` calls combine with `AND`. Reach for [Where complex](/en-us/docs/orm/orm/where-complex) the moment you need explicit precedence across `AND`, `OR`, and `NOT`.

## Order results

```rust
let accounts = dinoco::find_many::<Account>()
    .order_by(|account| account.created_at.desc())
    .execute(&client)
    .await?;
```

The builder accepts one typed ordering, via `asc()` or `desc()` on a field.

## Pagination

`take` caps how many rows come back; `skip` sets how many to skip before counting:

```rust
let page = dinoco::find_many::<Account>()
    .order_by(|account| account.id.asc())
    .take(25)
    .skip(50)
    .execute(&client)
    .await?;
```

> [!WARNING]
> Offset pagination (`take`/`skip`) only produces a stable, non-overlapping sequence of pages when paired with a stable `order_by`. Paginate without ordering — or order by a column with ties, like a status field — and rows can appear on two pages or vanish between them as the underlying data changes.

## Full-text search

Any field marked `@fulltext` exposes a matching method:

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.biography.fulltext("rust database"))
    .execute(&client)
    .await?;
```

See [Full-text search](/en-us/docs/orm/orm/full-text-search) for how this behaves differently per adapter.

## Select and include

```rust
let accounts = dinoco::find_many::<Account>()
    .select::<AccountSummary>()
    .execute(&client)
    .await?;
```

The return type becomes `Vec<AccountSummary>` instead of `Vec<Account>`. `includes(...)` works alongside `select`, and can filter, order, and paginate the related rows independently of the parent query — see [Select](/en-us/docs/orm/orm/select) and [Includes](/en-us/docs/orm/orm/includes).

## Pluck a single column

`pluck` projects onto one column and returns its raw values directly, instead of a row-model struct:

```rust
let ids = dinoco::find_many::<Account>()
    .where_(|account| account.active.eq(true))
    .pluck(|account| account.id)
    .execute(&client)
    .await?; // Vec<i64>
```

Unlike `select::<S>()`, `pluck` never builds `S` — the column comes back as `Vec<T>` (a `String`, `bool`, `i64`, `f64`, date, or JSON value, matching the field's type). It replaces `select`/`includes` rather than combining with them.

## Transform results

`transform` runs a plain Rust closure over every row after the query (and any `.includes(...)`) has already run — it is a post-query mapping, not a SQL projection:

```rust
let summaries = dinoco::find_many::<Account>()
    .transform(|account| serde_json::json!({
        "id": account.id,
        "email": account.email,
    }))
    .execute(&client)
    .await?; // Vec<serde_json::Value>
```

Because the callback receives the fully materialized row, it can read fields from `.includes(...)`, reshape data for an API response, or convert into a different type entirely.

## Read from primary

`read_in_primary()` routes this query — and every relation it `.includes(...)` — away from replicas and onto the primary. Reach for it specifically when the read depends on a write that just happened and can't tolerate replication lag. As with the other read builders, `find_many` isn't part of the closure transaction API; run it through `&client` directly, outside any `transaction(...)` closure.
