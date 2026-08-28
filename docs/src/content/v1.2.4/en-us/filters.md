# Filters

Generated `Where` types expose only real entity fields. Each field's Rust type limits accepted values, avoiding fragile runtime conversions.

## Build a where clause

```rust
let users = dinoco::find_many::<User>()
    .where_(|user| user.active.eq(true))
    .where_(|user| user.age.gte(18))
    .execute(&client)
    .await?;
```

Repeated `where_` calls are combined with `AND`. The same syntax works in finds, includes, counts, updates, and deletes.

## Common operators

| Method | SQL |
| --- | --- |
| `eq(value)` | `field = value` |
| `neq(value)` | `field <> value` |
| `gt(value)` | `field > value` |
| `gte(value)` | `field >= value` |
| `lt(value)` | `field < value` |
| `lte(value)` | `field <= value` |
| `batch(values)` | `field IN (...)` |
| `null()` | `field IS NULL` |
| `not_null()` | `field IS NOT NULL` |

Values are sent as adapter parameters and are never interpolated into SQL.

## String operators

`String` and `Option<String>` add:

```rust
dinoco::find_many::<User>().where_(|user| user.email.like("dinoco"));
dinoco::find_many::<User>().where_(|user| user.email.starts_with("support"));
dinoco::find_many::<User>().where_(|user| user.email.ends_with("@example.com"));
```

`like` adds `%` on both sides, `starts_with` on the right, and `ends_with` on the left. Do not add those wildcards manually.

Fields declared with `@fulltext` also expose `.fulltext(term)`. Regular strings do not have that method. See [Full-text search](/v1.2.4/orm/full-text-search).

## Numeric and temporal ranges

Integer, float, `DateTime`, `Date`, and their optional forms support inclusive ranges:

```rust
let users = dinoco::find_many::<User>()
    .where_(|user| user.age.between(18, 65))
    .execute(&client)
    .await?;
```

## Null checks

```rust
let tasks = dinoco::find_many::<Task>()
    .where_(|task| task.owner_id.null())
    .execute(&client)
    .await?;
```

Use `not_null()` when a row must have a populated foreign key.

## Batch values

```rust
let users = dinoco::find_many::<User>()
    .where_(|user| user.id.batch(["user-a", "user-b"]))
    .execute(&client)
    .await?;
```

`batch` emits `IN (...)` and also supplies multiple source keys for pivot `connect` and `disconnect`.

## Combine conditions

```rust
let users = dinoco::find_many::<User>()
    .where_(|user| user.office.eq("engineering"))
    .where_(|user| user.age.lt(30))
    .where_(|user| user.email.ends_with("@example.com"))
    .execute(&client)
    .await?;
```

For precedence with `AND`, `OR`, and `NOT`, see the dedicated [Where complex](/v1.2.4/orm/where-complex) page.
