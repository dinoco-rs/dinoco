# Select

`select::<S>()` narrows a query down to a smaller set of scalar columns and swaps the result type for a generated projection struct, instead of the full entity.

## 1. Declare a projection

Derive `EntityExtend` and point it at the entity the projection is drawn from:

```rust
use dinoco::EntityExtend;

#[derive(Debug, EntityExtend)]
#[extend(Account)]
pub struct AccountSummary {
    pub id: dinoco::Uuid,
    pub email: String,
}
```

Every field's name and type must match a scalar field on the source entity exactly — a projection isn't a place to rename or reshape data, only to pick a subset of it.

## 2. Select in a find

```rust
let accounts = dinoco::find_many::<Account>()
    .select::<AccountSummary>()
    .order_by(|account| account.email.asc())
    .execute(&client)
    .await?;
```

The return type becomes `Vec<AccountSummary>`; with `find_first` it becomes `Option<AccountSummary>` instead. `EntityExtend` generates a native row conversion for SQLite, PostgreSQL, and MySQL as part of the derive — there's no manual `From<Row>` implementation to write for a projection any more than there is for a full entity.

## 3. Combine with filters

`select` narrows what comes *back*, not what you can filter *on* — `EntityWhere` is unaffected, so filters keep using fields from the original model, not the projection:

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.active.eq(true))
    .select::<AccountSummary>()
    .execute(&client)
    .await?;
```

The same is true for `where_complex` and `.fulltext(...)` — both operate on the full model regardless of what you eventually `.select(...)`.

## Select relations

A projection can declare a compatible relation shape and then load it with `includes(...)`, the same way a full entity does. If a projection *doesn't* declare a given relation, don't try to `.includes(...)` it in that same query — there's no field on the projection for the loaded data to go into.

> [!TIP]
> When a related model is also using `select`, the include loader still carries the relation's foreign key internally to group rows correctly — the projection itself doesn't need to expose that key as a field just so Dinoco can do the grouping. Keep your projections limited to the data you actually want to read.
