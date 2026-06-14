# delete

Used to delete with an explicit filter.

---

## What you can do

- `.cond(...)`: defines which record will be removed.
- `.execute(&client)`: executes the removal in the database.

## Return

The return of `delete` is:

```rust
DinocoResult<()>
```

## Basic example

```rust
dinoco::delete::<User>()
    .cond(|w| w.id.eq(10))
    .execute(&client)
    .await?;
```

## Example with another filter

```rust
dinoco::delete::<Session>()
    .cond(|w| w.token.eq("session-1"))
    .execute(&client)
    .await?;
```

## Next steps

- [**`delete_many::&lt;M&gt;()`**](/v0.0.1/orm/delete-many): batch removal.
- [**`find_many::&lt;M&gt;()`**](/v0.0.1/orm/find-many): validate records before removing.
