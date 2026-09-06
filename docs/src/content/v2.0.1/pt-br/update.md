# Update

Builders de update mantêm duas coisas deliberadamente separadas: quais rows tocar (`where_`) e o que mudar nelas (`update`). Cada closure de `.update(...)` escolhe exatamente um field gerado, e seu method `.set(...)` só aceita o tipo Rust de verdade daquele field — não tem como atribuir acidentalmente uma `String` numa coluna `Integer`.

## Atualize um registro

```rust
dinoco::update::<User>()
    .where_(|x| x.id.eq(&user_id))
    .update(|x| x.name.set("Ana Silva".to_string()))
    .execute(&client)
    .await?;
```

Chame `.update(...)` mais de uma vez para mudar vários fields na mesma instrução:

```rust
dinoco::update::<User>()
    .where_(|x| x.id.eq(&user_id))
    .update(|x| x.name.set("Ana Silva"))
    .update(|x| x.active.set(true))
    .execute(&client)
    .await?;
```

`update` exige pelo menos um `.update(...)` — não existe update que não muda nada. Fields de relação nunca aparecem aqui; `.set(...)` só existe em fields escalares e enums.

## Atualize vários registros

`update_many` é exatamente a mesma API, aplicada a toda row que o filtro bater em vez de assumir uma só:

```rust
dinoco::update_many::<User>()
    .where_(|x| x.office.eq("support"))
    .update(|x| x.active.set(false))
    .execute(&client)
    .await?;
```

> [!WARNING]
> O type state de `update_many` **não** obriga uma chamada `.where_(...)` da forma que `delete` obriga. Um `update_many` sem filtro nenhum atualiza toda linha da tabela. Seja tão deliberado com um bulk update sem filtro quanto seria com um `DELETE FROM tabela` sem `WHERE`.

## Defina fields opcionais

O `.set(...)` de um field opcional recebe `Option<T>` — `Some(valor)` para atribuir um valor, `None` para gravar SQL `NULL`:

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

Tente isso num field *obrigatório* e simplesmente não compila — o `.set(...)` gerado para um escalar obrigatório recebe `T`, não `Option<T>`, então não tem `None` para passar por acidente.

## Retorne uma projeção

`.returning::<S>()` sempre volta como um `Vec`, mesmo conceitualmente em `update`, porque o filtro ao qual ele está ligado ainda pode bater com mais de uma row:

```rust
let changed = dinoco::update::<User>()
    .where_(|x| x.office.eq("support"))
    .update(|x| x.active.set(false))
    .returning::<UserSummary>()
    .execute(&client)
    .await?;
```

## Conecte e desconecte many-to-many

O Dinoco gera um field de ID virtual do alvo em cada endpoint de uma relação many-to-many implícita. Filtre o endpoint que está atualizando normalmente, e conecte ou desconecte por esse field virtual como qualquer outro update:

```rust
dinoco::update::<Post>()
    .where_(|x| x.id.eq(&post_id))
    .update(|x| x.tag_id.connect(&tag_id))
    .execute(&client)
    .await?;
```

Desconectar o mesmo par é idêntico, só com `.disconnect(...)`:

```rust
dinoco::update_many::<Post>()
    .where_(|x| x.id.eq(&post_id))
    .update(|x| x.tag_id.disconnect(&tag_id))
    .execute(&client)
    .await?;
```

> [!TIP]
> Precisa atualizar exatamente uma row e receber a entity completa e atualizada de volta sem recorrer a `.returning(...)` separadamente? É exatamente para isso que serve o [Find and update](/pt-br/docs/orm/orm/find-and-update).

Esses mesmos fields virtuais funcionam de forma idêntica em `update`, `update_many` e `find_and_update`, `.returning::<T>()` incluso nos dois primeiros. Rode o builder com `.execute(tx)` dentro de `transaction(&client, |tx| ...)` para que o write do endpoint e a mudança na pivô recebam commit ou rollback como uma única unidade.

Quando o endpoint que você está vinculando está sendo criado no mesmo fluxo, você pode pular o update separado por completo: preencha seu `Option<Id>` virtual antes de `insert_into` (ou em cada item aplicável de `insert_many`), e o Dinoco cria o vínculo na pivô logo depois de inserir o endpoint — veja [Insert](/pt-br/docs/orm/orm/insert#conecte-many-to-many-durante-o-insert).

Veja [Many-to-many implícito](/pt-br/docs/orm/guide/relations#many-to-many-implicito) para a API completa dos endpoints e a semântica exata da tabela pivô.
