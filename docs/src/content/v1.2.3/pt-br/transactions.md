# Transações

`transaction` executa uma closure assíncrona em uma transaction nativa e em uma única conexão física com o primary. Toda mutation executada com `tx` enxerga os writes anteriores da closure. O Dinoco faz commit somente quando a closure retorna `Ok`; qualquer `Err` causa rollback.

## Crie uma transaction

Passe o contexto transacional a cada mutation com `.execute(tx)`. O contexto é copiável e pode ser reutilizado durante toda a closure:

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

A closure pode retornar qualquer valor em `Ok(value)`. O resultado externo é `Result<T, TransactionError>`.

## Erros tipados

`TransactionError` preserva a categoria da operação e o erro original do driver. Updates atômicos continuam identificáveis por `AtomicUpdateError`:

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

Create, update, delete, decode e as categorias portáveis de constraint permanecem separados. `DatabaseError::original()` expõe o erro original de `rusqlite`, `tokio-postgres` ou `mysql_async` quando detalhes do driver forem necessários. A classificação de constraints usa códigos dos drivers, sem interpretar mensagens.

## Rollback automático

Todo erro retornado pela closure causa rollback, inclusive `AtomicUpdateError::RowNotAffected` e erros da aplicação:

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

Se o update não afetar nenhuma row, o insert anterior do audit também sofre rollback.

## Falhas de commit e rollback

Um erro retornado por `COMMIT` é `TransactionError::Commit(DatabaseError)`. Nesse momento o Dinoco informa o resultado do driver sem afirmar se o servidor fez commit, pois esse estado pode ser ambíguo depois de uma falha de conexão.

Se uma operação falhar e o `ROLLBACK` também falhar, o Dinoco retorna:

```rust
TransactionError::RollbackFailed {
    source,         // erro original da operação
    rollback_error, // erro original do driver no rollback
}
```

A falha original nunca é substituída pela falha do rollback.

## Atomicidade e conexão

- SQLite, PostgreSQL Direct, PgBouncer e MySQL usam uma transaction nativa.
- Todos os comandos usam a mesma conexão física com o primary; réplicas de leitura não participam.
- Cada comando executa imediatamente quando seu future é aguardado, permitindo usar controle de fluxo Rust com um resultado anterior.
- Operações numéricas e seus predicados `WHERE` permanecem no `UPDATE` do banco; não existe cálculo com leitura antes do write.
- Usar `tx` fora da closure é rejeitado pelo contexto transacional.

## Builders suportados

A API por closure suporta `insert_into`, `insert_many`, `update`, `update_many`, `delete`, `delete_many`, `find_and_update` e os helpers gerados para mutations aninhadas ou many-to-many. Writes com returning usam o suporte nativo do SQLite e PostgreSQL. No MySQL, `find_and_update` possui um fallback dedicado que executa primeiro o `UPDATE` condicional e recarrega depois por `id` ou pelas condições originais; ele nunca faz uma leitura de existência antes do update.

Builders de leitura que não aceitam um executor de mutation devem continuar usando o client fora desta API por closure. Mantenha dentro da closure todos os writes que precisam receber commit ou rollback juntos.
