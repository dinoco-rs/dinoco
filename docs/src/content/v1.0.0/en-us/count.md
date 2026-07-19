# Count

`count::<M>()` returns a generated count struct rather than a bare integer. Its `total` field is always populated, while relation count fields appear only when you request them.

## Count a model

```rust
let count = dinoco::count::<User>()
    .execute(&client)
    .await?;

println!("{} users", count.total);
```

The return type is `UserCount`. `total` is an `i64`, matching the aggregate values returned by supported databases.

## Filter the count

Use the same typed filters as find:

```rust
let active = dinoco::count::<User>()
    .where_(|x| x.active.eq(true))
    .where_(|x| x.email.ends_with("@example.com"))
    .execute(&client)
    .await?;
```

The filters constrain the parent total and provide context for requested relation counts.

## Count relations

Request a relation through `.includes(...)` or its `.count(...)` alias:

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
println!("active tokens: {}", result.tokens.unwrap().total);
```

Without `.includes(|x| x.tokens())`, `result.tokens` remains `None`. Dinoco does not count every relation implicitly.

Nested relation counts can be selected from the relation count builder:

```rust
let result = dinoco::count::<User>()
    .count(|x| x.posts().count(|post| post.comments()))
    .execute(&client)
    .await?;
```

## Generated count types

For an entity with relation fields, the derive generates a shape conceptually like:

```rust
pub struct UserCount {
    pub total: i64,
    pub tokens: Option<UserTokenCount>,
    pub posts: Option<PostCount>,
}
```

Each nested count type can carry its own `total` and relation counts. This makes requested aggregate structure explicit in the Rust type while avoiding work for omitted branches.
