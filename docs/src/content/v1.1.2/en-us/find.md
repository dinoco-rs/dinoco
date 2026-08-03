# Query overview

Dinoco read builders are lazy: chaining methods describes the query, while `.execute(&client).await` compiles adapter SQL and performs I/O.

## Choose the builder

| Goal | Builder | Default return |
| --- | --- | --- |
| Fetch zero or one row | `find_first::<M>()` | `Option<M>` |
| Fetch several rows | `find_many::<M>()` | `Vec<M>` |
| Update and return one row | `find_and_update::<M>()` | `M` |
| Count rows | `count::<M>()` | `MCount` |

Start with the dedicated page:

- [Find first](/v1.1.2/orm/find-first)
- [Find many](/v1.1.2/orm/find-many)
- [Find and update](/v1.1.2/orm/find-and-update)
- [Count](/v1.1.2/orm/count)

## Query stages

A read commonly follows four stages:

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.active.eq(true)) // 1. filter
    .order_by(|account| account.id.asc())      // 2. order
    .take(25)                                  // 3. limit
    .execute(&client)                          // 4. execute
    .await?;
```

Only `execute` accesses the database.

## Shared features

`find_first` and `find_many` share:

- typed `where_` filters;
- boolean `where_complex` groups;
- `.fulltext(...)` on configured fields;
- `select::<S>()`;
- `includes(...)`;
- `order_by(...)`;
- `read_in_primary()`;
- execution inside `Transaction`.

`find_many` additionally supports `take` and `skip`.

## Next steps

After choosing a builder, see:

- [Filters](/v1.1.2/orm/filters) for simple operators;
- [Where complex](/v1.1.2/orm/where-complex) for `AND`, `OR`, and `NOT`;
- [Full-text search](/v1.1.2/orm/full-text-search);
- [Select](/v1.1.2/orm/select);
- [Includes](/v1.1.2/orm/includes);
- [Transactions](/v1.1.2/orm/transactions).
