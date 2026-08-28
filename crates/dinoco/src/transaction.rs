use dinoco_engine::{
    DeleteQuery, DinocoClient, DinocoRowModel, FindQuery, InsertQuery, TransactionCommand, TransactionExecutor,
    UpdateQuery,
};
use std::sync::Arc;

use crate::TransactionError;

tokio::task_local! {
    static ACTIVE_TRANSACTION: TransactionExecutor;
}

/// Copyable capability passed to a transaction closure. It is valid only
/// while that closure is running.
#[derive(Debug, Clone, Copy)]
pub struct TransactionContext;

#[async_trait::async_trait]
pub trait MutationExecutor: Sync {
    async fn query<M>(&self, query: FindQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel;
    async fn insert(&self, query: InsertQuery) -> anyhow::Result<usize>;
    async fn insert_returning<M>(&self, query: InsertQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel;
    async fn update(&self, query: UpdateQuery) -> anyhow::Result<usize>;
    async fn update_returning<M>(&self, query: UpdateQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel;
    async fn atomic_update_returning<M>(&self, query: UpdateQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel;
    async fn delete(&self, query: DeleteQuery) -> anyhow::Result<usize>;
    async fn delete_returning<M>(&self, query: DeleteQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel;
}

#[async_trait::async_trait]
impl MutationExecutor for DinocoClient {
    async fn query<M>(&self, query: FindQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        self.backend.query(query).await
    }

    async fn insert(&self, query: InsertQuery) -> anyhow::Result<usize> {
        self.backend.insert(query).await
    }

    async fn insert_returning<M>(&self, query: InsertQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        self.backend.insert_returning(query).await
    }

    async fn update(&self, query: UpdateQuery) -> anyhow::Result<usize> {
        self.backend.update(query).await
    }

    async fn update_returning<M>(&self, query: UpdateQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        self.backend.update_returning(query).await
    }

    async fn atomic_update_returning<M>(&self, query: UpdateQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        self.backend.atomic_update_returning(query).await
    }

    async fn delete(&self, query: DeleteQuery) -> anyhow::Result<usize> {
        self.backend.delete(query).await
    }

    async fn delete_returning<M>(&self, query: DeleteQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        self.backend.delete_returning(query).await
    }
}

#[async_trait::async_trait]
impl<T> MutationExecutor for &T
where
    T: MutationExecutor + Send + Sync,
{
    async fn query<M>(&self, query: FindQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        (**self).query(query).await
    }

    async fn insert(&self, query: InsertQuery) -> anyhow::Result<usize> {
        (**self).insert(query).await
    }

    async fn insert_returning<M>(&self, query: InsertQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        (**self).insert_returning(query).await
    }

    async fn update(&self, query: UpdateQuery) -> anyhow::Result<usize> {
        (**self).update(query).await
    }

    async fn update_returning<M>(&self, query: UpdateQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        (**self).update_returning(query).await
    }

    async fn atomic_update_returning<M>(&self, query: UpdateQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        (**self).atomic_update_returning(query).await
    }

    async fn delete(&self, query: DeleteQuery) -> anyhow::Result<usize> {
        (**self).delete(query).await
    }

    async fn delete_returning<M>(&self, query: DeleteQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        (**self).delete_returning(query).await
    }
}

#[async_trait::async_trait]
impl<T> MutationExecutor for Arc<T>
where
    T: MutationExecutor + Send + Sync,
{
    async fn query<M>(&self, query: FindQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        (**self).query(query).await
    }

    async fn insert(&self, query: InsertQuery) -> anyhow::Result<usize> {
        (**self).insert(query).await
    }

    async fn insert_returning<M>(&self, query: InsertQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        (**self).insert_returning(query).await
    }

    async fn update(&self, query: UpdateQuery) -> anyhow::Result<usize> {
        (**self).update(query).await
    }

    async fn update_returning<M>(&self, query: UpdateQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        (**self).update_returning(query).await
    }

    async fn atomic_update_returning<M>(&self, query: UpdateQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        (**self).atomic_update_returning(query).await
    }

    async fn delete(&self, query: DeleteQuery) -> anyhow::Result<usize> {
        (**self).delete(query).await
    }

    async fn delete_returning<M>(&self, query: DeleteQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        (**self).delete_returning(query).await
    }
}

#[async_trait::async_trait]
impl MutationExecutor for TransactionExecutor {
    async fn query<M>(&self, query: FindQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        self.execute(TransactionCommand::find_many::<M>(query)).await
    }

    async fn insert(&self, query: InsertQuery) -> anyhow::Result<usize> {
        self.execute::<()>(TransactionCommand::insert(query)).await?;
        Ok(0)
    }

    async fn insert_returning<M>(&self, mut query: InsertQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        if self.is_mysql() {
            let returning = query
                .returning
                .take()
                .ok_or_else(|| anyhow::anyhow!("MySQL insert returning fallback requires a returning projection."))?;
            let id_index = query
                .fields
                .iter()
                .position(|field| *field == "id")
                .ok_or_else(|| anyhow::anyhow!("MySQL insert returning fallback requires an `id` field."))?;
            let ids = query.rows.iter().map(|row| row[id_index].clone()).collect::<Vec<_>>();
            let table = query.table;

            self.execute::<()>(TransactionCommand::insert(query)).await?;

            return self
                .execute(TransactionCommand::find_many::<M>(FindQuery {
                    fields: returning,
                    from: table,
                    conditions: vec![dinoco_engine::FindWhere::Batch("id", ids)],
                    limit: -1,
                    skip: -1,
                    order_by: None,
                }))
                .await;
        }

        self.execute(TransactionCommand::insert_returning_many::<M>(query)).await
    }

    async fn update(&self, query: UpdateQuery) -> anyhow::Result<usize> {
        self.execute::<()>(TransactionCommand::update(query)).await?;
        Ok(0)
    }

    async fn update_returning<M>(&self, query: UpdateQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        self.execute(TransactionCommand::update_returning::<M>(query)).await
    }

    async fn atomic_update_returning<M>(&self, query: UpdateQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        self.execute(TransactionCommand::atomic_update_returning::<M>(query)).await
    }

    async fn delete(&self, query: DeleteQuery) -> anyhow::Result<usize> {
        self.execute::<()>(TransactionCommand::delete(query)).await?;
        Ok(0)
    }

    async fn delete_returning<M>(&self, query: DeleteQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        self.execute(TransactionCommand::delete_returning::<M>(query)).await
    }
}

#[async_trait::async_trait]
impl MutationExecutor for TransactionContext {
    async fn query<M>(&self, query: FindQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        active_transaction()?.query(query).await
    }

    async fn insert(&self, query: InsertQuery) -> anyhow::Result<usize> {
        active_transaction()?.insert(query).await
    }

    async fn insert_returning<M>(&self, query: InsertQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        active_transaction()?.insert_returning(query).await
    }

    async fn update(&self, query: UpdateQuery) -> anyhow::Result<usize> {
        active_transaction()?.update(query).await
    }

    async fn update_returning<M>(&self, query: UpdateQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        active_transaction()?.update_returning(query).await
    }

    async fn atomic_update_returning<M>(&self, query: UpdateQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        active_transaction()?.atomic_update_returning(query).await
    }

    async fn delete(&self, query: DeleteQuery) -> anyhow::Result<usize> {
        active_transaction()?.delete(query).await
    }

    async fn delete_returning<M>(&self, query: DeleteQuery) -> anyhow::Result<Vec<M>>
    where
        M: DinocoRowModel,
    {
        active_transaction()?.delete_returning(query).await
    }
}

fn active_transaction() -> anyhow::Result<TransactionExecutor> {
    ACTIVE_TRANSACTION
        .try_with(Clone::clone)
        .map_err(|_| anyhow::anyhow!("transaction context used outside its transaction closure"))
}

pub async fn transaction<T, F, Fut>(client: &DinocoClient, callback: F) -> Result<T, TransactionError>
where
    F: FnOnce(TransactionContext) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    let executor = client
        .backend
        .begin_transaction()
        .await
        .map_err(|error| TransactionError::Begin(dinoco_engine::DatabaseError::new(error)))?;

    let operation = ACTIVE_TRANSACTION.scope(executor.clone(), callback(TransactionContext)).await;
    match operation {
        Ok(value) => {
            executor
                .commit()
                .await
                .map_err(|error| TransactionError::Commit(dinoco_engine::DatabaseError::new(error)))?;
            Ok(value)
        }
        Err(error) => {
            let source = TransactionError::from_operation(error);
            match executor.rollback().await {
                Ok(()) => Err(source),
                Err(error) => Err(TransactionError::RollbackFailed {
                    source: Box::new(source),
                    rollback_error: dinoco_engine::DatabaseError::new(error),
                }),
            }
        }
    }
}
