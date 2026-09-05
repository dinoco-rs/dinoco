# Count

`count::<M>()` retorna uma struct de count gerada, não um inteiro puro. O field `total` sempre é preenchido; fields de count de relação só aparecem quando você pede especificamente por eles.

## Conte um model

```rust
let count = dinoco::count::<User>()
    .execute(&client)
    .await?;

println!("{} users", count.total);
```

O tipo retornado é `UserCount`, gerado especificamente para `User`. `total` é `i64`, batendo com o tipo agregado que todo banco suportado retorna para `COUNT(*)`.

## Filtre o count

Filtrar um count usa exatamente os mesmos filtros tipados de um find:

```rust
let active = dinoco::count::<User>()
    .where_(|x| x.active.eq(true))
    .where_(|x| x.email.ends_with("@example.com"))
    .execute(&client)
    .await?;
```

Esses filtros restringem o `total` em si, e também definem o contexto dentro do qual qualquer count de relação que você pedir é calculado.

## Conte relações

Peça o count de uma relação da mesma forma que você a carregaria com `includes(...)`. O builder da relação ainda aceita seus próprios filtros tipados, então um count de relação pode ser mais restrito do que — ou filtrado independentemente de — o count do parent:

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
> Sem `.includes(|x| x.tokens())`, `result.tokens` permanece `None` — o Dinoco nunca conta uma relação implicitamente só porque ela existe no model. Você sempre opta explicitamente, uma relação de cada vez.

## Tipos de count gerados

Para uma entity com fields de relação, o derive gera uma struct conceitualmente parecida com esta:

```rust
pub struct UserCount {
    pub total: i64,
    pub tokens: Option<i64>,
    pub posts: Option<i64>,
}
```

Junto dela, o derive gera um seletor interno `UserCountInclude` — é isso que a closure de `.includes(...)` de fato recebe, e é ali que os methods de relação (`.tokens()`, `.posts()`) vivem, não no `UserCount` que você recebe de volta. Toda relação que você pede volta como `Some(total)`; toda relação que você não pede fica `None`, distinguível de "zero rows relacionadas".
