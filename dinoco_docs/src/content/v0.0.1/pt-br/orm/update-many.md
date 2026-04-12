# update_many

Usado para atualizar vários registros de uma vez.

---

## O que você pode fazer

- `.cond(...)`: restringe quais registros podem ser atualizados.
- `.values(Vec&lt;M&gt;)`: define os itens usados na atualização em lote.
- `.returning::&lt;T&gt;()`: retorna os registros atualizados como uma lista tipada.
- `.execute(&client)`: executa o update em lote.

## Retorno

Sem `.returning::&lt;T&gt;()`, o retorno é:

```rust
DinocoResult<()>
```

Com `.returning::&lt;T&gt;()`, o retorno passa a ser:

```rust
DinocoResult<Vec<T>>
```

## Exemplo básico

```rust
dinoco::update_many::<User>()
    .values(vec![
        User { id: 1, email: "a@acme.com".into(), name: "Ana".into() },
        User { id: 2, email: "b@acme.com".into(), name: "Bia".into() },
    ])
    .execute(&client)
    .await?;
```

## Exemplo com retorno

```rust
let updated = dinoco::update_many::<User>()
    .values(vec![
        User { id: 2, name: "Ana Batch".to_string() },
        User { id: 3, name: "Caio Batch".to_string() },
    ])
    .returning::<User>()
    .execute(&client)
    .await?;
```

## Exemplo com filtro

```rust
dinoco::update_many::<User>()
    .cond(|x| x.active.eq(true))
    .values(vec![
        User { id: 10, email: "a@acme.com".into(), name: "Ana".into() },
        User { id: 11, email: "b@acme.com".into(), name: "Bia".into() },
    ])
    .execute(&client)
    .await?;
```

## Próximos passos

- [**`update::&lt;M&gt;()`**](/v0.0.1/orm/update): update tradicional com condição.
- [**`find_and_update::&lt;M&gt;()`**](/v0.0.1/orm/find-and-update): update atômico em um único registro.
