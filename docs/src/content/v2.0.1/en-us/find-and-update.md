# Find and update

`find_and_update` applies a conditional `UPDATE` and hands back the resulting entity directly — no separate read afterward. Its numeric operations are evaluated *by the database*, which is what lets a condition like `balance >= amount` and the matching decrement happen as one atomic mutation, immune to the race a read-then-write-in-Rust approach would have between two concurrent requests.

Because it writes data, this builder always runs against the primary backend, even when read replicas are configured — there's no read-replica routing decision to make here.

## 1. Define the filter

```rust
let business = dinoco::find_and_update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .where_(|business| business.balance.gte(amount))
    .update(|business| business.balance.decrement(amount))
    .execute(&client)
    .await?;
```

The conditions compile directly into the `UPDATE` statement — Dinoco never runs a preliminary `SELECT` to check existence or to compute a value first. Prefer filtering on a primary key or another genuinely unique condition, since *every* row the filter matches gets updated, not just "the first one."

## 2. Define the changes

Each `.update(...)` call represents one field's change; they accumulate and compile into a single statement:

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

Every value stays a bind parameter throughout. The regular `.set(value)`, `connect`, and `disconnect` operations all keep working here exactly as they do on `update`/`update_many`.

> [!WARNING]
> Updating the *same* field twice in one `find_and_update` call is rejected with `AtomicUpdateError::DuplicateField`, rather than silently picking one — different databases order multiple assignments to the same column differently, and Dinoco would rather fail loudly than let that ambiguity produce a database-specific result.

## Numeric operations

Generated `Integer`, `Float`, `Integer?`, and `Float?` update fields expose four database-evaluated operations:

```rust
.increment(value)
.decrement(value)
.multiply(value)
.divide(value)
```

These compile to `field = field + ?`, `field = field - ?`, `field = field * ?`, and `field = field / ?` respectively. Optional numeric columns keep ordinary SQL `NULL` semantics throughout — arithmetic on `NULL` stays `NULL`; Dinoco never inserts an implicit `COALESCE` to paper over that.

Division by zero, overflow, rounding, and numeric range behavior are all owned entirely by the database you're running against, and surface back to you through the typed error hierarchy below — Dinoco never pre-reads a value or performs the arithmetic itself in Rust.

## 3. Read the result

The return type is `Result<Model, AtomicUpdateError>` — notably `Result`, not `Option<Model>` wrapped in a `Result`. There's no `.returning()` call needed or available; the updated entity comes back as the success value directly:

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

`RowNotAffected` is determined purely from the mutation's own affected-row count — never from a separate existence `SELECT`. The other variants distinguish an empty update, a duplicate field, a row-decoding failure, a structured database constraint violation, and other database-level failures; `DatabaseError` still keeps the original driver error reachable underneath.

## Adapter behavior

SQLite, PostgreSQL Direct, and PgBouncer all do this in one `UPDATE ... RETURNING` statement. MySQL, which has no equivalent, opens a native transaction, runs the conditional `UPDATE` first, checks its affected-row count, and only then reloads the row — by `id` when an equality predicate on it is available, or by reusing the original filter conditions otherwise. There is never a pre-update `SELECT` on any adapter, MySQL included.

> [!NOTE]
> MySQL's compatibility reload is a second statement on the same physical transaction connection, not a separate round-trip outside it. If a *non*-`id` filter no longer matches by the time the reload runs (a genuinely rare race, but a real one), Dinoco rolls the whole mutation back rather than reporting a misleading `RowNotAffected` after the update itself actually succeeded. This is one good reason to prefer an `id`-equality condition specifically when the field you're filtering on is also one you're updating.

## Use where complex

`find_and_update` accepts the same boolean grouping as the find builders:

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

The same override rule applies here too: once `where_complex` is present, every plain `where_` on the builder is ignored. See [Where complex](/en-us/docs/orm/orm/where-complex).

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

Inside a transaction, `RowNotAffected` is promoted to `TransactionError::AtomicUpdate` and automatically rolls back everything the closure did before it — including the audit-style insert that ran just before it here.

## Limitations

- No `select`, `includes`, `order_by`, `take`, or `skip` — this builder is deliberately narrow, purpose-built for one atomic conditional update.
- At least one `.update(...)` call is required.
- The same column can't be changed twice within one builder call.
- A missing row, or one that no longer matches by the time the update runs, is `AtomicUpdateError::RowNotAffected` — not a panic, not a silent no-op.
- MySQL specifically performs a post-update reload, preferring an equality predicate on `id` and otherwise falling back to the original filter conditions.
