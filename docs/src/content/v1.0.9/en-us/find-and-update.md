# Find and update

`find_and_update` combines filtering, updating, and returning the complete entity in one builder. Use it when code needs the updated value without assembling a separate `.returning()` call.

## 1. Define the filter

```rust
let account = dinoco::find_and_update::<Account>()
    .where_(|account| account.id.eq(&account_id))
    .update(|account| account.name.set("Matheus".to_string()))
    .execute(&client)
    .await?;
```

The filter should identify exactly one intended row. If several rows match, all may be updated and the builder returns one of them; use a primary key or another unique constraint.

## 2. Define the changes

Each `.update(...)` adds one field:

```rust
let account = dinoco::find_and_update::<Account>()
    .where_(|account| account.id.eq(&account_id))
    .update(|account| account.name.set("Matheus".to_string()))
    .update(|account| account.active.set(true))
    .execute(&client)
    .await?;
```

At least one update is required. `connect` and `disconnect` are not accepted by this builder.

## 3. Read the result

The return type is `anyhow::Result<Account>`, not `Option<Account>`. No `.returning()` call is required.

When no row matches, execution returns an error:

```text
Record from table 'account' could not be found for update.
```

SQL, constraint, and conversion failures are propagated too.

## Adapter behavior

SQLite, PostgreSQL Direct, and PgBouncer use one `UPDATE ... RETURNING` statement. MySQL does not expose the same returning operation, so Dinoco finds IDs, runs the update, and reads the result in separate steps.

The MySQL path therefore does not provide statement-level atomicity. Keep the filter unique and do not depend on concurrency between those steps.

## Use where complex

`find_and_update` accepts the same boolean groups as find builders:

```rust
let account = dinoco::find_and_update::<Account>()
    .where_(|account| account.id.eq("ignored"))
    .where_complex(|account, m| {
        m.and([
            account.email.eq("matheus@example.com"),
            m.not(account.locked.eq(true)),
        ])
    })
    .update(|account| account.active.set(true))
    .execute(&client)
    .await?;
```

When `where_complex` is present, every `where_` on the builder is ignored, before or after it. See [Where complex](/v1.0.9/orm/where-complex).

## Use full-text

An `@fulltext` field can select the row:

```rust
let article = dinoco::find_and_update::<Article>()
    .where_(|article| article.body.fulltext("dinoco"))
    .update(|article| article.reviewed.set(true))
    .execute(&client)
    .await?;
```

The method only exists on String fields marked in `schema.dinoco`.

## Use in a transaction

```rust
let batch = dinoco::transaction![
    dinoco::find_and_update::<Account>()
        .where_(|account| account.id.eq(&account_id))
        .update(|account| account.active.set(true)),
];

let mut results = dinoco::transactions(batch)
    .execute(&client)
    .await?;

let account = results.take::<Account>(0)?;
```

Transactional support is available on SQLite, PostgreSQL Direct, and PgBouncer. MySQL still rejects `find_and_update` inside a batch because returning writes are not part of that executor yet.

## Limitations

- No `select`, `includes`, `order_by`, `take`, or `skip`.
- No `connect` or `disconnect`.
- At least one `.update(...)` is required.
- A missing row is an error.
- MySQL emulates the return with more than one statement.
- It is unavailable in MySQL transactions in v1.0.9.
