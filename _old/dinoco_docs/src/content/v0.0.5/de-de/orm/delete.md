# delete

Usado para deletar com filtro explícito.

---

## O que você pode fazer

- `.cond(...)`: define qual registro será removido.
- `.execute(&client)`: executa a remoção no banco.

## Retorno

O retorno de `delete` é:

```rust
DinocoResult<()>
```

## Exemplo básico

```rust
dinoco::delete::<User>()
    .cond(|w| w.id.eq(10))
    .execute(&client)
    .await?;
```

## Exemplo com outro filtro

```rust
dinoco::delete::<Session>()
    .cond(|w| w.token.eq("session-1"))
    .execute(&client)
    .await?;
```

## Próximos passos

- [**`delete_many::&lt;M&gt;()`**](/v0.0.1/orm/delete-many): remoção em lote.
- [**`find_many::&lt;M&gt;()`**](/v0.0.1/orm/find-many): validar registros antes de remover.
