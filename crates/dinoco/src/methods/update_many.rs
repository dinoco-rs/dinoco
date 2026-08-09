use std::marker::PhantomData;

use dinoco_engine::{
    DinocoClient, DinocoEntity, DinocoProjection, DinocoRowModel, FindQuery, FindWhere, TransactionCommand,
    UpdateQuery, UpdateSet,
};

use crate::{DinocoRelationValue, has_many_to_many_update_sets, load_update_matches};
use crate::{
    IntoTransactionOperation, execute_relation_update_sets, split_update_sets, transaction_many_to_many_writes,
};

pub struct UpdateMany<M> {
    sets: Vec<UpdateSet>,
    conditions: Vec<FindWhere>,
    marker: PhantomData<M>,
}

pub struct UpdateManyReturning<M, S> {
    sets: Vec<UpdateSet>,
    conditions: Vec<FindWhere>,
    marker: PhantomData<fn() -> (M, S)>,
}

pub fn update_many<M>() -> UpdateMany<M>
where
    M: DinocoEntity,
{
    UpdateMany { sets: Vec::new(), conditions: Vec::new(), marker: PhantomData }
}

impl<M> UpdateMany<M>
where
    M: DinocoEntity,
{
    pub fn where_<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::Where) -> FindWhere,
    {
        self.conditions.push(callback(M::Where::default()));

        self
    }

    pub fn update<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::Update) -> UpdateSet,
    {
        self.sets.push(callback(M::Update::default()));

        self
    }

    pub fn returning<S>(self) -> UpdateManyReturning<M, S>
    where
        S: DinocoProjection<M>,
    {
        UpdateManyReturning { sets: self.sets, conditions: self.conditions, marker: PhantomData }
    }

    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<()>
    where
        M: DinocoProjection<M> + DinocoRowModel + DinocoRelationValue,
    {
        if self.sets.is_empty() {
            anyhow::bail!("update_many::<{}>() requires at least one .update(...) call.", M::TABLE_NAME);
        }

        let has_many_to_many = has_many_to_many_update_sets(&self.sets);
        let (sets, connects, disconnects) = split_update_sets(self.sets);
        let parents =
            if has_many_to_many { load_update_matches::<M, M>(&self.conditions, client).await? } else { Vec::new() };

        execute_relation_update_sets(M::TABLE_NAME, &self.conditions, connects, disconnects, &parents, client).await?;

        if !sets.is_empty() {
            let query = UpdateQuery { table: M::TABLE_NAME, sets, conditions: self.conditions, returning: None };
            client.backend.update(query).await?;
        }

        Ok(())
    }
}

impl<M, S> UpdateManyReturning<M, S>
where
    M: DinocoEntity + DinocoProjection<M> + DinocoRowModel + DinocoRelationValue,
    S: DinocoProjection<M> + DinocoRowModel,
{
    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<Vec<S>> {
        if self.sets.is_empty() {
            anyhow::bail!("update_many::<{}>() requires at least one .update(...) call.", M::TABLE_NAME);
        }

        let has_many_to_many = has_many_to_many_update_sets(&self.sets);
        let (sets, connects, disconnects) = split_update_sets(self.sets);
        let parents =
            if has_many_to_many { load_update_matches::<M, M>(&self.conditions, client).await? } else { Vec::new() };

        execute_relation_update_sets(M::TABLE_NAME, &self.conditions, connects, disconnects, &parents, client).await?;

        if sets.is_empty() {
            return load_update_matches::<M, S>(&self.conditions, client).await;
        }

        let query = UpdateQuery { table: M::TABLE_NAME, sets, conditions: self.conditions, returning: Some(S::FIELDS) };

        client.backend.update_returning::<S>(query).await
    }
}

impl<M> IntoTransactionOperation for UpdateMany<M>
where
    M: DinocoEntity,
{
    fn into_transaction_operation(self) -> TransactionCommand {
        if self.sets.is_empty() {
            return TransactionCommand::invalid(format!(
                "update_many::<{}>() requires at least one .update(...) call.",
                M::TABLE_NAME
            ));
        }

        let (sets, connects, disconnects) = split_update_sets(self.sets);
        let (connects, disconnects) =
            match transaction_many_to_many_writes(M::TABLE_NAME, &self.conditions, connects, disconnects) {
                Ok(writes) => writes,
                Err(error) => return TransactionCommand::invalid(error.to_string()),
            };
        let command = if sets.is_empty() {
            TransactionCommand::empty_write()
        } else {
            TransactionCommand::update(UpdateQuery {
                table: M::TABLE_NAME,
                sets,
                conditions: self.conditions,
                returning: None,
            })
        };

        command.with_many_to_many_writes(connects, disconnects)
    }
}

impl<M, S> IntoTransactionOperation for UpdateManyReturning<M, S>
where
    M: DinocoEntity,
    S: DinocoProjection<M> + DinocoRowModel,
{
    fn into_transaction_operation(self) -> TransactionCommand {
        if self.sets.is_empty() {
            return TransactionCommand::invalid(format!(
                "update_many::<{}>() requires at least one .update(...) call.",
                M::TABLE_NAME
            ));
        }

        let (sets, connects, disconnects) = split_update_sets(self.sets);
        let (connects, disconnects) =
            match transaction_many_to_many_writes(M::TABLE_NAME, &self.conditions, connects, disconnects) {
                Ok(writes) => writes,
                Err(error) => return TransactionCommand::invalid(error.to_string()),
            };
        let command = if sets.is_empty() {
            TransactionCommand::find_many::<S>(FindQuery {
                fields: S::FIELDS,
                from: M::TABLE_NAME,
                conditions: self.conditions,
                limit: -1,
                skip: -1,
                order_by: None,
            })
        } else {
            TransactionCommand::update_returning::<S>(UpdateQuery {
                table: M::TABLE_NAME,
                sets,
                conditions: self.conditions,
                returning: Some(S::FIELDS),
            })
        };

        command.with_many_to_many_writes(connects, disconnects)
    }
}
