# find_and_update

Used to locate a single record, apply atomic updates to the database, and return the updated item.

---

## What you can do

- `.cond(...)`
- `.update(...)`
- `.execute(&client)`

## Method descriptions

- `.cond(...)`: defines which record will be located.
- `.update(...)`: applies an atomic operation to a model field.
- `.execute(&client)`: executes the update and returns the updated record.

## Return

The return of `find_and_update` is:

```rust
DinocoResult<M>
```

## Basic example

```rust
let task = dinoco::find_and_update::<Task>()
    .cond(|x| x.id.eq(task_id.clone()))
    .update(|x| x.status.set(TaskStatus::REVIEW))
    .execute(&client)
    .await?;
```

## Available operations in `ModelUpdate`

- `set(value)`
- `increment(value)`
- `decrement(value)`
- `multiply(value)`
- `division(value)`

## Notes

- The update is executed in a single `UPDATE`.
- If no row matches the condition, the return will be `DinocoError::RecordNotFound`.
- The update DSL does not expose relationships.
- Currently, the flow supports simple primary keys to locate and return the updated record.

## Next steps

- [**`update::&lt;M&gt;()`**](/v0.0.1/orm/update): traditional update.
- [**`update_many::&lt;M&gt;()`**](/v0.0.1/orm/update-many): batch update.
