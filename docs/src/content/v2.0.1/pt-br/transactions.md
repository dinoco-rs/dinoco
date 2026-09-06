# Transactions

`transaction` executa uma closure assíncrona contra uma transaction nativa do banco, presa a uma única conexão física com a primary durante toda sua duração. Toda mutation que você roda pelo handle `tx` que ela te dá enxerga todo write anterior daquela mesma closure — leituras dentro da transaction nunca ficam desatualizadas em relação a writes que já aconteceram nela. O Dinoco só faz commit quando a closure retorna `Ok`; qualquer `Err`, de qualquer origem, dispara um rollback.

## Crie uma transaction

Passe o contexto transacional para cada mutation com `.execute(tx)` em vez de `.execute(&client)`. O contexto é `Copy`, então pode ser passado adiante e reutilizado livremente durante toda a closure:

```rust
use dinoco::{find_and_update, insert_into, transaction};

let result = transaction(&client, |tx| async move {
    let business = find_and_update::<Business>()
        .where_(|business| business.id.eq(&business_id))
        .where_(|business| business.balance.gte(amount))
        .update(|business| business.balance.decrement(amount))
        .execute(tx)
        .await?;

    insert_into::<BusinessTransaction>()
        .value(&movement)
        .execute(tx)
        .await?;

    Ok(business)
})
.await;
```

A closure pode retornar qualquer coisa dentro de `Ok(value)` — o Dinoco não restringe o tipo de sucesso. O resultado externo é `Result<T, TransactionError>`.

## Erros tipados

`TransactionError` preserva tanto a categoria da operação que falhou quanto o erro original do driver por baixo dela. Falhas de update atômico continuam especificamente identificáveis por `AtomicUpdateError`:

```rust
use dinoco::{AtomicUpdateError, DatabaseConstraintError, TransactionError};

match result {
    Ok(business) => use_updated_business(business),
    Err(TransactionError::AtomicUpdate(AtomicUpdateError::RowNotAffected)) => {
        // O registro não existe ou deixou de satisfazer o WHERE.
    }
    Err(TransactionError::Create(dinoco::CreateError::Constraint {
        kind: DatabaseConstraintError::UniqueViolation,
        ..
    })) => {
        // Violação estruturada de constraint unique.
    }
    Err(TransactionError::Update(error)) => handle_update(error),
    Err(TransactionError::Delete(error)) => handle_delete(error),
    Err(error) => return Err(error.into()),
}
```

Categorias de falha de create, update, delete, decode e constraint portável são mantidas distintas em vez de achatadas num erro genérico único — então um `match` como o de cima consegue tratar os casos específicos que interessam e cair num caminho genérico para o resto. Quando você precisa de detalhe no nível do driver além do que o Dinoco classifica, `DatabaseError::original()` expõe diretamente o erro original de `rusqlite`, `tokio-postgres` ou `mysql_async`. A própria classificação de constraint é baseada em códigos de erro estruturados do driver, nunca em parsear uma string de mensagem de erro.

## Rollback automático

Qualquer erro que a closure retorne desfaz tudo que a transaction fez até ali — incluindo `AtomicUpdateError::RowNotAffected`, que costuma ser exatamente o sinal que você quer para disparar um rollback:

```rust
let result = transaction(&client, |tx| async move {
    insert_into::<AuditEntry>().value(&entry).execute(tx).await?;

    find_and_update::<Business>()
        .where_(|business| business.id.eq(&business_id))
        .where_(|business| business.balance.gte(amount))
        .update(|business| business.balance.decrement(amount))
        .execute(tx)
        .await?;

    Ok(())
})
.await;
```

Se o update de saldo aqui afetar zero linhas (digamos, porque `balance.gte(amount)` deixou de valer), o `?` propaga o `RowNotAffected` para fora da closure — e o registro de audit inserido logo antes sofre rollback junto, exatamente como se nenhuma das duas instruções tivesse rodado.

## Falhas de commit e rollback

Um erro vindo do próprio `COMMIT` volta como `TransactionError::Commit(DatabaseError)`. Nesse ponto específico, o Dinoco relata o resultado do driver honestamente, sem afirmar se o servidor de fato commitou — depois de uma falha de conexão bem no momento do commit, esse estado pode ser genuinamente ambíguo, e fingir o contrário seria pior do que expor a incerteza.

> [!WARNING]
> Se uma operação dentro da closure falhar **e** o `ROLLBACK` seguinte também falhar, o Dinoco retorna as duas informações em vez de descartar uma:
>
> ```rust
> TransactionError::RollbackFailed {
>     source,         // erro original da operação
>     rollback_error, // erro original do driver no rollback
> }
> ```
>
> A falha original nunca é substituída silenciosamente pela falha do rollback — você consegue ver o que de fato deu errado primeiro, e o que deu errado tentando limpar depois disso.

## Atomicidade e conexão

- SQLite, PostgreSQL Direct, PgBouncer e MySQL usam todos uma transaction nativa de verdade por baixo dos panos — isso não é uma emulação no nível da aplicação.
- Todo comando na closure compartilha exatamente a mesma conexão física com a primary; réplicas de leitura nunca participam de uma transaction.
- Cada comando executa assim que seu future é aguardado, na ordem em que você escreve — controle de fluxo Rust comum (um `if`, um loop, um `let` anterior) funciona exatamente como esperado, porque não existe batching ou execução adiada acontecendo por trás.
- Operações numéricas (`increment`/`decrement`/`multiply`/`divide`) e seus predicados `WHERE` ficam os dois dentro da mesma instrução `UPDATE` do banco — o Dinoco nunca introduz uma corrida de ler-depois-calcular-depois-escrever calculando o novo valor no Rust primeiro.
- O contexto transacional rejeita ser reutilizado fora da closure a que pertence, por construção.

## Builders suportados

A API por closure suporta `insert_into`, `insert_many`, `update`, `update_many`, `delete`, `delete_many`, `find_and_update`, e os helpers gerados para mutations aninhadas ou many-to-many. Writes com returning (`.returning::<S>()`) usam o suporte nativo de cada banco no SQLite e PostgreSQL; o `find_and_update` do MySQL usa um fallback dedicado — ele roda o `UPDATE` condicional primeiro, depois recarrega a linha por `id` ou pelas condições originais, e nunca faz uma checagem de existência separada antes de tentar o update.

> [!NOTE]
> Builders de leitura que não aceitam um executor de mutation (não existe um para aceitar, já que eles não alteram nada) ficam totalmente fora dessa API por closure — continue usando `&client` para eles, mesmo em código que está logicamente "dentro" de uma unidade de trabalho maior. O que pertence dentro da closure é especificamente o conjunto de writes que precisa receber commit ou rollback juntos, como uma unidade.
