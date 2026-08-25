# Find and update

`find_and_update` aplica um `UPDATE` condicional e retorna a entity. As operações numéricas são avaliadas pelo banco, portanto uma condição como `balance >= amount` e o decremento correspondente fazem parte da mesma mutation atômica.

Como altera dados, o builder sempre usa o backend primary, mesmo quando existem réplicas de leitura configuradas.

## 1. Defina o filtro

```rust
let business = dinoco::find_and_update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .where_(|business| business.balance.gte(amount))
    .update(|business| business.balance.decrement(amount))
    .execute(&client)
    .await?;
```

As condições são compiladas no próprio `UPDATE`. O Dinoco não executa um `SELECT` antes para verificar existência ou calcular um valor numérico. Prefira uma primary key ou outra condição única, pois todas as rows compatíveis podem ser atualizadas.

## 2. Defina as alterações

Cada `.update(...)` representa a mudança de um field. As chamadas são acumuladas e compiladas em um único statement:

```rust
let business = dinoco::find_and_update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .update(|business| business.balance.decrement(amount))
    .update(|business| business.total_withdrawn.increment(amount))
    .update(|business| business.transaction_count.increment(1))
    .execute(&client)
    .await?;
```

Conceitualmente, isso produz:

```sql
UPDATE business
SET balance = balance - ?,
    total_withdrawn = total_withdrawn + ?,
    transaction_count = transaction_count + ?
WHERE id = ?
RETURNING ...
```

Os valores continuam como bind parameters. As operações existentes `.set(value)`, `connect` e `disconnect` continuam funcionando. Alterar o mesmo field mais de uma vez é rejeitado com `AtomicUpdateError::DuplicateField`, evitando semânticas de ordem diferentes entre bancos.

## Operações numéricas

Os fields de update gerados para `Integer`, `Float`, `Integer?` e `Float?` oferecem:

```rust
.increment(value)
.decrement(value)
.multiply(value)
.divide(value)
```

Eles são compilados respectivamente como `field = field + ?`, `field = field - ?`, `field = field * ?` e `field = field / ?`. Colunas numéricas opcionais preservam a semântica SQL normal de `NULL`: aritmética sobre `NULL` continua `NULL`; o Dinoco não adiciona `COALESCE` implícito.

Divisão por zero, overflow, arredondamento e limites numéricos continuam sob responsabilidade do banco selecionado e são propagados pela hierarquia tipada de erros. O Dinoco não faz leitura prévia nem calcula os valores em Rust.

## 3. Leia o retorno

O retorno é `Result<Model, AtomicUpdateError>`, e não `Option<Model>`. Não é necessário chamar `.returning()`:

```rust
use dinoco::AtomicUpdateError;

match result {
    Ok(business) => use_updated_business(business),
    Err(AtomicUpdateError::RowNotAffected) => {
        // Empresa ausente ou condição concorrente de saldo não satisfeita.
    }
    Err(AtomicUpdateError::Constraint { kind, source }) => {
        handle_constraint(kind, source);
    }
    Err(error) => return Err(error.into()),
}
```

`RowNotAffected` é determinado pelo resultado da mutation, sem um `SELECT` de existência. As outras variantes diferenciam update vazio, field duplicado, decode da row, constraint estruturada e outras falhas de banco. `DatabaseError` mantém o erro original do driver.

## Comportamento por adapter

SQLite, PostgreSQL Direct e PgBouncer usam um único `UPDATE ... RETURNING`. O MySQL abre uma transaction nativa, executa primeiro o `UPDATE` condicional, verifica a quantidade de rows afetadas e só então recarrega a row. A recarga usa `id` quando existe um predicado de igualdade; caso contrário, reutiliza as condições originais. Nunca existe um `SELECT` antes do update.

A recarga de compatibilidade do MySQL é um segundo statement na mesma conexão física transacional. Se um filtro sem `id` deixar de corresponder depois da alteração, o Dinoco desfaz a mutation em vez de informar `RowNotAffected` depois de um update bem-sucedido. Prefira uma condição de igualdade em `id` ao alterar um field que também participa do filtro.

## Use where complex

`find_and_update` aceita os mesmos grupos booleanos dos finds:

```rust
let account = dinoco::find_and_update::<Account>()
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

Quando `where_complex` é usado, todos os `where_` do builder são ignorados. Veja [Where complex](/v1.2.3/orm/where-complex).

## Use em uma transaction

```rust
let business = dinoco::transaction(&client, |tx| async move {
    let business = dinoco::find_and_update::<Business>()
        .where_(|business| business.id.eq(&business_id))
        .where_(|business| business.balance.gte(amount))
        .update(|business| business.balance.decrement(amount))
        .execute(tx)
        .await?;

    dinoco::insert_into::<BusinessTransaction>()
        .value(&movement)
        .execute(tx)
        .await?;

    Ok(business)
})
.await?;
```

`RowNotAffected` é promovido para `TransactionError::AtomicUpdate` e causa rollback automático dos writes anteriores.

## Limitações

- Não possui `select`, `includes`, `order_by`, `take` ou `skip`.
- Exige ao menos uma chamada `.update(...)`.
- A mesma coluna não pode ser alterada duas vezes no mesmo builder.
- Uma row ausente ou que deixou de satisfazer as condições gera `AtomicUpdateError::RowNotAffected`.
- O MySQL faz uma recarga depois do update, preferindo um predicado de igualdade em `id` e reutilizando as condições originais quando ele não existe.
