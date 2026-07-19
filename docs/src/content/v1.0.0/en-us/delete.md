# Delete

Dinoco separates single-purpose `delete` from bulk `delete_many`. The first builder enforces a filter at compile time; the second allows an intentionally unfiltered table-wide operation.

## Delete one record

```rust
dinoco::delete::<User>()
    .where_(|x| x.id.eq(&user_id))
    .execute(&client)
    .await?;
```

Additional `.where_` calls are allowed and combined with `AND`.

## The mandatory filter

`delete::<M>()` starts in a type state that has no `.execute()` method. Calling `.where_(...)` moves it into the executable state. This code does not compile:

```rust
// Intentionally invalid: delete requires where_.
dinoco::delete::<User>()
    .execute(&client)
    .await?;
```

The rule prevents the most common accidental table-wide deletion while keeping the filtered API concise.

## Delete many records

Use `delete_many` for a bulk condition:

```rust
dinoco::delete_many::<Session>()
    .where_(|x| x.expires_at.lt(cutoff))
    .execute(&client)
    .await?;
```

`delete_many::<M>().execute(...)` without a filter deletes every row in the table. That behavior is deliberate so cleanup jobs can express a full reset, but it deserves an explicit review at each call site.

## Return deleted data

Both builders support a projection and return a vector of deleted rows:

```rust
let deleted = dinoco::delete::<User>()
    .where_(|x| x.id.eq(&user_id))
    .returning::<UserSummary>()
    .execute(&client)
    .await?;
```

Without `.returning`, execution returns `()`. Avoid returning large deleted models when a count or application log is enough.

## Relations and referential actions

Delete behavior across related tables comes from the migration constraint:

- `Cascade` removes dependent rows.
- `Restrict` or `NoAction` can reject the delete.
- `SetNull` detaches optional related rows.
- `SetDefault` applies the foreign key default.

The runtime does not silently override those choices. Handle a restriction error or explicitly disconnect the relation before deleting.
