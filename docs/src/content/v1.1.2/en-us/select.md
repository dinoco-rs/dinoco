# Select

`select::<S>()` reduces returned scalar columns and changes the result type to a generated projection.

## 1. Declare a projection

Use `EntityExtend` and identify the source entity:

```rust
use dinoco::EntityExtend;

#[derive(Debug, EntityExtend)]
#[extend(Account)]
pub struct AccountSummary {
    pub id: dinoco::Uuid,
    pub email: String,
}
```

Names and types must match scalar fields from the entity.

## 2. Select in a find

```rust
let accounts = dinoco::find_many::<Account>()
    .select::<AccountSummary>()
    .order_by(|account| account.email.asc())
    .execute(&client)
    .await?;
```

The return type is `Vec<AccountSummary>`. With `find_first`, it is `Option<AccountSummary>`.

The derive implements native row conversion for SQLite, PostgreSQL, and MySQL; no manual mapping is required.

## 3. Combine with filters

`select` does not change `EntityWhere`: filters still use fields from the original model.

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.active.eq(true))
    .select::<AccountSummary>()
    .execute(&client)
    .await?;
```

The same applies to `where_complex` and `fulltext`.

## Select relations

A projection may declare a compatible relation shape and then use `includes(...)`. If the projection does not declare that relation, do not include it in that result.

When a child also uses `select`, the loader carries its relation key separately. The projection does not need to expose the foreign key merely so Dinoco can group rows.

## Select in transactions

`find_first::<M>().select::<S>()` and `find_many::<M>().select::<S>()` preserve their types in a transaction. Read the results as `Option<S>` or `Vec<S>`.

Includes remain unavailable inside a v1.1.2 batch.
