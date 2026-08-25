use std::marker::PhantomData;

use dinoco_engine::{DinocoProjection, DinocoRowModel, InsertQuery, TransactionCommand};

use crate::{
    CreateError, DinocoInsertable, InsertPayload, IntoTransactionOperation, MutationExecutor,
    execute_insert_models_returning, execute_insert_payloads, execute_insert_payloads_returning, reload_inserted,
};

pub struct InsertMany<M, V = M> {
    items: Vec<V>,
    marker: PhantomData<M>,
}

pub struct InsertManyReturning<M, V = M, S = M> {
    items: Vec<V>,
    marker: PhantomData<fn() -> (M, S)>,
}

pub fn insert_many<M>() -> InsertMany<M>
where
    M: DinocoInsertable,
{
    InsertMany { items: Vec::new(), marker: PhantomData }
}

impl<M, V> InsertMany<M, V>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel + 'static,
    V: InsertPayload<M>,
{
    pub fn values<N, I>(self, items: I) -> InsertMany<M, N>
    where
        N: InsertPayload<M>,
        I: IntoIterator<Item = N>,
    {
        InsertMany { items: items.into_iter().collect(), marker: PhantomData }
    }

    pub fn returning<S>(self) -> InsertManyReturning<M, V, S>
    where
        S: DinocoProjection<M>,
    {
        InsertManyReturning { items: self.items, marker: PhantomData }
    }

    pub async fn execute<C>(self, client: C) -> anyhow::Result<()>
    where
        C: MutationExecutor,
    {
        execute_insert_payloads::<M, V, V, C>(&self.items, &client).await.map_err(CreateError::from_database)?;

        Ok(())
    }
}

impl<M, V, S> InsertManyReturning<M, V, S>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel + 'static,
    V: InsertPayload<M>,
    S: DinocoProjection<M> + DinocoRowModel,
{
    pub async fn execute<C>(self, client: C) -> anyhow::Result<Vec<S>>
    where
        C: MutationExecutor,
    {
        if !V::HAS_NESTED {
            let models = self.items.iter().map(InsertPayload::dinoco_insert_model).collect::<Vec<_>>();

            return execute_insert_models_returning::<M, S, C>(&models, &client)
                .await
                .map_err(|error| CreateError::from_database(error).into());
        }

        let inserted = execute_insert_payloads_returning::<M, V, V, C>(&self.items, &client)
            .await
            .map_err(CreateError::from_database)?;
        reload_inserted::<M, S, C>(&inserted, &client).await.map_err(|error| CreateError::from_database(error).into())
    }
}

impl<M, V> IntoTransactionOperation for InsertMany<M, V>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel + 'static,
    V: InsertPayload<M>,
{
    fn into_transaction_operation(self) -> TransactionCommand {
        if V::HAS_TRANSACTION_NESTED {
            return TransactionCommand::invalid(
                "Nested relation inserts are not supported inside a transaction batch yet.",
            );
        }

        let models = self.items.iter().map(InsertPayload::dinoco_insert_model).collect::<Vec<_>>();
        let mut writes = Vec::new();
        for (item, model) in self.items.iter().zip(&models) {
            match item.dinoco_transaction_many_to_many_writes(model) {
                Ok(item_writes) => writes.extend(item_writes),
                Err(error) => return TransactionCommand::invalid(error.to_string()),
            }
        }
        let rows = models.iter().map(DinocoInsertable::dinoco_insert_values).collect::<Vec<_>>();
        if rows.is_empty() {
            return TransactionCommand::empty_write();
        }
        TransactionCommand::insert(InsertQuery {
            table: M::TABLE_NAME,
            fields: M::INSERT_FIELDS.to_vec(),
            rows,
            returning: None,
        })
        .with_appended_many_to_many_connects(writes)
    }
}

impl<M, V, S> IntoTransactionOperation for InsertManyReturning<M, V, S>
where
    M: DinocoInsertable + DinocoProjection<M> + DinocoRowModel + 'static,
    V: InsertPayload<M>,
    S: DinocoProjection<M> + DinocoRowModel,
{
    fn into_transaction_operation(self) -> TransactionCommand {
        if V::HAS_TRANSACTION_NESTED {
            return TransactionCommand::invalid(
                "Nested relation inserts are not supported inside a transaction batch yet.",
            );
        }

        let models = self.items.iter().map(InsertPayload::dinoco_insert_model).collect::<Vec<_>>();
        let mut writes = Vec::new();
        for (item, model) in self.items.iter().zip(&models) {
            match item.dinoco_transaction_many_to_many_writes(model) {
                Ok(item_writes) => writes.extend(item_writes),
                Err(error) => return TransactionCommand::invalid(error.to_string()),
            }
        }
        let rows = models.iter().map(DinocoInsertable::dinoco_insert_values).collect::<Vec<_>>();
        if rows.is_empty() {
            return TransactionCommand::empty_rows::<S>();
        }
        TransactionCommand::insert_returning_many::<S>(InsertQuery {
            table: M::TABLE_NAME,
            fields: M::INSERT_FIELDS.to_vec(),
            rows,
            returning: Some(S::FIELDS),
        })
        .with_appended_many_to_many_connects(writes)
    }
}
