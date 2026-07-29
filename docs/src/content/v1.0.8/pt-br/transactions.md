# Transações

Uma transaction batch executa builders diferentes, na ordem em que foram adicionados, usando uma única conexão física. Se qualquer operação falhar, o adapter faz rollback de todas as operações anteriores. O commit acontece somente depois que a lista inteira termina com sucesso.

## Crie uma transaction

Use `Transaction::new()` porque um `Vec` comum não consegue guardar builders Rust de tipos diferentes:

```rust
use dinoco::{
    Transaction, find_first, insert_into, transactions,
};

let session = AccountSession::new(
    "session-1".to_string(),
    account_id.clone(),
);

let mut transaction = Transaction::new();

transaction.push(
    find_first::<Account>()
        .where_(|account| account.id.eq(&account_id))
);

transaction.push(
    find_first::<AccountSession>()
        .where_(|session| session.id.eq("session-1"))
);

transaction.push(
    insert_into::<AccountSession>().values(&session)
);

let results = transactions(transaction)
    .execute(&client)
    .await?;
```

`Transcation` também é exportado como alias de `Transaction` para compatibilidade com a grafia usada em exemplos antigos. Código novo deve preferir `Transaction`.

## Use o macro

O macro `transaction!` cria a mesma lista com uma sintaxe mais compacta:

```rust
let transaction = dinoco::transaction![
    find_first::<Account>()
        .where_(|account| account.id.eq(&account_id)),
    insert_into::<AccountSession>().values(&session),
];

let results = transactions(transaction)
    .execute(&client)
    .await?;
```

## Leia os resultados

Cada builder produz uma posição, na mesma ordem dos `push`. Leituras preservam seu retorno normal e writes sem `returning` produzem `()`:

```rust
let mut results = transactions(transaction)
    .execute(&client)
    .await?;

let account: Option<Account> = results.take(0)?;
results.take::<()>(1)?;
```

Use `get::<T>(índice)` para emprestar um resultado ou `take::<T>(índice)` para removê-lo. Um índice inválido, já removido ou lido com o tipo errado retorna erro.

## Atomicidade e conexão

- SQLite, PostgreSQL Direct, PgBouncer e MySQL executam a batch dentro de uma transaction nativa.
- Todas as operações usam o backend primary; réplicas de leitura nunca participam.
- Uma operação enxerga os writes concluídos pelas operações anteriores da mesma batch.
- Erro de SQL, constraint ou conversão de row causa rollback.
- Builders inválidos são rejeitados antes de abrir a transaction.

## Builders suportados

Transactions aceitam `find_first`, `find_many`, `count`, inserts planos, updates escalares e deletes. `returning` e `find_and_update` funcionam no SQLite e PostgreSQL.

Finds adicionados à batch preservam `where_complex`, incluindo grupos `and`, `or`, `or_many` e `not`.
Condições `fulltext` também são preservadas em `find_first` e `find_many`, usando a estratégia do adapter.

Esta versão ainda rejeita dentro da batch:

- `includes` em finds ou counts;
- inserts com payloads de relações aninhadas;
- `connect` e `disconnect`;
- writes com `returning` e `find_and_update` no MySQL.

Execute esses fluxos fora da batch ou divida-os em builders escalares explícitos até que o runtime transacional passe a suportá-los.
