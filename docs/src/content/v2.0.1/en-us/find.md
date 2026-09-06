# Query overview

Every Dinoco read builder is lazy: chaining `.where_(...)`, `.order_by(...)`, `.take(...)` and the rest only *describes* the query. Nothing touches the database until `.execute(&client).await` runs — which compiles the description into the active adapter's SQL and performs the actual I/O.

## Choose the builder

| Goal | Builder | Default return |
| --- | --- | --- |
| Fetch zero or one row | `find_first::<M>()` | `Option<M>` |
| Fetch several rows | `find_many::<M>()` | `Vec<M>` |
| Update and return one row | `find_and_update::<M>()` | `M` |
| Count rows | `count::<M>()` | `MCount` |

Jump straight to the page for the builder you need:

- [Find first](/en-us/docs/orm/orm/find-first)
- [Find many](/en-us/docs/orm/orm/find-many)
- [Find and update](/en-us/docs/orm/orm/find-and-update)
- [Count](/en-us/docs/orm/orm/count)

## Query stages

Most reads follow the same four stages, in the same order:

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.active.eq(true)) // 1. filter
    .order_by(|account| account.id.asc())      // 2. order
    .take(25)                                  // 3. limit
    .execute(&client)                          // 4. execute
    .await?;
```

> [!NOTE]
> Only step 4, `.execute(...)`, ever reaches the database. Everything before it is just building up a value in memory — you can assign a partially built query to a variable, pass it to a function, and keep chaining onto it later, all without a single network round-trip happening.

## Shared features

`find_first` and `find_many` share the same building blocks:

- typed `where_` filters;
- boolean `where_complex` groups;
- `.fulltext(...)` on fields configured for it;
- `select::<S>()`;
- `includes(...)`;
- `order_by(...)`;
- `read_in_primary()`.

`find_many` additionally supports `take` and `skip`, since pagination only makes sense when more than one row can come back.

## Next steps

Once you've picked a builder, these cover the pieces that plug into it:

- [Filters](/en-us/docs/orm/orm/filters) for the simple operators (`eq`, `gt`, `like`, and so on).
- [Where complex](/en-us/docs/orm/orm/where-complex) for explicit `AND`, `OR`, and `NOT` grouping.
- [Full-text search](/en-us/docs/orm/orm/full-text-search).
- [Select](/en-us/docs/orm/orm/select) for typed projections.
- [Includes](/en-us/docs/orm/orm/includes) for loading relations.
- [Transactions](/en-us/docs/orm/orm/transactions) for grouping reads and writes atomically.
