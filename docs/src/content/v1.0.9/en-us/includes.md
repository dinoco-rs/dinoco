# Includes

`includes(...)` populates relation fields in a query. Without an include, relations keep their generated empty value: `Vec::new()` for many and `None` for one.

## Include a many relation

```rust
let accounts = dinoco::find_many::<Account>()
    .includes(|account| account.sessions())
    .execute(&client)
    .await?;
```

Dinoco collects parent keys and loads children in one batch query.

## Include a one relation

```rust
let session = dinoco::find_first::<AccountSession>()
    .where_(|session| session.id.eq(&session_id))
    .includes(|session| session.account())
    .execute(&client)
    .await?;
```

One relations use a left-join strategy and remain optional when no row matches.

## Filter the relation

The relation builder exposes the same generated filters:

```rust
let accounts = dinoco::find_many::<Account>()
    .includes(|account| {
        account
            .sessions()
            .where_(|session| session.revoked.eq(false))
            .order_by(|session| session.created_at.desc())
            .take(5)
            .skip(0)
    })
    .execute(&client)
    .await?;
```

For a many relation, `take(5)` applies per parent. The compiler uses a window partition in the batch query.

## Use where complex and full-text

```rust
let accounts = dinoco::find_many::<Account>()
    .includes(|account| {
        account.sessions().where_complex(|session, m| {
            m.and([
                session.label.fulltext("mobile"),
                m.not(session.revoked.eq(true)),
            ])
        })
    })
    .execute(&client)
    .await?;
```

`where_complex` ignores every `where_` on the same relation builder. `.fulltext(...)` only exists when the related field has `@fulltext`.

## Build nested includes

```rust
let projects = dinoco::find_many::<Project>()
    .includes(|project| project.owner())
    .includes(|project| {
        project
            .tasks()
            .order_by(|task| task.priority.desc())
            .take(10)
            .includes(|task| task.assignee())
    })
    .execute(&client)
    .await?;
```

Sibling includes are awaited in parallel; each nested level repeats the appropriate strategy.

## Combine with select

The relation builder also accepts `select::<S>()`. Its relation key is loaded separately from the projection to preserve correct grouping.

## Primary and transactions

`read_in_primary()` on the parent find routes both the parent and every include to primary.

Includes are not supported inside a `Transaction` in v1.0.9. The builder returns an error before opening the transaction.
