# Find and update

`find_and_update` aplica um `UPDATE` condicional e devolve a entity resultante diretamente — sem uma leitura separada depois. Suas operações numéricas são avaliadas *pelo banco*, o que é o que permite uma condição como `balance >= amount` e o decremento correspondente acontecerem como uma única mutation atômica, imune à corrida que uma abordagem de ler-depois-escrever-no-Rust teria entre duas requisições concorrentes.

Como altera dados, esse builder sempre roda contra o backend primary, mesmo quando réplicas de leitura estão configuradas — não existe decisão de roteamento para réplica aqui.

## 1. Defina o filtro

```rust
let business = dinoco::find_and_update::<Business>()
    .where_(|business| business.id.eq(&business_id))
    .where_(|business| business.balance.gte(amount))
    .update(|business| business.balance.decrement(amount))
    .execute(&client)
    .await?;
```

As condições compilam direto no statement `UPDATE` — o Dinoco nunca roda um `SELECT` preliminar para checar existência ou calcular um valor antes. Prefira filtrar por uma primary key ou outra condição genuinamente única, já que *toda* row que o filtro bater é atualizada, não só "a primeira".

## 2. Defina as alterações

Cada chamada `.update(...)` representa a mudança de um field; elas se acumulam e compilam num único statement:

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

Todo valor continua como bind parameter do início ao fim. As operações comuns `.set(value)`, `connect` e `disconnect` continuam funcionando aqui exatamente como em `update`/`update_many`.

> [!WARNING]
> Alterar o *mesmo* field duas vezes numa chamada de `find_and_update` é rejeitado com `AtomicUpdateError::DuplicateField`, em vez de escolher um silenciosamente — bancos diferentes ordenam múltiplas atribuições à mesma coluna de formas diferentes, e o Dinoco prefere falhar de forma visível a deixar essa ambiguidade produzir um resultado específico de cada banco.

## Operações numéricas

Fields de update gerados para `Integer`, `Float`, `Integer?` e `Float?` expõem quatro operações avaliadas pelo banco:

```rust
.increment(value)
.decrement(value)
.multiply(value)
.divide(value)
```

Eles compilam para `field = field + ?`, `field = field - ?`, `field = field * ?` e `field = field / ?`, respectivamente. Colunas numéricas opcionais mantêm a semântica SQL normal de `NULL` do início ao fim — aritmética sobre `NULL` continua `NULL`; o Dinoco nunca insere um `COALESCE` implícito para disfarçar isso.

Divisão por zero, overflow, arredondamento e comportamento de limites numéricos são todos de responsabilidade inteira do banco contra o qual você está rodando, e chegam até você pela hierarquia tipada de erros abaixo — o Dinoco nunca faz uma leitura prévia nem calcula a aritmética ele mesmo em Rust.

## 3. Leia o retorno

O tipo de retorno é `Result<Model, AtomicUpdateError>` — repare que é `Result`, não `Option<Model>` embrulhado num `Result`. Não existe (nem está disponível) uma chamada `.returning()`; a entity atualizada volta como o próprio valor de sucesso diretamente:

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

`RowNotAffected` é determinado puramente pela contagem de rows afetadas da própria mutation — nunca por um `SELECT` de existência separado. As outras variantes diferenciam um update vazio, um field duplicado, uma falha de decode da row, uma violação de constraint estruturada do banco, e outras falhas no nível do banco; `DatabaseError` continua mantendo o erro original do driver acessível por baixo.

## Comportamento por adapter

SQLite, PostgreSQL Direct e PgBouncer fazem tudo isso num único statement `UPDATE ... RETURNING`. O MySQL, que não tem equivalente, abre uma transaction nativa, roda o `UPDATE` condicional primeiro, verifica sua contagem de rows afetadas, e só então recarrega a row — por `id` quando existe um predicado de igualdade nele, ou reutilizando as condições originais do filtro caso contrário. Nunca existe um `SELECT` antes do update em nenhum adapter, MySQL incluso.

> [!NOTE]
> A recarga de compatibilidade do MySQL é um segundo statement na mesma conexão física da transaction, não um round-trip separado fora dela. Se um filtro que *não* seja `id` deixar de corresponder no momento em que a recarga roda (uma corrida genuinamente rara, mas real), o Dinoco desfaz a mutation inteira em vez de reportar um `RowNotAffected` enganoso depois do update em si ter de fato sucedido. Esse é um bom motivo para preferir uma condição de igualdade em `id` especificamente quando o field que você está filtrando também é um que você está atualizando.

## Use where complex

`find_and_update` aceita o mesmo agrupamento booleano que os finds:

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

A mesma regra de sobrescrita se aplica aqui também: uma vez que `where_complex` está presente, todo `where_` comum no builder é ignorado. Veja [Where complex](/pt-br/docs/orm/orm/where-complex).

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

Dentro de uma transaction, `RowNotAffected` é promovido para `TransactionError::AtomicUpdate` e desfaz automaticamente tudo que a closure fez antes dele — incluindo o insert estilo audit que rodou logo antes aqui.

## Limitações

- Sem `select`, `includes`, `order_by`, `take` ou `skip` — esse builder é deliberadamente restrito, feito sob medida para uma única atualização condicional atômica.
- Pelo menos uma chamada `.update(...)` é obrigatória.
- A mesma coluna não pode ser alterada duas vezes numa mesma chamada do builder.
- Uma row ausente, ou uma que deixou de bater no momento em que o update roda, é `AtomicUpdateError::RowNotAffected` — não um panic, não um no-op silencioso.
- O MySQL especificamente faz uma recarga pós-update, preferindo um predicado de igualdade em `id` e caindo para as condições originais do filtro caso contrário.
