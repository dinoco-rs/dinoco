# Count

`count::<M>()` retorna uma struct gerada, não apenas um inteiro. `total` sempre é preenchido; counts de relação só aparecem quando solicitados.

## Conte um model

```rust
let count = dinoco::count::<User>()
    .execute(&client)
    .await?;

println!("{} users", count.total);
```

O tipo retornado é `UserCount`, e `total` é `i64`.

## Filtre o count

```rust
let active = dinoco::count::<User>()
    .where_(|x| x.active.eq(true))
    .where_(|x| x.email.ends_with("@example.com"))
    .execute(&client)
    .await?;
```

## Conte relações

```rust
let result = dinoco::count::<User>()
    .includes(|x| {
        x.tokens()
            .where_(|token| token.is_expired.eq(false))
    })
    .execute(&client)
    .await?;

println!("users: {}", result.total);
println!("tokens: {}", result.tokens.unwrap().total);
```

Sem `.includes(|x| x.tokens())`, `result.tokens` fica `None`. `.count(...)` é um alias de `.includes(...)`, e relações aninhadas também podem ser escolhidas:

```rust
let result = dinoco::count::<User>()
    .count(|x| x.posts().count(|post| post.comments()))
    .execute(&client)
    .await?;
```

## Tipos de count gerados

O derive gera uma forma equivalente a:

```rust
pub struct UserCount {
    pub total: i64,
    pub tokens: Option<UserTokenCount>,
    pub posts: Option<PostCount>,
}
```

Cada nível possui seu `total` e seus próprios counts opcionais, sem executar branches não pedidas.
