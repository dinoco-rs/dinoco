# Filters

Generated `Where` types expose only fields that belong to the entity. Each field carries its Rust type, so unsupported values fail during compilation instead of being converted through strings at runtime.

## Build a where clause

Pass a closure to `.where_` and return one field predicate:

```rust
let users = find_many::<User>()
    .where_(|x| x.active.eq(true))
    .where_(|x| x.age.gte(18))
    .execute(&client)
    .await?;
```

Multiple calls are joined with `AND`. The same closure style is shared by finds, includes, counts, updates, and deletes.

## Common operators

Every scalar field supports:

| Method | SQL meaning |
| --- | --- |
| `.eq(value)` | `field = value` |
| `.neq(value)` | `field <> value` |
| `.gt(value)` | `field > value` |
| `.gte(value)` | `field >= value` |
| `.lt(value)` | `field < value` |
| `.lte(value)` | `field <= value` |
| `.batch(values)` | `field IN (...)` |
| `.null()` | `field IS NULL` |
| `.not_null()` | `field IS NOT NULL` |

Values become adapter parameters. Dinoco does not interpolate user input into the generated SQL.

## String operators

`String` and `Option<String>` fields add three convenience filters:

```rust
let matching = find_many::<User>()
    .where_(|x| x.email.like("dinoco"))
    .execute(&client)
    .await?;

let prefixed = find_many::<User>()
    .where_(|x| x.email.starts_with("support"))
    .execute(&client)
    .await?;

let company = find_many::<User>()
    .where_(|x| x.email.ends_with("@example.com"))
    .execute(&client)
    .await?;
```

`like("word")` adds `%` on both sides, `starts_with` adds it on the right, and `ends_with` adds it on the left. Do not add those wildcards yourself.

## Numeric ranges

Integer and float fields, including optional numeric fields, support `.between(start, end)`:

```rust
let adults = find_many::<User>()
    .where_(|x| x.age.between(18, 65))
    .execute(&client)
    .await?;
```

The adapter compiler emits the dialect's range predicate. The boundary values are inclusive, matching SQL `BETWEEN` semantics.

## Null checks

Use null-specific methods instead of comparing to a string or sentinel:

```rust
let unassigned = find_many::<Task>()
    .where_(|x| x.owner_id.null())
    .execute(&client)
    .await?;
```

`.not_null()` is useful when loading rows that must have a relation key.

## Batch values

`.batch` accepts any iterator of values convertible to the field value:

```rust
let users = find_many::<User>()
    .where_(|x| x.id.batch(["user-a", "user-b", "user-c"]))
    .execute(&client)
    .await?;
```

It is also the supported way to provide multiple source IDs for a many-to-many `connect` or `disconnect` update.

## Combining conditions

The v1.0.7 builder combines repeated `.where_` calls with `AND`:

```rust
let users = find_many::<User>()
    .where_(|x| x.office.eq("engineering"))
    .where_(|x| x.age.lt(30))
    .where_(|x| x.email.ends_with("@example.com"))
    .execute(&client)
    .await?;
```

Keep complex authorization predicates in a small application helper that returns or applies a complete builder. This makes the query visible and avoids duplicating security conditions across handlers.
