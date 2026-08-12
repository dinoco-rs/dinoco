# Find and update

`find_and_update` combina filtro, update e retorno da entity em um único builder. Use-o quando o código precisa do valor atualizado sem montar uma chamada `.returning()` separada.

Como altera dados, `find_and_update` sempre executa no backend primary, mesmo quando existem `read_replicas` configurados.

## 1. Defina o filtro

```rust
let account = dinoco::find_and_update::<Account>()
    .where_(|account| account.id.eq(&account_id))
    .update(|account| account.name.set("Matheus".to_string()))
    .execute(&client)
    .await?;
```

O filtro deve identificar exatamente a row pretendida. Se várias rows forem compatíveis, todas podem ser atualizadas e o builder retorna uma delas; use primary key ou outra constraint única.

## 2. Defina as alterações

Cada `.update(...)` adiciona um field:

```rust
let account = dinoco::find_and_update::<Account>()
    .where_(|account| account.id.eq(&account_id))
    .update(|account| account.name.set("Matheus".to_string()))
    .update(|account| account.active.set(true))
    .execute(&client)
    .await?;
```

É obrigatório informar pelo menos um update. IDs virtuais de relações many-to-many implícitas aceitam `connect` e `disconnect` neste builder:

```rust
let account = dinoco::find_and_update::<Account>()
    .where_(|account| account.id.eq(&account_id))
    .update(|account| account.system_id.connect(system.id))
    .execute(&client)
    .await?;
```

O ID virtual é um comando write-only da relação e continua `None` nas entities carregadas. Consulte [Many-to-many implícito](/v1.1.10/guide/relations#many-to-many-implícito) para os dois sentidos da relação, disconnect, inserts e comportamento da pivô.

## 3. Leia o retorno

O retorno é `anyhow::Result<Account>`, não `Option<Account>`. Não é necessário chamar `.returning()`.

Se nenhuma row corresponder ao filtro, a execução retorna erro:

```text
Record from table 'account' could not be found for update.
```

Falhas de SQL, constraint ou conversão também são propagadas.

## Comportamento por adapter

SQLite, PostgreSQL Direct e PgBouncer usam `UPDATE ... RETURNING` em um statement. O MySQL, que não oferece esse retorno da mesma forma, busca os IDs, executa o update e lê o resultado em etapas separadas.

Por isso, no MySQL o builder não oferece a mesma atomicidade de statement. Mantenha o filtro único e evite depender de concorrência entre essas etapas.

## Use where complex

`find_and_update` aceita os mesmos grupos booleanos dos finds:

```rust
let account = dinoco::find_and_update::<Account>()
    .where_(|account| account.id.eq("ignorado"))
    .where_complex(|account, m| {
        m.and([
            account.email.eq("matheus@example.com"),
            m.not(account.locked.eq(true)),
        ])
    })
    .update(|account| account.active.set(true))
    .execute(&client)
    .await?;
```

Quando `where_complex` é usado, todos os `where_` do builder são ignorados, antes ou depois dele. Veja [Where complex](/v1.1.10/orm/where-complex).

## Use full-text

Um field `@fulltext` pode selecionar a row:

```rust
let article = dinoco::find_and_update::<Article>()
    .where_(|article| article.body.fulltext("dinoco"))
    .update(|article| article.reviewed.set(true))
    .execute(&client)
    .await?;
```

O method só existe nos fields String marcados no `schema.dinoco`.

## Use em uma transaction

```rust
let batch = dinoco::transaction![
    dinoco::find_and_update::<Account>()
        .where_(|account| account.id.eq(&account_id))
        .update(|account| account.active.set(true)),
];

let mut results = dinoco::transactions(batch)
    .execute(&client)
    .await?;

let account = results.take::<Account>(0)?;
```

O suporte transacional existe no SQLite, PostgreSQL Direct e PgBouncer. Isso inclui `connect` e `disconnect` many-to-many implícitos: alterações na pivô e alterações escalares recebem commit ou rollback juntas. O MySQL ainda rejeita `find_and_update` com writes escalares dentro de uma batch porque writes com retorno ainda não fazem parte desse executor.

## Limitações

- Não possui `select`, `includes`, `order_by`, `take` ou `skip`.
- Exige ao menos um `.update(...)`.
- A ausência da row é erro.
- No MySQL, o retorno é emulado em mais de um statement.
- Dentro de transactions, não está disponível no MySQL v1.1.10.
