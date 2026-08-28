# Delete

`delete` exige um filtro em compile time. `delete_many` permite operações em massa, inclusive uma remoção intencional da tabela inteira.

## Remova um registro

```rust
dinoco::delete::<User>()
    .where_(|x| x.id.eq(&user_id))
    .execute(&client)
    .await?;
```

Vários `.where_` são permitidos e combinados com `AND`.

## O filtro obrigatório

Antes de `.where_(...)`, o builder não possui method `.execute()`. Portanto, este código não compila:

```rust
// Inválido de propósito.
dinoco::delete::<User>()
    .execute(&client)
    .await?;
```

Esse typestate evita a forma mais comum de apagar uma tabela por engano.

## Remova vários registros

```rust
dinoco::delete_many::<Session>()
    .where_(|x| x.expires_at.lt(cutoff))
    .execute(&client)
    .await?;
```

Sem filtro, `delete_many` apaga todas as rows. A API permite isso para jobs de limpeza, mas a chamada deve ser tratada como operação crítica no review.

## Retorne os dados removidos

```rust
let deleted = dinoco::delete::<User>()
    .where_(|x| x.id.eq(&user_id))
    .returning::<UserSummary>()
    .execute(&client)
    .await?;
```

Com `returning`, ambos os builders retornam `Vec<S>`. Sem ele, retornam `()`.

## Relações e ações referenciais

O banco aplica a ação declarada na migration: `Cascade` remove dependentes, `Restrict`/`NoAction` podem rejeitar, `SetNull` desvincula relações opcionais e `SetDefault` aplica o default. O runtime não substitui silenciosamente essa regra.
