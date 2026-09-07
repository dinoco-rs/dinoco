# Filters

Every generated `Where` type only exposes methods for fields that actually exist on the entity — there's no string-keyed `.where("some_column", ...)` escape hatch. Each field's Rust type also constrains which values compile against it, so a filter comparing a `DateTime` field against a `String` fails at compile time rather than as a confusing runtime database error.

## Build a where clause

```rust
let users = dinoco::find_many::<User>()
    .where_(|user| user.active.eq(true))
    .where_(|user| user.age.gte(18))
    .execute(&client)
    .await?;
```

Repeated `where_` calls combine with `AND` — this same pattern works identically across finds, includes, counts, updates, and deletes, so once you've learned it here, it's the same everywhere else.

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

Every value passed to these is sent as a bound adapter parameter — never interpolated directly into the SQL string, which is what keeps this API immune to SQL injection by construction, not by convention.

## String operators

`String` and `Option<String>` fields get three more:

```rust
dinoco::find_many::<User>().where_(|user| user.email.like("dinoco"));
dinoco::find_many::<User>().where_(|user| user.email.starts_with("support"));
dinoco::find_many::<User>().where_(|user| user.email.ends_with("@example.com"));
```

`like` wraps your value in `%` on both sides, `starts_with` adds it only on the right, and `ends_with` only on the left — don't add the `%` wildcards yourself, or you'll end up searching for a literal `%` character.

A field declared with `@fulltext` additionally exposes `.fulltext(term)` — a plain `String` field without that attribute doesn't have this method at all, so there's no way to accidentally call full-text search against an unindexed column. See [Full-text search](/en-us/docs/orm/orm/full-text-search).

## Numeric and temporal ranges

`Integer`, `Float`, `DateTime`, `Date`, and their optional forms all support an inclusive range check:

```rust
let users = dinoco::find_many::<User>()
    .where_(|user| user.age.between(18, 65))
    .execute(&client)
    .await?;
```

## Null checks

```rust
let tasks = dinoco::find_many::<Task>()
    .where_(|task| task.owner_id.null()) // SQL: owner_id IS NULL
    .execute(&client)
    .await?;
```

Use `not_null()` for the opposite — rows where a nullable foreign key (or any nullable field) is actually populated.

## Batch values

```rust
let users = dinoco::find_many::<User>()
    .where_(|user| user.id.batch(["user-a", "user-b"]))
    .execute(&client)
    .await?;
```

`batch` emits SQL `IN (...)`. It's also what you'll reach for when you need to supply multiple source keys at once for a many-to-many pivot's `connect`/`disconnect` operations.

## Combine conditions

```rust
let users = dinoco::find_many::<User>()
    .where_(|user| user.office.eq("engineering"))
    .where_(|user| user.age.lt(30))
    .where_(|user| user.email.ends_with("@example.com"))
    .execute(&client)
    .await?;
```

> [!NOTE]
> Every example on this page combines conditions with an implicit `AND` — that's the only precedence `where_` understands. The moment you need `OR` or `NOT`, or `AND`/`OR` mixed with explicit grouping, this API can't express it; see [Where complex](/en-us/docs/orm/orm/where-complex) instead.
