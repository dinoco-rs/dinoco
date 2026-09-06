# Find first

`find_first` fetches at most one row and returns `Option<M>`. Reach for it whenever a missing record is a normal, expected outcome — not a failure.

## Basic query

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.id.eq(&account_id))
    .execute(&client)
    .await?;
```

The full type here is `anyhow::Result<Option<Account>>` — two layers of "it might not work out," and they mean different things. Only convert the `None` into an error at the point where absence actually *is* invalid for your use case:

```rust
let account = account.ok_or_else(|| anyhow::anyhow!("account not found"))?;
```

## Filter the result

Repeated `where_` calls combine with `AND`:

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.active.eq(true))
    .where_(|account| account.email.ends_with("@example.com"))
    .execute(&client)
    .await?;
```

Need `OR`/`NOT` instead? See [Where complex](/en-us/docs/orm/orm/where-complex). `@fulltext` fields work here too, through the same `where_`:

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.name.fulltext("matheus"))
    .execute(&client)
    .await?;
```

## Order before choosing

`find_first` accepts exactly one `order_by`:

```rust
let newest = dinoco::find_first::<Account>()
    .where_(|account| account.active.eq(true))
    .order_by(|account| account.created_at.desc())
    .execute(&client)
    .await?;
```

> [!WARNING]
> Without an `order_by`, which of several matching rows comes back is up to the database — it's allowed to return any of them, and that choice can even change between two otherwise-identical queries. If "the first matching row" needs to mean something specific (the newest, the highest-priority), say so explicitly with `order_by`.

## Select and include

`select::<S>()` changes the return type to `Option<S>`; `includes(...)` loads a relation alongside the row:

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.id.eq(&account_id))
    .includes(|account| account.sessions())
    .execute(&client)
    .await?;
```

See [Select](/en-us/docs/orm/orm/select) and [Includes](/en-us/docs/orm/orm/includes) for the details of each.

## Read from primary

With replicas configured, add `read_in_primary()` when this specific query needs to observe a write that just happened:

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.id.eq(&account_id))
    .read_in_primary()
    .execute(&client)
    .await?;
```

Every `.includes(...)` on that same query follows it to the primary too — you can't route the main row to the primary while letting its includes fall back to a replica. Note that the closure transaction API only accepts mutation builders, not `find_first`; run ordinary reads like this one through `&client` directly, before or after the transaction closure.
