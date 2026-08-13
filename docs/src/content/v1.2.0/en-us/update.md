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

## Connect and disconnect many-to-many records

Dinoco generates a virtual target ID on each endpoint. Filter the endpoint normally and connect through that virtual field:

```rust
dinoco::update::<Post>()
    .where_(|x| x.id.eq(&post_id))
    .update(|x| x.tag_id.connect(&tag_id))
    .execute(&client)
    .await?;
```

Disconnect the same pair:

```rust
dinoco::update_many::<Post>()
    .where_(|x| x.id.eq(&post_id))
    .update(|x| x.tag_id.disconnect(&tag_id))
    .execute(&client)
    .await?;
```

To update exactly one row and receive the complete entity without `.returning()`, see the dedicated [Find and update](/v1.2.0/orm/find-and-update) page.

The same virtual fields work with `update`, `update_many`, and `find_and_update`, including `.returning::<T>()` on the first two builders. They also work inside transaction batches: the endpoint write and its pivot changes commit or roll back together while the builder still produces one result position.

When one endpoint is being created, you can skip the separate update: assign its virtual `Option<Id>` before `insert_into`, or on every applicable `insert_many` item, and Dinoco creates the pivot link after inserting the endpoint.

See [Implicit many-to-many](/v1.2.0/guide/relations#implicit-many-to-many) for the complete endpoint API and pivot-table semantics.
