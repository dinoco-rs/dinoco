# Find first

`find_first` fetches at most one row and returns `Option<M>`. Use it when a missing record is an expected result.

## Basic query

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.id.eq(&account_id))
    .execute(&client)
    .await?;
```

The type is `anyhow::Result<Option<Account>>`. Convert `None` into an error only where absence is invalid:

```rust
let account = account
    .ok_or_else(|| anyhow::anyhow!("account not found"))?;
```

## Filter the result

Repeated `where_` calls use `AND`:

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.active.eq(true))
    .where_(|account| account.email.ends_with("@example.com"))
    .execute(&client)
    .await?;
```

Use [Where complex](/v1.2.7/orm/where-complex) for explicit groups. `@fulltext` fields work here too:

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.name.fulltext("matheus"))
    .execute(&client)
    .await?;
```

## Order before choosing

`find_first` accepts one `order_by`:

```rust
let newest = dinoco::find_first::<Account>()
    .where_(|account| account.active.eq(true))
    .order_by(|account| account.created_at.desc())
    .execute(&client)
    .await?;
```

Without ordering, the database may choose any matching row.

## Select and include

`select::<S>()` changes the return to `Option<S>`. `includes(...)` loads relations:

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.id.eq(&account_id))
    .includes(|account| account.sessions())
    .execute(&client)
    .await?;
```

See [Select](/v1.2.7/orm/select) and [Includes](/v1.2.7/orm/includes).

## Read from primary

With replicas configured, use `read_in_primary()` when the query must observe a recent write:

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.id.eq(&account_id))
    .read_in_primary()
    .execute(&client)
    .await?;
```

Every include in that query follows the primary. The closure transaction API accepts mutation builders; perform ordinary `find_first` reads through the client before or after the closure.
