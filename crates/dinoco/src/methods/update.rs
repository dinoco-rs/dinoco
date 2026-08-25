use std::marker::PhantomData;

use dinoco_engine::{
    DinocoEntity, DinocoProjection, DinocoRowModel, FindQuery, FindWhere, TransactionCommand, UpdateQuery, UpdateSet,
};

use crate::{DinocoRelationValue, has_many_to_many_update_sets, load_update_matches};
use crate::{
    IntoTransactionOperation, MutationExecutor, UpdateError, duplicate_update_field, execute_relation_update_sets,
    split_update_sets, transaction_many_to_many_writes,
};

pub struct Update<M> {
    sets: Vec<UpdateSet>,
    conditions: Vec<FindWhere>,
    marker: PhantomData<M>,
}

pub struct UpdateReturning<M, S> {
    sets: Vec<UpdateSet>,
    conditions: Vec<FindWhere>,
    marker: PhantomData<fn() -> (M, S)>,
}

pub fn update<M>() -> Update<M>
where
    M: DinocoEntity,
{
    Update { sets: Vec::new(), conditions: Vec::new(), marker: PhantomData }
}

impl<M> Update<M>
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

    pub fn returning<S>(self) -> UpdateReturning<M, S>
    where
        S: DinocoProjection<M>,
    {
        UpdateReturning { sets: self.sets, conditions: self.conditions, marker: PhantomData }
    }

    pub async fn execute<C>(self, client: C) -> anyhow::Result<()>
    where
        M: DinocoProjection<M> + DinocoRowModel + DinocoRelationValue,
        C: MutationExecutor,
    {
        if self.sets.is_empty() {
            return Err(UpdateError::InvalidOperation(format!(
                "update::<{}>() requires at least one .update(...) call.",
                M::TABLE_NAME
            ))
            .into());
        }
        if let Some(field) = duplicate_update_field(&self.sets) {
            return Err(UpdateError::InvalidOperation(format!(
                "field `{field}` is updated more than once in one statement"
            ))
            .into());
        }

        let has_many_to_many = has_many_to_many_update_sets(&self.sets);
        let (sets, connects, disconnects) = split_update_sets(self.sets);
        let parents = if has_many_to_many {
            load_update_matches::<M, M, C>(&self.conditions, &client).await.map_err(UpdateError::from_database)?
        } else {
            Vec::new()
        };

        execute_relation_update_sets(M::TABLE_NAME, &self.conditions, connects, disconnects, &parents, &client)
            .await
            .map_err(UpdateError::from_database)?;

        if !sets.is_empty() {
            let query = UpdateQuery { table: M::TABLE_NAME, sets, conditions: self.conditions, returning: None };
            client.update(query).await.map_err(UpdateError::from_database)?;
        }

        Ok(())
    }
}

impl<M, S> UpdateReturning<M, S>
where
    M: DinocoEntity + DinocoProjection<M> + DinocoRowModel + DinocoRelationValue,
    S: DinocoProjection<M> + DinocoRowModel,
{
    pub async fn execute<C>(self, client: C) -> anyhow::Result<Vec<S>>
    where
        C: MutationExecutor,
    {
        if self.sets.is_empty() {
            return Err(UpdateError::InvalidOperation(format!(
                "update::<{}>() requires at least one .update(...) call.",
                M::TABLE_NAME
            ))
            .into());
        }
        if let Some(field) = duplicate_update_field(&self.sets) {
            return Err(UpdateError::InvalidOperation(format!(
                "field `{field}` is updated more than once in one statement"
            ))
            .into());
        }

        let has_many_to_many = has_many_to_many_update_sets(&self.sets);
        let (sets, connects, disconnects) = split_update_sets(self.sets);
        let parents = if has_many_to_many {
            load_update_matches::<M, M, C>(&self.conditions, &client).await.map_err(UpdateError::from_database)?
        } else {
            Vec::new()
        };

        execute_relation_update_sets(M::TABLE_NAME, &self.conditions, connects, disconnects, &parents, &client)
            .await
            .map_err(UpdateError::from_database)?;

        if sets.is_empty() {
            return load_update_matches::<M, S, C>(&self.conditions, &client)
                .await
                .map_err(|error| UpdateError::from_database(error).into());
        }

        let query = UpdateQuery { table: M::TABLE_NAME, sets, conditions: self.conditions, returning: Some(S::FIELDS) };

        client.update_returning::<S>(query).await.map_err(|error| UpdateError::from_database(error).into())
    }
}

impl<M> IntoTransactionOperation for Update<M>
where
    M: DinocoEntity,
{
    fn into_transaction_operation(self) -> TransactionCommand {
        if self.sets.is_empty() {
            return TransactionCommand::invalid(format!(
                "update::<{}>() requires at least one .update(...) call.",
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

impl<M, S> IntoTransactionOperation for UpdateReturning<M, S>
where
    M: DinocoEntity,
    S: DinocoProjection<M> + DinocoRowModel,
{
    fn into_transaction_operation(self) -> TransactionCommand {
        if self.sets.is_empty() {
            return TransactionCommand::invalid(format!(
                "update::<{}>() requires at least one .update(...) call.",
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
