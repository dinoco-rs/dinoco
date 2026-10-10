# Transactions

`transaction` executa uma closure assíncrona contra uma transaction nativa do banco, presa a uma única conexão física com a primary durante toda sua duração. Toda operação que você roda pelo handle `tx` que ela te dá — writes, e leituras com `find_first`/`find_many` — enxerga todo write anterior daquela mesma closure — leituras dentro da transaction nunca ficam desatualizadas em relação a writes que já aconteceram nela. O Dinoco só faz commit quando a closure retorna `Ok`; qualquer `Err`, de qualquer origem, dispara um rollback. Para fazer rollback com um tipo de erro seu, use [`transaction_with_error`](#erros-customizados).

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

A closure retorna `anyhow::Result<T>` e pode retornar qualquer coisa dentro de `Ok(value)` — o Dinoco não restringe o tipo de sucesso, e nem a closure nem o binding precisam de anotação de tipo. O `?` funciona em todo builder, e `anyhow::bail!(...)` aborta. O resultado externo é `Result<T, TransactionError>`.

## Erros tipados

`TransactionError` preserva tanto a categoria da operação que falhou quanto o erro original do driver por baixo dela. Falhas de update atômico continuam especificamente identificáveis por `AtomicUpdateError`:

```rust
use dinoco::{AtomicUpdateError, CreateError, TransactionError};

match result {
    Ok(business) => use_updated_business(business),
    Err(TransactionError::AtomicUpdate(AtomicUpdateError::RowNotAffected)) => {
        // O registro não existe ou deixou de satisfazer o WHERE.
    }
    Err(TransactionError::Create(CreateError::UniqueViolation { columns, .. })) => {
        // Violação estruturada de constraint unique, ex.: columns == ["email"].
    }
    Err(TransactionError::Update(error)) => handle_update(error),
    Err(TransactionError::Delete(error)) => handle_delete(error),
    Err(error) => return Err(error.into()),
}
```

Inserts reportam cada tipo de constraint como sua própria variante de `CreateError` (`UniqueViolation`, `ForeignKeyViolation`, `NotNullViolation`, `CheckViolation`), com a tabela, a constraint e as colunas que o driver citou — veja [Trate erros de insert](/pt-br/docs/orm/orm/insert#trate-erros-de-insert). Categorias de falha de create, update, delete, decode e constraint portável são mantidas distintas em vez de achatadas num erro genérico único — então um `match` como o de cima consegue tratar os casos específicos que interessam e cair num caminho genérico para o resto. Quando você precisa de detalhe no nível do driver além do que o Dinoco classifica, `DatabaseError::original()` expõe diretamente o erro original de `rusqlite`, `tokio-postgres` ou `mysql_async`. A própria classificação de constraint é baseada em códigos de erro estruturados do driver, nunca em parsear uma string de mensagem de erro.

## Erros customizados

Para abortar uma transaction por uma regra sua — e não por uma falha do banco — use `transaction_with_error`. Ele roda exatamente como o `transaction`, mas a closure retorna `Result<T, TransactionError<E>>`, e retornar `Err(TransactionError::Custom(error))` desfaz tudo que a closure já fez e devolve `error` para quem chamou, tipado:

```rust
use dinoco::{TransactionError, find_and_update, transaction_with_error};

#[derive(Debug, thiserror::Error)]
enum RuntimeError {
    #[error("este business não pode sacar")]
    WithoutPermission,
}

let result = transaction_with_error::<RuntimeError, _>(&client, |tx| async move {
    let business = find_and_update::<Business>()
        .where_(|business| business.id.eq(&business_id))
        .where_(|business| business.balance.gte(amount))
        .update(|business| business.balance.decrement(amount))
        .execute(tx)
        .await?;

    if !business.is_admin {
        return Err(TransactionError::Custom(RuntimeError::WithoutPermission));
    }

    Ok(business)
})
.await;
```

`::<RuntimeError, _>` nomeia o tipo de erro na chamada: `RuntimeError` é o `E`, e o `_` deixa o tipo de sucesso ser inferido a partir do `Ok(business)`. O Rust aceita todos os argumentos genéricos de uma função ou nenhum, então o `_` é obrigatório. O turbofish pode ser omitido quando algo já nomeia o `E`, como o `TransactionError::Custom(RuntimeError::WithoutPermission)` na closure ou uma função que retorna `Result<_, TransactionError<RuntimeError>>`.

O saldo já tinha sido decrementado quando a checagem falha, e o erro customizado desfaz esse `UPDATE` como qualquer outro erro faria. O resultado é `Result<Business, TransactionError<RuntimeError>>`, então o caso customizado é tratado no mesmo `match` que os do banco:

```rust
use dinoco::{AtomicUpdateError, TransactionError};

match result {
    Ok(business) => use_updated_business(business),
    Err(TransactionError::Custom(RuntimeError::WithoutPermission)) => {
        // Rollback feito: esse business não pode sacar.
    }
    Err(TransactionError::AtomicUpdate(AtomicUpdateError::RowNotAffected)) => {
        // Rollback feito: saldo insuficiente.
    }
    Err(error) => return Err(error.into()),
}
```

- Erros customizados existem só no `transaction_with_error`. O `transaction(...)` mantém a closure `anyhow::Result` e retorna `TransactionError`, cujo `E` é `Infallible`.
- Dentro da closure, o `?` converte erros de insert, update, delete e update atômico para a sua própria variante, e qualquer `anyhow::Error` (leituras incluídas) para `TransactionError::Operation`. O `anyhow::bail!` não encaixa no tipo de retorno dessa closure; para uma mensagem pontual sem um tipo dedicado, `return Err(anyhow::anyhow!("...").into())` aborta com `TransactionError::Operation`.
- Um helper que retorna `Result<_, RuntimeError>` se compõe com `?` via `.map_err(TransactionError::Custom)?`.
- `E` pode ser qualquer tipo: um enum, uma struct, uma `String`, um erro de outra crate. O `transaction_with_error(...)` não impõe nenhum bound sobre ele. `TransactionError<E>` implementa `Display` quando `E` implementa (a variante customizada é exibida como o próprio `E`) e `std::error::Error` quando `E` também é `Debug`, então o `?` para `anyhow::Error` continua funcionando e `downcast_ref::<TransactionError<RuntimeError>>()` o recupera.

## Leia dentro de uma transaction

`find_first` e `find_many` aceitam o mesmo handle `tx`. A leitura roda na conexão da transaction, então enxerga todo write ainda não commitado feito antes na closure — incluindo relações carregadas com `.includes(...)`:

```rust
use dinoco::{find_first, find_many, insert_into, transaction};

let result = transaction(&client, |tx| async move {
    insert_into::<Business>().value(&business).execute(tx).await?;
    insert_into::<Office>().value(&office).execute(tx).await?;

    let stored = find_first::<Business>()
        .where_(|x| x.id.eq(&business_id))
        .includes(|x| x.offices())
        .execute(tx)
        .await?
        .expect("inserido acima");

    let office_ids = find_many::<Office>()
        .where_(|x| x.business_id.eq(&business_id))
        .pluck(|office| office.id)
        .execute(tx)
        .await?;

    Ok((stored, office_ids))
})
.await;
```

Se a closure falhar depois, essas leituras só viram dados que nunca chegam a ser commitados — nada fora da transaction os observa. Leituras pelo `tx` sempre vão para a conexão primary da transaction, nunca para uma réplica, então `read_in_primary()` não faz diferença ali. `count` e `exists` ainda aceitam apenas `&client`.

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

Se o update de saldo aqui afetar zero linhas (digamos, porque `balance.gte(amount)` deixou de valer), o `?` propaga o `RowNotAffected` para fora da closure — e o registro de audit inserido logo antes sofre rollback junto, exatamente como se nenhuma das duas instruções tivesse rodado. Um [erro customizado](#erros-customizados) retornado pelo `transaction_with_error` faz rollback do mesmo jeito.

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

A API por closure suporta `find_first`, `find_many`, `insert_into`, `insert_many`, `update`, `update_many`, `delete`, `delete_many`, `find_and_update`, e os helpers gerados para mutations aninhadas ou many-to-many. Writes com returning (`.returning::<S>()`) usam o suporte nativo de cada banco no SQLite e PostgreSQL; o `find_and_update` do MySQL usa um fallback dedicado — ele roda o `UPDATE` condicional primeiro, depois recarrega a linha por `id` ou pelas condições originais, e nunca faz uma checagem de existência separada antes de tentar o update.

> [!NOTE]
> `count` e `exists` ainda não aceitam o contexto de transaction — continue usando `&client` para eles. Eles leem apenas dados commitados, então não enxergam writes que a closure ainda não commitou.
