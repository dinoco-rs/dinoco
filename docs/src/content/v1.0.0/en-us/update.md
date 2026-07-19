# Update

Update builders keep filtering and changed fields separate. A closure selects one generated field at a time, and its `.set(...)` method accepts the same Rust type as that field.

## Update one record

```rust
dinoco::update::<User>()
    .where_(|x| x.id.eq(&user_id))
    .update(|x| x.name.set("Ana Silva".to_string()))
    .execute(&client)
    .await?;
```

Call `.update(...)` more than once to change multiple fields:

```rust
dinoco::update::<User>()
    .where_(|x| x.id.eq(&user_id))
    .update(|x| x.name.set("Ana Silva"))
    .update(|x| x.active.set(true))
    .execute(&client)
    .await?;
```

`update` requires at least one update operation. Relations are not exposed through `.set(...)`.

## Update many records

`update_many` uses the same API and applies the change to every matching row:

```rust
dinoco::update_many::<User>()
    .where_(|x| x.office.eq("support"))
    .update(|x| x.active.set(false))
    .execute(&client)
    .await?;
```

Be deliberate with an unfiltered bulk update. Unlike `delete`, its type state does not force a `.where_` call.

## Set optional fields

An optional field accepts `Option<T>`. Use `Some(value)` to assign and `None` to write SQL `NULL`:

```rust
dinoco::update::<User>()
    .where_(|x| x.id.eq(&user_id))
    .update(|x| x.bio.set(Some("Maintainer".to_string())))
    .execute(&client)
    .await?;

dinoco::update::<User>()
    .where_(|x| x.id.eq(&user_id))
    .update(|x| x.bio.set(None::<String>))
    .execute(&client)
    .await?;
```

The generated update field prevents assigning `None` to a required scalar.

## Return a projection

`.returning::<S>()` returns a vector because a filter may match more than one row:

```rust
let changed = dinoco::update::<User>()
    .where_(|x| x.office.eq("support"))
    .update(|x| x.active.set(false))
    .returning::<UserSummary>()
    .execute(&client)
    .await?;
```

## Atomic find and update

Use `find_and_update` when one statement must update and return the selected entity. It returns the full updated model by default, with no `.returning()` call:

```rust
let user = dinoco::find_and_update::<User>()
    .where_(|x| x.id.eq(&user_id))
    .update(|x| x.name.set("Ana Silva"))
    .execute(&client)
    .await?;
```

The operation fails if no record can be returned. It does not support relation `connect` or `disconnect`.

## Connect and disconnect many-to-many records

Dinoco generates the pivot entity automatically. Identify its source key with `eq` or `batch`:

```rust
dinoco::update::<PostTag>()
    .where_(|x| x.post_id.eq(&post_id))
    .update(|x| x.tag_id.connect(&tag_id))
    .execute(&client)
    .await?;
```

Disconnect the same pair:

```rust
dinoco::update_many::<PostTag>()
    .where_(|x| x.post_id.eq(&post_id))
    .update(|x| x.tag_id.disconnect(&tag_id))
    .execute(&client)
    .await?;
```

For several source IDs, use `.batch(...)` in the filter. Connect and disconnect accept only `eq` and `batch` pivot-key filters, and cannot be combined with `.returning::<T>()`.
