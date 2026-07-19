# update_many

Usado para atualizar vários registros de uma vez.

---

## O que você pode fazer

- `.cond(...)`: restringe quais registros podem ser atualizados.
- `.values(Vec<M>)` ou `.values(Vec<&M>)`: define os itens usados na atualização em lote.
- `.returning::<T>()`: retorna os registros atualizados como uma lista tipada.
- `.execute(&client)`: executa o update em lote.

## Retorno

Sem `.returning::<T>()`, o retorno é:

```rust
DinocoResult<()>
```

Com `.returning::<T>()`, o retorno passa a ser:

```rust
DinocoResult<Vec<T>>
```

## Exemplo básico

```rust
let payloads = vec![
    User { id: 1, email: "a@acme.com".into(), name: "Ana".into() },
    User { id: 2, email: "b@acme.com".into(), name: "Bia".into() },
];

dinoco::update_many::<User>()
    .values(payloads)
    .execute(&client)
    .await?;
```

## Exemplo com referências

```rust
let user_a = User { id: 10, email: "a@acme.com".into(), name: "Ana".into() };
let user_b = User { id: 11, email: "b@acme.com".into(), name: "Bia".into() };

dinoco::update_many::<User>()
    .values(vec![&user_a, &user_b])
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

## Exemplo com worker

```rust
use database::*;

let _worker = workers()
    .on::<Vec<User>, _, _>("user.batch-updated", |job| async move {
        println!("Usuários atualizados: {}", job.data.len());
        job.success();
    })
    .run()
    .await?;

dinoco::update_many::<User>()
    .values(vec![
        User { id: 10, email: "a@acme.com".into(), name: "Ana".into() },
        User { id: 11, email: "b@acme.com".into(), name: "Bia".into() },
    ])
    .enqueue("user.batch-updated")
    .execute(&client)
    .await?;
```

Veja mais sobre workers em [**`queues`**](/v0.0.5/orm/queues).

## Próximos passos

- [**`update::<M>()`**](/v0.0.5/orm/update): update tradicional com condição.
- [**`find_and_update::<M>()`**](/v0.0.5/orm/find-and-update): update atômico em um único registro.
