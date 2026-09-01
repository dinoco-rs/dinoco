# Find and update

`find_and_update` applies a conditional `UPDATE` and returns the entity. Its numeric operations are evaluated by the database, so a condition such as `balance >= amount` and the matching decrement participate in the same atomic mutation.

Because it writes data, the builder always uses the primary backend, even when read replicas are configured.

## 1. Define the filter

```rust
let business = dinoco::find_and_update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .where_(|business| business.balance.gte(amount))
    .update(|business| business.balance.decrement(amount))
    .execute(&client)
    .await?;
```

The conditions are compiled into the `UPDATE` itself. Dinoco does not run a `SELECT` to check existence or calculate a numeric value first. Prefer a primary key or another unique condition because all matching rows may be updated.

## 2. Define the changes

Each `.update(...)` represents one field change. Calls accumulate and compile into one statement:

```rust
let business = dinoco::find_and_update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .update(|business| business.balance.decrement(amount))
    .update(|business| business.total_withdrawn.increment(amount))
    .update(|business| business.transaction_count.increment(1))
    .execute(&client)
    .await?;
```

Conceptually, this produces:

```sql
UPDATE business
SET balance = balance - ?,
    total_withdrawn = total_withdrawn + ?,
    transaction_count = transaction_count + ?
WHERE id = ?
RETURNING ...
```

Values remain bind parameters. Existing `.set(value)`, `connect`, and `disconnect` operations continue to work. Updating the same field more than once is rejected with `AtomicUpdateError::DuplicateField`, avoiding database-specific assignment-order semantics.

## Numeric operations

Generated `Integer`, `Float`, `Integer?`, and `Float?` update fields provide:

```rust
.increment(value)
.decrement(value)
.multiply(value)
.divide(value)
```

They compile respectively to `field = field + ?`, `field = field - ?`, `field = field * ?`, and `field = field / ?`. Optional numeric columns keep normal SQL `NULL` semantics: arithmetic on `NULL` remains `NULL`; Dinoco does not insert an implicit `COALESCE`.

Division by zero, overflow, rounding, and numeric range behavior remain owned by the selected database and are surfaced through the typed error hierarchy. Dinoco does not pre-read or calculate values in Rust.

## 3. Read the result

The return type is `Result<Model, AtomicUpdateError>`, not `Option<Model>`. No `.returning()` call is needed:

```rust
use dinoco::AtomicUpdateError;

match result {
    Ok(business) => use_updated_business(business),
    Err(AtomicUpdateError::RowNotAffected) => {
        // Missing business or the concurrent balance condition did not match.
    }
    Err(AtomicUpdateError::Constraint { kind, source }) => {
        handle_constraint(kind, source);
    }
    Err(error) => return Err(error.into()),
}
```

`RowNotAffected` is determined from the mutation result, without an existence `SELECT`. Other variants distinguish an empty update, a duplicate field, row decoding, a structured database constraint, and other database failures. `DatabaseError` retains the original driver error.

## Adapter behavior

SQLite, PostgreSQL Direct, and PgBouncer use one `UPDATE ... RETURNING` statement. MySQL opens a native transaction, executes the conditional `UPDATE` first, checks its affected-row count, and only then reloads the row. It reloads by `id` when an equality predicate is available; otherwise it reuses the original conditions. There is never a pre-update `SELECT`.

The MySQL compatibility reload is a second statement on the same physical transaction connection. If a non-`id` filter no longer matches after the change, Dinoco rolls the mutation back instead of reporting `RowNotAffected` after a successful update. Prefer an `id` equality condition when updating a field that also participates in the filter.

## Use where complex

`find_and_update` accepts the same boolean groups as find builders:

```rust
let account = dinoco::find_and_update::<Account>()
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

When `where_complex` is present, every `where_` on the builder is ignored. See [Where complex](/v1.3.0/orm/where-complex).

## Use in a transaction

```rust
let business = dinoco::transaction(&client, |tx| async move {
    let business = dinoco::find_and_update::<Business>()
        .where_(|business| business.id.eq(&business_id))
        .where_(|business| business.balance.gte(amount))
        .update(|business| business.balance.decrement(amount))
        .execute(tx)
        .await?;

    dinoco::insert_into::<BusinessTransaction>()
        .value(&movement)
        .execute(tx)
        .await?;

    Ok(business)
})
.await?;
```

`RowNotAffected` is promoted to `TransactionError::AtomicUpdate` and automatically rolls back earlier writes.

## Limitations

- No `select`, `includes`, `order_by`, `take`, or `skip`.
- At least one `.update(...)` is required.
- The same column cannot be changed twice in one builder.
- A missing or no-longer-matching row is `AtomicUpdateError::RowNotAffected`.
- MySQL performs a post-update reload, preferring an equality predicate on `id` and otherwise reusing the original conditions.
