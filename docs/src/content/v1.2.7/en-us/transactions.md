# Transactions

`transaction` runs an async closure on one native database transaction and one physical primary connection. Every mutation executed with `tx` observes earlier writes from that closure. Dinoco commits only when the closure returns `Ok`; any `Err` triggers rollback.

## Create a transaction

Pass the transaction context to each mutation with `.execute(tx)`. The context is copyable, so it can be reused throughout the closure:

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

The closure may return any value in `Ok(value)`. The outer result is `Result<T, TransactionError>`.

## Typed errors

`TransactionError` preserves the operation category and the original driver error. Atomic updates remain matchable through `AtomicUpdateError`:

```rust
use dinoco::{AtomicUpdateError, DatabaseConstraintError, TransactionError};

match result {
    Ok(business) => use_updated_business(business),
    Err(TransactionError::AtomicUpdate(AtomicUpdateError::RowNotAffected)) => {
        // The record does not exist or no longer satisfies the WHERE clause.
    }
    Err(TransactionError::Create(dinoco::CreateError::Constraint {
        kind: DatabaseConstraintError::UniqueViolation,
        ..
    })) => {
        // A structured unique-constraint violation.
    }
    Err(TransactionError::Update(error)) => handle_update(error),
    Err(TransactionError::Delete(error)) => handle_delete(error),
    Err(error) => return Err(error.into()),
}
```

Create, update, delete, decode, and portable constraint categories are kept separate. `DatabaseError::original()` exposes the underlying `rusqlite`, `tokio-postgres`, or `mysql_async` error when driver-specific detail is needed. Constraint classification uses driver codes rather than message parsing.

## Automatic rollback

Every error returned by the closure causes rollback, including `AtomicUpdateError::RowNotAffected` and application errors:

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

If the update affects no row, the earlier audit insert is rolled back.

## Commit and rollback failures

An error returned by `COMMIT` is `TransactionError::Commit(DatabaseError)`. At that point Dinoco reports the driver result without claiming whether the server committed, because that state can be ambiguous after a connection failure.

If an operation fails and `ROLLBACK` also fails, Dinoco returns:

```rust
TransactionError::RollbackFailed {
    source,         // original operation error
    rollback_error, // original rollback driver error
}
```

The original failure is never replaced by the rollback failure.

## Atomicity and connection

- SQLite, PostgreSQL Direct, PgBouncer, and MySQL use a native transaction.
- All commands use the same physical primary connection; read replicas do not participate.
- Each command executes immediately when its future is awaited, so ordinary Rust control flow can use an earlier result.
- Numeric operations and their `WHERE` predicates remain in the database `UPDATE`; no read-before-write calculation is introduced.
- Reusing `tx` outside the closure is rejected by the transaction context.

## Supported builders

The closure API supports `insert_into`, `insert_many`, `update`, `update_many`, `delete`, `delete_many`, `find_and_update`, and generated nested or many-to-many mutation helpers. Returning writes use native support on SQLite and PostgreSQL. MySQL `find_and_update` has a dedicated fallback that executes the conditional `UPDATE` first and then reloads by `id` or the original conditions; it never performs a pre-update existence read.

Read builders that do not accept a mutation executor should continue to use the client outside this closure API. Keep all writes that must commit or roll back together inside the closure.
