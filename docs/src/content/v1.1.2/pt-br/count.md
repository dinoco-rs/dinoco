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
println!("tokens: {}", result.tokens.unwrap());
```

O builder da relação aceita filtros tipados. Assim, você pode contar somente os registros relacionados que interessam:

```rust
let result = dinoco::count::<Orixa>()
    .where_(|orixa| orixa.is_show.eq(true))
    .includes(|orixa| {
        orixa.questions()
            .where_(|question| question.age.gt(3))
    })
    .execute(&client)
    .await?;
```

Sem `.includes(|x| x.tokens())`, `result.tokens` fica `None`. Relações não são contadas implicitamente.

## Tipos de count gerados

O derive gera uma forma equivalente a:

```rust
pub struct UserCount {
    pub total: i64,
    pub tokens: Option<i64>,
    pub posts: Option<i64>,
}
```

O derive também cria internamente um seletor `UserCountInclude` para o callback de `.includes(...)`. Os métodos de relação ficam nesse seletor, não no `UserCount` retornado. Cada relação solicitada recebe `Some(total)`; as omitidas permanecem `None`.
