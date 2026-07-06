# transactions

Usado para executar uma lista de operações dentro de uma única transação.

---

## O que você pode fazer

- Montar ações com `.tx()` (ou `tx(...)`) a partir dos métodos do ORM.
- Agrupar ações em `Vec<TransactionAction<A>>`.
- Executar com `transactions(actions).execute(&client).await`.
- Garantir rollback automático se alguma ação falhar.

## Exemplo básico

```rust
use dinoco::{transactions, TransactionActionExt, find_first, insert_into, update};
use dinoco_engine::SqliteAdapter;

let mut actions = Vec::<dinoco::TransactionAction<SqliteAdapter>>::new();

actions.push(
    find_first::<User>()
        .cond(|w| w.id.eq(1_i64))
        .tx(),
);

actions.push(
    insert_into::<User>()
        .values(User {
            id: 1,
            email: "alice@dinoco.dev".to_string(),
            name: "Alice".to_string(),
        })
        .tx(),
);

actions.push(
    update::<User>()
        .cond(|w| w.id.eq(1_i64))
        .values(User {
            id: 1,
            email: "alice@dinoco.dev".to_string(),
            name: "Alice Updated".to_string(),
        })
        .tx(),
);

transactions(actions).execute(&client).await?;
```

## Exemplo com referências

```rust
use dinoco::{transactions, tx, find_first, insert_many, update};
use dinoco_engine::SqliteAdapter;

let first = User { id: 1, email: "alice@dinoco.dev".into(), name: "Alice".into() };
let second = User { id: 2, email: "bruno@dinoco.dev".into(), name: "Bruno".into() };
let updated = User { id: 1, email: "alice@dinoco.dev".into(), name: "Alice Updated".into() };

let mut actions = Vec::<dinoco::TransactionAction<SqliteAdapter>>::new();

actions.push(tx(find_first::<User>().cond(|w| w.id.eq(1_i64))));
actions.push(tx(insert_many::<User>().values(vec![&first, &second])));
actions.push(tx(update::<User>().cond(|w| w.id.eq(1_i64)).values(&updated)));

transactions(actions).execute(&client).await?;
```

## Comportamento de rollback

Se qualquer passo retornar erro, a transação é revertida e nenhuma alteração parcial é persistida.

## Compatibilidade

`transactions(...).execute(...)` funciona com:

- `SqliteAdapter`
- `MySqlAdapter`
- `PostgresAdapter`

## Observações

- O retorno atual de `transactions(...).execute(...)` é `DinocoResult<()>`.
- As execuções seguem o mesmo padrão async com `Send + Sync` usado nos outros métodos.

## Próximos passos

- [**`insert_into::<M>()`**](/v0.1.1/orm/insert-into): inserção única.
- [**`insert_many::<M>()`**](/v0.1.1/orm/insert-many): inserção em lote.
- [**`update::<M>()`**](/v0.1.1/orm/update): atualização com condição.
