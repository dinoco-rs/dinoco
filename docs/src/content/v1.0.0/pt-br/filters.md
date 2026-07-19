# Filtros

Os tipos `Where` gerados expõem somente fields reais da entity. O tipo do field limita os valores aceitos, evitando conversões frágeis em runtime.

## Monte uma cláusula where

```rust
let users = dinoco::find_many::<User>()
    .where_(|x| x.active.eq(true))
    .where_(|x| x.age.gte(18))
    .execute(&client)
    .await?;
```

Vários `.where_` são combinados com `AND`. A mesma sintaxe funciona em finds, includes, count, update e delete.

## Operadores comuns

Todo field escalar possui `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `batch`, `null` e `not_null`. Os valores são enviados como parâmetros do adapter, sem interpolar input no SQL.

```rust
let users = dinoco::find_many::<User>()
    .where_(|x| x.age.gte(18))
    .where_(|x| x.age.lt(65))
    .execute(&client)
    .await?;
```

## Operadores de String

`String` e `Option<String>` adicionam:

```rust
dinoco::find_many::<User>().where_(|x| x.email.like("dinoco"));
dinoco::find_many::<User>().where_(|x| x.email.starts_with("support"));
dinoco::find_many::<User>().where_(|x| x.email.ends_with("@example.com"));
```

`like` coloca `%` dos dois lados, `starts_with` à direita e `ends_with` à esquerda. Não inclua os curingas manualmente.

## Ranges numéricos

Inteiros, floats e suas versões opcionais suportam range inclusivo:

```rust
let users = dinoco::find_many::<User>()
    .where_(|x| x.age.between(18, 65))
    .execute(&client)
    .await?;
```

## Verificações de null

```rust
let tasks = dinoco::find_many::<Task>()
    .where_(|x| x.owner_id.null())
    .execute(&client)
    .await?;
```

Use `.not_null()` quando a row precisa ter uma foreign key preenchida.

## Valores em batch

```rust
let users = dinoco::find_many::<User>()
    .where_(|x| x.id.batch(["user-a", "user-b"]))
    .execute(&client)
    .await?;
```

`batch` gera `IN (...)` e também fornece várias keys de origem para `connect` e `disconnect` em pivots.

## Combinando condições

```rust
let users = dinoco::find_many::<User>()
    .where_(|x| x.office.eq("engineering"))
    .where_(|x| x.age.lt(30))
    .where_(|x| x.email.ends_with("@example.com"))
    .execute(&client)
    .await?;
```

Para autorização complexa, centralize a composição do builder em um helper pequeno para não esquecer condições de segurança em handlers diferentes.
