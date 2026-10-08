# Transactions

`transaction` runs an async closure against one native database transaction, pinned to one physical primary connection for its whole duration. Every operation you run through the `tx` handle it hands you — writes, and `find_first`/`find_many` reads — sees every earlier write from that same closure — reads inside the transaction are never stale relative to writes that already happened in it. Dinoco commits only when the closure returns `Ok`; any `Err`, from any source, triggers a rollback.

## Create a transaction

Pass the transaction context to each mutation with `.execute(tx)` instead of `.execute(&client)`. The context is `Copy`, so it can be passed around and reused freely throughout the closure:

```rust
use dinoco::{find_and_update, insert_into, transaction};

let result = transaction(&client, |tx| async move {
    let business = find_and_update::<Business>()
        .where_(|business| business.id.eq(&business_id))
        .where_(|business| business.balance.gte(amount))
        .update(|business| business.balance.decrement(amount))
        .execute(tx)
        .await?;

    insert_into::<BusinessTransaction>()
        .value(&movement)
        .execute(tx)
        .await?;

    Ok(business)
})
.await;
```

The closure can return anything wrapped in `Ok(value)` — Dinoco doesn't constrain the success type. The outer result is `Result<T, TransactionError>`.

## Typed errors

`TransactionError` preserves both the category of operation that failed and the original driver error underneath it. Atomic-update failures stay specifically matchable through `AtomicUpdateError`:

```rust
use dinoco::{AtomicUpdateError, CreateError, TransactionError};

match result {
    Ok(business) => use_updated_business(business),
    Err(TransactionError::AtomicUpdate(AtomicUpdateError::RowNotAffected)) => {
        // The record does not exist or no longer satisfies the WHERE clause.
    }
    Err(TransactionError::Create(CreateError::UniqueViolation { columns, .. })) => {
        // A structured unique-constraint violation, e.g. columns == ["email"].
    }
    Err(TransactionError::Update(error)) => handle_update(error),
    Err(TransactionError::Delete(error)) => handle_delete(error),
    Err(error) => return Err(error.into()),
}
```

Inserts report each constraint kind as its own `CreateError` variant (`UniqueViolation`, `ForeignKeyViolation`, `NotNullViolation`, `CheckViolation`) with the table, constraint, and columns the driver named — see [Handle insert errors](/en-us/docs/orm/orm/insert#handle-insert-errors). Create, update, delete, decode, and portable-constraint failure categories are kept distinct rather than flattened into one generic error — so a `match` like the one above can handle the specific cases it cares about and fall through to a generic path for the rest. When you need driver-level detail beyond what Dinoco classifies, `DatabaseError::original()` exposes the underlying `rusqlite`, `tokio-postgres`, or `mysql_async` error directly. Constraint classification itself is based on structured driver error codes, never on parsing an error message string.

## Read inside a transaction

`find_first` and `find_many` accept the same `tx` handle. The read runs on the transaction's connection, so it sees every uncommitted write made earlier in the closure — including relations loaded with `.includes(...)`:

```rust
use dinoco::{find_first, find_many, insert_into, transaction};

let result = transaction(&client, |tx| async move {
    insert_into::<Business>().value(&business).execute(tx).await?;
    insert_into::<Office>().value(&office).execute(tx).await?;

    let stored = find_first::<Business>()
        .where_(|x| x.id.eq(&business_id))
        .includes(|x| x.offices())
        .execute(tx)
        .await?
        .expect("inserted above");

    let office_ids = find_many::<Office>()
        .where_(|x| x.business_id.eq(&business_id))
        .pluck(|office| office.id)
        .execute(tx)
        .await?;

    Ok((stored, office_ids))
})
.await;
```

If the closure later fails, those reads simply saw data that never gets committed — nothing outside the transaction ever observes it. Reads through `tx` always go to the transaction's primary connection, never to a replica, so `read_in_primary()` makes no difference there. `count`, `exists`, and `find_batch` still take `&client` only.

## Automatic rollback

Any error the closure returns rolls back everything the transaction has done so far — including `AtomicUpdateError::RowNotAffected`, which is often the exact signal you want to trigger a rollback on:

```rust
let result = transaction(&client, |tx| async move {
    insert_into::<AuditEntry>().value(&entry).execute(tx).await?;

    find_and_update::<Business>()
        .where_(|business| business.id.eq(&business_id))
        .where_(|business| business.balance.gte(amount))
        .update(|business| business.balance.decrement(amount))
        .execute(tx)
        .await?;

    Ok(())
})
.await;
```

If the balance update here affects zero rows (say, because `balance.gte(amount)` no longer holds), the `?` propagates `RowNotAffected` out of the closure — and the audit entry inserted just before it is rolled back along with it, exactly as if neither statement had run.

## Commit and rollback failures

An error from `COMMIT` itself comes back as `TransactionError::Commit(DatabaseError)`. At that specific point, Dinoco reports the driver's result honestly without asserting whether the server actually committed — after a connection failure right at commit time, that state can be genuinely ambiguous, and pretending otherwise would be worse than surfacing the uncertainty.

> [!WARNING]
> If an operation inside the closure fails **and** the subsequent `ROLLBACK` also fails, Dinoco returns both pieces of information rather than discarding one:
>
> ```rust
> TransactionError::RollbackFailed {
>     source,         // original operation error
>     rollback_error, // original rollback driver error
> }
> ```
>
> The original failure is never silently replaced by the rollback failure — you get to see what actually went wrong first, and what went wrong trying to clean up after it.

## Atomicity and connection

- SQLite, PostgreSQL Direct, PgBouncer, and MySQL all use a real native database transaction underneath — this isn't an application-level emulation.
- Every command in the closure shares the exact same physical primary connection; read replicas never participate in a transaction.
- Each command executes as soon as its future is awaited, in the order you write it — ordinary Rust control flow (an `if`, a loop, an earlier `let`) works exactly as you'd expect, because there's no batching or deferred execution happening behind the scenes.
- Numeric operations (`increment`/`decrement`/`multiply`/`divide`) and their `WHERE` predicates both stay inside the single database `UPDATE` statement — Dinoco never introduces a read-then-compute-then-write race by calculating the new value in Rust first.
- The transaction context rejects being reused outside the closure it belongs to, by construction.

## Supported builders

The closure API supports `find_first`, `find_many`, `insert_into`, `insert_many`, `update`, `update_many`, `delete`, `delete_many`, `find_and_update`, and the generated helpers for nested or many-to-many mutations. Returning writes (`.returning::<S>()`) use each database's native support on SQLite and PostgreSQL; MySQL's `find_and_update` uses a dedicated fallback instead — it runs the conditional `UPDATE` first, then reloads the row by `id` or by the original conditions, and never performs a separate existence check before attempting the update.

> [!NOTE]
> `count`, `exists`, and `find_batch` don't accept the transaction context yet — keep using `&client` for them. They read committed data only, so they won't see writes the closure hasn't committed.
