# Select and includes

Use a selection to reduce the scalar columns returned by a query. Use an include to populate relation fields. They solve different problems and can be composed when the projection declares the required relation shape.

## Custom selections

Define a projection with `EntityExtend` and point it to its source entity:

```rust
use dinoco::EntityExtend;

#[derive(Debug, EntityExtend)]
#[extend(User)]
pub struct UserSummary {
    pub id: dinoco::Uuid,
    pub email: String,
}
```

Then select it from either find builder:

```rust
let users = find_many::<User>()
    .select::<UserSummary>()
    .order_by(|x| x.email.asc())
    .execute(&client)
    .await?;
```

There is no separate select macro or manual row mapping. The derive implements native row conversions for every supported adapter.

## Include a relation

Relation methods are generated from the entity schema:

```rust
let users = find_many::<User>()
    .includes(|x| x.tokens())
    .execute(&client)
    .await?;

let token = find_first::<UserToken>()
    .includes(|x| x.user())
    .execute(&client)
    .await?;
```

An include is explicit: without it, relation fields retain their generated empty value (`Vec::new()` or `None`).

## Filter an include

Relation builders support typed filters. A `many` relation also supports ordering, projection, pagination, and nested includes:

```rust
let users = find_many::<User>()
    .includes(|x| {
        x.tokens()
            .where_(|token| token.is_expired.eq(false))
            .order_by(|token| token.id.desc())
            .take(5)
            .skip(0)
    })
    .execute(&client)
    .await?;
```

`.take(5)` applies per parent, not to one global child result. The SQL compiler uses a window partition in the batched relation query so each user receives at most five tokens.

## Complex nested include

```rust
let projects = dinoco::find_many::<Project>()
    .where_(|project| project.archived.eq(false))
    .order_by(|project| project.created_at.desc())
    .includes(|project| project.owner())
    .includes(|project| {
        project
            .tasks()
            .where_(|task| task.priority.gte(5))
            .order_by(|task| task.priority.desc())
            .take(10)
            .includes(|task| task.assignee())
    })
    .take(25)
    .execute(&client)
    .await?;
```

This limits the page to 25 projects and separately limits each project to ten tasks.

## How relations are loaded

Dinoco chooses a strategy from relation cardinality:

- `one` relations use a left-join query.
- `many` relations use a batched data loader with all parent keys in one query.
- sibling includes are awaited in parallel.
- nested includes repeat the same strategy at the next level.

The loader always carries the relation key separately from selected fields. A custom child projection does not need to expose its foreign key merely so Dinoco can group rows back onto parents.

This design bounds query growth by relation depth and included branches rather than parent row count, avoiding the classic N+1 pattern.

## Practical guidance

Select only when a smaller return type is useful to the caller. Returning the full entity is often clearer for internal operations. Include only relations needed for this request, and use relation-level `take` for unbounded collections.

For consistency-sensitive reads, append `.read_in_primary()` to the parent find. The parent and every include will use the primary together.
