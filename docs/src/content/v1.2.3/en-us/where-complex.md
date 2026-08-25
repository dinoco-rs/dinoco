# Where complex

`where_complex` builds a boolean tree with explicit parentheses. Use it when a sequence of `where_` calls joined by `AND` cannot represent the query rule.

## Understand x and m

```rust
.where_complex(|x, m| {
    // x: AccountWhere
    // m: group manipulator
})
```

`x` is the model's generated `EntityWhere`. It exposes fields and their typed operators.

`m` builds the logical structure:

| Method | Use |
| --- | --- |
| `m.and([a, b, ...])` | Every condition |
| `m.or(a, b)` | Either condition |
| `m.or_many([a, b, ...])` | Any listed condition |
| `m.not(a)` | Negate a condition or group |

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

Each group keeps its SQL parentheses. Values remain adapter parameters in tree order.

## Where complex replaces where

When `where_complex` is present, every `where_` on that builder is ignored regardless of call order:

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|x| x.active.eq(false)) // ignored
    .where_complex(|x, m| {
        m.or(x.role.eq(Role::ADMIN), x.active.eq(true))
    })
    .where_(|x| x.deleted_at.not_null()) // ignored
    .execute(&client)
    .await?;
```

A second `where_complex` call replaces the previous tree.

## Supported builders

`where_complex` works on:

- `find_first`;
- `find_many`;
- `find_and_update`;
- `one` and `many` relations inside `includes`.

When `find_and_update` executes with `tx`, the same complex condition is compiled into the transactional `UPDATE`.

The same rule for ignoring `where_` applies to relation builders.

## Combine with full-text

`@fulltext` fields can participate in any group:

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

Avoid constructing `and([])` or `or_many([])` from empty input. Build the condition list in application code and explicitly choose whether to omit the filter, return no rows, or execute a non-empty group.
