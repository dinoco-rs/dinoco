use std::marker::PhantomData;

use dinoco_engine::{
    DinocoEntity, DinocoProjection, DinocoRowModel, FindQuery, FindWhere, TransactionCommand, UpdateQuery, UpdateSet,
    WhereComplex,
};

use crate::{
    AtomicUpdateError, DinocoRelationValue, IntoTransactionOperation, MutationExecutor, duplicate_update_field,
    execute_relation_update_sets, has_many_to_many_update_sets, load_update_matches, split_update_sets,
    transaction_many_to_many_writes,
};

pub struct FindAndUpdate<M> {
    sets: Vec<UpdateSet>,
    conditions: Vec<FindWhere>,
    complex_where: bool,
    marker: PhantomData<M>,
}

pub fn find_and_update<M>() -> FindAndUpdate<M>
where
    M: DinocoEntity,
{
    FindAndUpdate { sets: Vec::new(), conditions: Vec::new(), complex_where: false, marker: PhantomData }
}

impl<M> FindAndUpdate<M>
where
    M: DinocoEntity + DinocoProjection<M> + DinocoRowModel + DinocoRelationValue,
{
    pub fn where_<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::Where) -> FindWhere,
    {
        if !self.complex_where {
            self.conditions.push(callback(M::Where::default()));
        }

        self
    }

    pub fn where_complex<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::Where, WhereComplex) -> FindWhere,
    {
        self.conditions = vec![callback(M::Where::default(), WhereComplex)];
        self.complex_where = true;

        self
    }

    pub fn update<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::Update) -> UpdateSet,
    {
        self.sets.push(callback(M::Update::default()));

        self
    }

    pub async fn execute<C>(self, client: C) -> Result<M, AtomicUpdateError>
    where
        C: MutationExecutor,
    {
        if self.sets.is_empty() {
            return Err(AtomicUpdateError::EmptyUpdate);
        }
        if let Some(field) = duplicate_update_field(&self.sets) {
            return Err(AtomicUpdateError::DuplicateField(field));
        }

        let has_many_to_many = has_many_to_many_update_sets(&self.sets);
        let (sets, connects, disconnects) = split_update_sets(self.sets);
        let has_relation_updates = !connects.is_empty() || !disconnects.is_empty();
        let mut parents = if has_relation_updates {
            load_update_matches::<M, M, C>(&self.conditions, &client).await.map_err(AtomicUpdateError::from_database)?
        } else {
            Vec::new()
        };

        let relation_parents = if has_many_to_many { parents.as_slice() } else { &[] };
        execute_relation_update_sets(M::TABLE_NAME, &self.conditions, connects, disconnects, relation_parents, &client)
            .await
            .map_err(AtomicUpdateError::from_database)?;

        if sets.is_empty() {
            return parents.pop().ok_or(AtomicUpdateError::RowNotAffected);
        }

        let query = UpdateQuery {
            table: M::TABLE_NAME,
            sets,
            conditions: self.conditions,
            returning: Some(<M as DinocoProjection<M>>::FIELDS),
        };
        let mut rows = client.atomic_update_returning::<M>(query).await.map_err(AtomicUpdateError::from_database)?;

        rows.pop().ok_or(AtomicUpdateError::RowNotAffected)
    }
}

impl<M> IntoTransactionOperation for FindAndUpdate<M>
where
    M: DinocoEntity + DinocoProjection<M> + DinocoRowModel + DinocoRelationValue,
{
    fn into_transaction_operation(self) -> TransactionCommand {
        if self.sets.is_empty() {
            return TransactionCommand::invalid(format!(
                "find_and_update::<{}>() requires at least one .update(...) call.",
                M::TABLE_NAME
            ));
        }

        let (sets, connects, disconnects) = split_update_sets(self.sets);
        let (connects, disconnects) =
            match transaction_many_to_many_writes(M::TABLE_NAME, &self.conditions, connects, disconnects) {
                Ok(writes) => writes,
                Err(error) => return TransactionCommand::invalid(error.to_string()),
            };
        let missing_message = format!("Record from table '{}' could not be found for update.", M::TABLE_NAME);
        let command = if sets.is_empty() {
            TransactionCommand::find_one::<M>(
                FindQuery {
                    fields: <M as DinocoProjection<M>>::FIELDS,
                    from: M::TABLE_NAME,
                    conditions: self.conditions,
                    limit: -1,
                    skip: -1,
                    order_by: None,
                },
                missing_message,
            )
        } else {
            TransactionCommand::update_returning_one::<M>(
                UpdateQuery {
                    table: M::TABLE_NAME,
                    sets,
                    conditions: self.conditions,
                    returning: Some(<M as DinocoProjection<M>>::FIELDS),
                },
                missing_message,
            )
        };

        command.with_many_to_many_writes(connects, disconnects)
    }
}
