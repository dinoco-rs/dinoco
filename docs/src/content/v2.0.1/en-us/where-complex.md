# Where complex

`where_complex` builds a boolean condition tree with explicit, SQL-parenthesized grouping. Reach for it the moment a plain sequence of `AND`-joined `where_` calls can no longer express the rule you actually need — anything involving `OR`, `NOT`, or mixed precedence.

## Understand x and m

```rust
.where_complex(|x, m| {
    // x: AccountWhere
    // m: group manipulator
})
```

`x` is the same generated `EntityWhere` you already know from `where_` — the same fields, the same typed operators, nothing new to learn there.

`m` is what's new: it builds the logical structure around those conditions.

| Method | Use |
| --- | --- |
| `m.and([a, b, ...])` | Every condition must hold |
| `m.or(a, b)` | Either condition holds |
| `m.or_many([a, b, ...])` | Any one of several conditions holds |
| `m.not(a)` | Negates a condition or an entire group |

## Build nested groups

```rust
let account = dinoco::find_first::<Account>()
    .where_complex(|x, m| {
        m.or(
            m.and([
                x.id.eq("id"),
                x.name.eq("matheus"),
            ]),
            m.or(
                m.and([
                    x.id.eq("second-id"),
                    x.name.eq("ana"),
                ]),
                m.and([
                    x.id.eq("third-id"),
                    m.not(x.name.eq("blocked")),
                ]),
            ),
        )
    })
    .execute(&client)
    .await?;
```

Each `m.and`/`m.or` group becomes its own parenthesized group in the generated SQL, nested exactly as deep as you nest them in Rust. Values still travel as adapter parameters, in the same order the tree is built.

## Where complex replaces where

The moment a builder uses `where_complex`, every `where_` on that same builder is ignored — regardless of whether you wrote it before or after the `where_complex` call:

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|x| x.active.eq(false)) // ignored
    .where_complex(|x, m| {
        m.or(x.role.eq(Role::Admin), x.active.eq(true))
    })
    .where_(|x| x.deleted_at.not_null()) // ignored
    .execute(&client)
    .await?;
```

> [!WARNING]
> This silent override is easy to miss in a diff — adding a `where_complex` to a builder that already had `where_` calls doesn't produce a compile error or a runtime warning, it just quietly drops the plain filters. Search for existing `where_` calls before adding `where_complex` to a builder that might already have some. Calling `where_complex` a second time on the same builder replaces the previous tree the same way, not merges with it.

## Supported builders

`where_complex` works on:

- `find_first`
- `find_many`
- `find_and_update`
- `one`/`many` relation builders inside `includes`

When `find_and_update` runs with `tx` inside a transaction, the same complex condition tree is compiled straight into the transactional `UPDATE` — there's no special-casing needed to make it work there too. Relation builders follow the exact same "where_complex silently wins" rule as top-level finds.

## Combine with full-text

`@fulltext` fields participate in a complex group exactly like any other condition:

```rust
let articles = dinoco::find_many::<Article>()
    .where_complex(|article, m| {
        m.and([
            article.body.fulltext("dinoco"),
            m.not(article.body.fulltext("deprecated")),
        ])
    })
    .execute(&client)
    .await?;
```

## Empty groups

> [!WARNING]
> Don't call `and([])` or `or_many([])` with a list built from empty runtime input — an empty `AND` group and an empty `OR` group have opposite, easy-to-get-backwards SQL meanings ("match everything" vs. "match nothing"), and neither is likely to be what you intended. When conditions come from dynamic application input, build the list explicitly and decide up front what an empty list should mean: skip the filter entirely, return no rows, or fall back to a specific non-empty group.
