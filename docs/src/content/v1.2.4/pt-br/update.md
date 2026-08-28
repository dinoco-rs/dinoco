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

O Dinoco gera um ID virtual do model oposto em cada endpoint. Filtre o endpoint normalmente e conecte por esse campo virtual:

```rust
dinoco::update::<Post>()
    .where_(|x| x.id.eq(&post_id))
    .update(|x| x.tag_id.connect(&tag_id))
    .execute(&client)
    .await?;

dinoco::update_many::<Post>()
    .where_(|x| x.id.eq(&post_id))
    .update(|x| x.tag_id.disconnect(&tag_id))
    .execute(&client)
    .await?;
```

Os mesmos campos funcionam em `update`, `update_many` e `find_and_update`, inclusive com `.returning::<T>()` nos dois primeiros. Execute o builder com `.execute(tx)` dentro de `transaction(&client, |tx| ...)` para que o write do endpoint e as alterações na pivô recebam commit ou rollback juntos.

Quando um endpoint ainda será criado, o update separado é dispensável: preencha seu `Option<Id>` virtual antes de `insert_into`, ou em cada item aplicável de `insert_many`, e o Dinoco cria o vínculo depois de inserir o endpoint.

Consulte [Many-to-many implícito](/v1.2.4/guide/relations#many-to-many-implícito) para a API completa pelos endpoints e as regras da tabela pivô.

Para atualizar exatamente uma row e receber a entity completa sem `.returning()`, consulte a página dedicada [Find and update](/v1.2.4/orm/find-and-update).
