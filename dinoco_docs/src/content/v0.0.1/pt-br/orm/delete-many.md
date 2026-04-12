# delete_many

Usado para deletar vários registros com uma ou mais condições.

---

## O que você pode fazer

- `.cond(...)`: adiciona filtros para a remoção em lote.
- `.execute(&client)`: executa a remoção dos registros correspondentes.

## Retorno

O retorno de `delete_many` é:

```rust
DinocoResult<()>
```

## Exemplo básico

Isso deleta apenas os registros que atendem à condição.

```rust
dinoco::delete_many::<Session>()
    .cond(|w| w.expiresAt.lt(dinoco::Utc::now()))
    .execute(&client)
    .await?;
```

## Exemplo sem filtro

Isso deleta todos os dados da tabela.

```rust
dinoco::delete_many::<TaskAssignees>()
    .execute(&client)
    .await?;
```

## Próximos passos

- [**`delete::&lt;M&gt;()`**](/v0.0.1/orm/delete): remoção pontual com filtro explícito.
- [**`find_many::&lt;M&gt;()`**](/v0.0.1/orm/find-many): listar antes de remover em lote.
