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

The future returned by `.execute(&client)` is `Send`, so a transaction batch can be awaited directly inside an Axum handler or another multithreaded Tokio task.

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

## Many-to-many relations

Implicit many-to-many writes participate in the same database transaction as the rest of the batch. Existing endpoints can be connected or disconnected through the generated virtual ID in `update`, `update_many`, and `find_and_update`:

```rust
let batch = dinoco::transaction![
    dinoco::update::<Task>()
        .where_(|task| task.id.eq(&task_id))
        .update(|task| task.tag_id.connect(&new_tag_id)),
    dinoco::update::<Task>()
        .where_(|task| task.id.eq(&task_id))
        .update(|task| task.tag_id.disconnect(&old_tag_id)),
];

dinoco::transactions(batch).execute(&client).await?;
```

Each update remains one logical result in `TransactionResults`, even when Dinoco executes an additional pivot-table statement for it. A scalar update and a relation write can be placed in the same `.update(...)` closure.

Virtual IDs also work on transactional `insert_into` and `insert_many`. Dinoco inserts each endpoint first and then creates its pivot link before moving to the next builder:

```rust
let mut tag = Tag::new("documentation".to_string());
tag.task_id = Some(task_id.clone());

let mut tags = vec![
    Tag::new("rust".to_string()),
    Tag::new("database".to_string()),
];
for tag in &mut tags {
    tag.task_id = Some(task_id.clone());
}

let batch = dinoco::transaction![
    dinoco::insert_into::<Tag>().values(&tag),
    dinoco::insert_many::<Tag>().values(&tags),
];

dinoco::transactions(batch).execute(&client).await?;
```

`None` inserts the endpoint without a pivot link. If an endpoint insert or pivot write fails, every scalar and relation write in the batch is rolled back.

The inserted endpoint ID must be known before the transaction starts, as with UUID, Snowflake, or a caller-provided ID. A populated virtual relation key cannot be used on a transactional insert whose endpoint primary key is generated with `autoincrement()`; insert that endpoint first and connect it in a later update. Regular non-transactional inserts continue to support this case because they can read the generated ID before creating the pivot row.

## Supported builders

Transactions accept `find_first`, `find_many`, `count`, inserts, updates, deletes, implicit many-to-many `connect`/`disconnect`, and populated virtual IDs on `insert_into` and `insert_many`. `returning` and `find_and_update` work on SQLite and PostgreSQL.

Find builders added to a batch preserve `where_complex`, including `and`, `or`, `or_many`, and `not` groups.
They also preserve `fulltext` conditions in `find_first` and `find_many`, using the active adapter's strategy.

This version rejects the following inside a batch:

- `includes` on finds or counts;
- inserts with nested one-to-one, one-to-many, or many-to-one payloads;
- populated many-to-many virtual IDs on endpoints whose primary key uses `autoincrement()`;
- writes with `returning` and `find_and_update` on MySQL.

These limitations do not affect implicit many-to-many writes with UUID, Snowflake, or caller-provided endpoint IDs.
