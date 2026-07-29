# Update

Os builders separam filtros dos fields alterados. `.set(...)` aceita o mesmo tipo Rust do field gerado.

## Atualize um registro

```rust
dinoco::update::<User>()
    .where_(|x| x.id.eq(&user_id))
    .update(|x| x.name.set("Ana Silva".to_string()))
    .update(|x| x.active.set(true))
    .execute(&client)
    .await?;
```

É obrigatório ter pelo menos um `.update(...)`. Relações não possuem `.set(...)`.

## Atualize vários registros

```rust
dinoco::update_many::<User>()
    .where_(|x| x.office.eq("support"))
    .update(|x| x.active.set(false))
    .execute(&client)
    .await?;
```

Um `update_many` sem filtro altera toda a tabela; revise esse tipo de chamada explicitamente.

## Defina fields opcionais

```rust
dinoco::update::<User>()
    .where_(|x| x.id.eq(&user_id))
    .update(|x| x.bio.set(Some("Maintainer".to_string())))
    .execute(&client)
    .await?;

dinoco::update::<User>()
    .where_(|x| x.id.eq(&user_id))
    .update(|x| x.bio.set(None::<String>))
    .execute(&client)
    .await?;
```

`None` grava SQL `NULL`; fields obrigatórios não aceitam esse valor.

## Retorne uma projeção

```rust
let changed = dinoco::update::<User>()
    .where_(|x| x.office.eq("support"))
    .update(|x| x.active.set(false))
    .returning::<UserSummary>()
    .execute(&client)
    .await?;
```

O retorno é `Vec<UserSummary>`, pois o filtro pode atingir várias rows.

## Conecte e desconecte many-to-many

O Dinoco gera a entity pivot automaticamente. Identifique as source keys com `eq` ou `batch`:

```rust
dinoco::update::<PostTag>()
    .where_(|x| x.post_id.eq(&post_id))
    .update(|x| x.tag_id.connect(&tag_id))
    .execute(&client)
    .await?;

dinoco::update_many::<PostTag>()
    .where_(|x| x.post_id.eq(&post_id))
    .update(|x| x.tag_id.disconnect(&tag_id))
    .execute(&client)
    .await?;
```

`connect`/`disconnect` não combinam com `.returning::<T>()`.

Para atualizar exatamente uma row e receber a entity completa sem `.returning()`, consulte a página dedicada [Find and update](/v1.0.9/orm/find-and-update).
