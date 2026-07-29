# Transactions

A transaction batch executes different builders in insertion order on one physical connection. If any operation fails, the adapter rolls back every previous operation. Commit happens only after the entire list succeeds.

## Create a transaction

Use `Transaction::new()` because a regular Rust `Vec` cannot contain builders of different concrete types:

```rust
use dinoco::{
    Transaction, find_first, insert_into, transactions,
};

let session = AccountSession::new(
    "session-1".to_string(),
    account_id.clone(),
);

let mut transaction = Transaction::new();

transaction.push(
    find_first::<Account>()
        .where_(|account| account.id.eq(&account_id))
);

transaction.push(
    find_first::<AccountSession>()
        .where_(|session| session.id.eq("session-1"))
);

transaction.push(
    insert_into::<AccountSession>().values(&session)
);

let results = transactions(transaction)
    .execute(&client)
    .await?;
```

`Transcation` is also exported as an alias for `Transaction` for compatibility with the spelling used by earlier examples. New code should prefer `Transaction`.

## Use the macro

The `transaction!` macro builds the same list more compactly:

```rust
let transaction = dinoco::transaction![
    find_first::<Account>()
        .where_(|account| account.id.eq(&account_id)),
    insert_into::<AccountSession>().values(&session),
];

let results = transactions(transaction)
    .execute(&client)
    .await?;
```

## Read results

Each builder produces one position in push order. Reads preserve their normal return type and writes without `returning` produce `()`:

```rust
let mut results = transactions(transaction)
    .execute(&client)
    .await?;

let account: Option<Account> = results.take(0)?;
results.take::<()>(1)?;
```

Use `get::<T>(index)` to borrow a result or `take::<T>(index)` to remove it. An invalid index, an already-taken result, or a type mismatch returns an error.

## Atomicity and connection

- SQLite, PostgreSQL Direct, PgBouncer, and MySQL run the batch in a native transaction.
- Every operation uses the primary backend; read replicas never participate.
- An operation observes writes completed by earlier operations in the same batch.
- SQL, constraint, and row-conversion errors cause rollback.
- Invalid builders are rejected before the transaction opens.

## Supported builders

Transactions accept `find_first`, `find_many`, `count`, flat inserts, scalar updates, and deletes. `returning` and `find_and_update` work on SQLite and PostgreSQL.

Find builders added to a batch preserve `where_complex`, including `and`, `or`, `or_many`, and `not` groups.
They also preserve `fulltext` conditions in `find_first` and `find_many`, using the active adapter's strategy.

This version rejects the following inside a batch:

- `includes` on finds or counts;
- inserts with nested relation payloads;
- `connect` and `disconnect`;
- writes with `returning` and `find_and_update` on MySQL.

Run those flows outside the batch or split them into explicit scalar builders until transactional execution supports them.
