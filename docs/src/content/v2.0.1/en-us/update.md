# Update

Update builders keep two things deliberately separate: which rows to touch (`where_`) and what to change on them (`update`). Each `.update(...)` closure picks exactly one generated field, and its `.set(...)` method only accepts that field's actual Rust type — there's no way to accidentally assign a `String` into an `Integer` column.

## Update one record

```rust
dinoco::update::<User>()
    .where_(|x| x.id.eq(&user_id))
    .update(|x| x.name.set("Ana Silva".to_string()))
    .execute(&client)
    .await?;
```

Call `.update(...)` more than once to change several fields in the same statement:

```rust
dinoco::update::<User>()
    .where_(|x| x.id.eq(&user_id))
    .update(|x| x.name.set("Ana Silva"))
    .update(|x| x.active.set(true))
    .execute(&client)
    .await?;
```

`update` requires at least one `.update(...)` call — there's no such thing as an update that changes nothing. Relation fields never appear here at all; `.set(...)` only exists on scalar and enum fields.

## Update many records

`update_many` is the exact same API, applied to every row the filter matches instead of assuming one:

```rust
dinoco::update_many::<User>()
    .where_(|x| x.office.eq("support"))
    .update(|x| x.active.set(false))
    .execute(&client)
    .await?;
```

> [!WARNING]
> `update_many`'s type state does **not** force a `.where_(...)` call the way `delete` does. An `update_many` with no filter at all updates every single row in the table. Be as deliberate about an unfiltered bulk update as you would be about `DELETE FROM table` with no `WHERE`.

## Set optional fields

An optional field's `.set(...)` takes `Option<T>` — `Some(value)` to assign a value, `None` to write SQL `NULL`:

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

Try this on a *required* field and it simply won't compile — the generated `.set(...)` for a required scalar takes `T`, not `Option<T>`, so there's no `None` to accidentally pass.

## Return a projection

`.returning::<S>()` always comes back as a `Vec`, even conceptually for `update`, because the filter it's attached to could still match more than one row:

```rust
let changed = dinoco::update::<User>()
    .where_(|x| x.office.eq("support"))
    .update(|x| x.active.set(false))
    .returning::<UserSummary>()
    .execute(&client)
    .await?;
```

## Connect and disconnect many-to-many records

Dinoco generates a virtual target-ID field on each endpoint of an implicit many-to-many relation. Filter the endpoint you're updating normally, then connect or disconnect through that virtual field like any other update:

```rust
dinoco::update::<Post>()
    .where_(|x| x.id.eq(&post_id))
    .update(|x| x.tag_id.connect(&tag_id))
    .execute(&client)
    .await?;
```

Disconnecting the same pair looks identical, just with `.disconnect(...)`:

```rust
dinoco::update_many::<Post>()
    .where_(|x| x.id.eq(&post_id))
    .update(|x| x.tag_id.disconnect(&tag_id))
    .execute(&client)
    .await?;
```

> [!TIP]
> Need to update exactly one row and get the complete, up-to-date entity back without reaching for `.returning(...)` separately? That's exactly what [Find and update](/en-us/docs/orm/orm/find-and-update) is for.

These same virtual fields work identically across `update`, `update_many`, and `find_and_update`, `.returning::<T>()` included on the first two. Run the builder with `.execute(tx)` inside `transaction(&client, |tx| ...)` to make the endpoint write and the pivot change commit or roll back as one unit.

When the endpoint you're linking is being created in the same flow, you can skip the separate update entirely: assign its virtual `Option<Id>` before `insert_into` (or on each applicable `insert_many` item), and Dinoco creates the pivot link right after inserting the endpoint — see [Insert](/en-us/docs/orm/orm/insert#connect-an-implicit-many-to-many-during-insert).

See [Implicit many-to-many](/en-us/docs/orm/guide/relations#implicit-many-to-many) for the complete endpoint API and the pivot table's exact semantics.
