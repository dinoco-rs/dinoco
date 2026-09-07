# Count

`count::<M>()` returns a generated count struct, not a bare integer. Its `total` field is always populated; relation-count fields only show up when you specifically ask for them.

## Count a model

```rust
let count = dinoco::count::<User>()
    .execute(&client)
    .await?;

println!("{} users", count.total);
```

The return type is `UserCount`, generated specifically for `User`. `total` is an `i64`, matching the aggregate type every supported database returns for `COUNT(*)`.

## Filter the count

Filtering a count uses the exact same typed filters as a find:

```rust
let active = dinoco::count::<User>()
    .where_(|x| x.active.eq(true))
    .where_(|x| x.email.ends_with("@example.com"))
    .execute(&client)
    .await?;
```

These filters constrain `total` itself, and also set the context that any relation counts you request are computed within.

## Count relations

Request a relation's count the same way you'd load it with `includes(...)`. The relation builder still accepts its own typed filters, so a relation count can be narrower than — or filtered independently of — the parent count:

```rust
let result = dinoco::count::<User>()
    .where_(|x| x.active.eq(true))
    .includes(|x| {
        x.tokens()
            .where_(|token| token.is_expired.eq(false))
    })
    .execute(&client)
    .await?;

println!("users: {}", result.total);
println!("active tokens: {}", result.tokens.unwrap());
```

> [!NOTE]
> Without `.includes(|x| x.tokens())`, `result.tokens` stays `None` — Dinoco never counts a relation implicitly just because it exists on the model. You always opt in, one relation at a time.

## Generated count types

For an entity with relation fields, the derive generates a struct conceptually shaped like this:

```rust
pub struct UserCount {
    pub total: i64,
    pub tokens: Option<i64>,
    pub posts: Option<i64>,
}
```

Alongside it, the derive generates an internal `UserCountInclude` selector — that's what the `.includes(...)` closure actually receives, and it's where the relation methods (`.tokens()`, `.posts()`) live, not on the `UserCount` you get back. Every relation you request comes back `Some(total)`; every relation you don't request stays `None`, distinguishable from "zero related rows."
