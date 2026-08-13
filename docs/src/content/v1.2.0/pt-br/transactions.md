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

O future retornado por `.execute(&client)` é `Send`, portanto uma batch transacional pode ser aguardada diretamente dentro de um handler do Axum ou de outra task multithread do Tokio.

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

## Relações many-to-many

Writes many-to-many implícitos participam da mesma transaction do banco que o restante da batch. Endpoints existentes podem ser conectados ou desconectados pelo ID virtual gerado em `update`, `update_many` e `find_and_update`:

```rust
let batch = dinoco::transaction![
    dinoco::update::<Task>()
        .where_(|task| task.id.eq(&task_id))
        .update(|task| task.tag_id.connect(&new_tag_id)),
    dinoco::update::<Task>()
        .where_(|task| task.id.eq(&task_id))
        .update(|task| task.tag_id.disconnect(&old_tag_id)),
];

dinoco::transactions(batch).execute(&client).await?;
```

Cada update continua ocupando uma única posição lógica em `TransactionResults`, mesmo quando o Dinoco executa um statement adicional na pivô. Um update escalar e um write de relação também podem ficar na mesma closure `.update(...)`.

IDs virtuais funcionam igualmente em `insert_into` e `insert_many` transacionais. O Dinoco insere cada endpoint e cria seu vínculo na pivô antes de avançar para o próximo builder:

```rust
let mut tag = Tag::new("documentation".to_string());
tag.task_id = Some(task_id.clone());

let mut tags = vec![
    Tag::new("rust".to_string()),
    Tag::new("database".to_string()),
];
for tag in &mut tags {
    tag.task_id = Some(task_id.clone());
}

let batch = dinoco::transaction![
    dinoco::insert_into::<Tag>().values(&tag),
    dinoco::insert_many::<Tag>().values(&tags),
];

dinoco::transactions(batch).execute(&client).await?;
```

`None` insere o endpoint sem criar vínculo. Se o insert do endpoint ou o write na pivô falhar, todos os writes escalares e relacionais da batch sofrem rollback.

O ID do endpoint inserido precisa ser conhecido antes de a transaction começar, como acontece com UUID, Snowflake ou um ID fornecido pelo caller. Uma chave virtual preenchida não pode ser usada em um insert transacional quando a primary key do endpoint usa `autoincrement()`; insira esse endpoint primeiro e conecte-o em um update posterior. Inserts comuns fora de uma transaction continuam suportando esse caso, pois conseguem ler o ID gerado antes de criar a row da pivô.

## Builders suportados

Transactions aceitam `find_first`, `find_many`, `count`, inserts, updates, deletes, `connect`/`disconnect` many-to-many implícitos e IDs virtuais preenchidos em `insert_into` e `insert_many`. `returning` e `find_and_update` funcionam no SQLite e PostgreSQL.

Finds adicionados à batch preservam `where_complex`, incluindo grupos `and`, `or`, `or_many` e `not`.
Condições `fulltext` também são preservadas em `find_first` e `find_many`, usando a estratégia do adapter.

Esta versão ainda rejeita dentro da batch:

- `includes` em finds ou counts;
- inserts com payloads one-to-one, one-to-many ou many-to-one aninhados;
- IDs virtuais many-to-many preenchidos em endpoints cuja primary key usa `autoincrement()`;
- writes com `returning` e `find_and_update` no MySQL.

Essas limitações não afetam writes many-to-many implícitos com IDs UUID, Snowflake ou fornecidos pelo caller.
