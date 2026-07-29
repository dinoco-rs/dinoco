use std::marker::PhantomData;

use dinoco_engine::{
    DinocoClient, DinocoEntity, DinocoProjection, DinocoRowModel, FindWhere, TransactionCommand, UpdateQuery,
    UpdateSet, WhereComplex,
};

use crate::{IntoTransactionOperation, split_update_sets};

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
    M: DinocoEntity + DinocoProjection<M> + DinocoRowModel,
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

    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<M> {
        if self.sets.is_empty() {
            anyhow::bail!("find_and_update::<{}>() requires at least one .update(...) call.", M::TABLE_NAME);
        }

        let (sets, connects, disconnects) = split_update_sets(self.sets);

        if !connects.is_empty() || !disconnects.is_empty() {
            anyhow::bail!("find_and_update::<{}>() does not support connect/disconnect.", M::TABLE_NAME);
        }

        let query = UpdateQuery {
            table: M::TABLE_NAME,
            sets,
            conditions: self.conditions,
            returning: Some(<M as DinocoProjection<M>>::FIELDS),
        };
        let mut rows = client.backend.update_returning::<M>(query).await?;

        rows.pop()
            .ok_or_else(|| anyhow::anyhow!("Record from table '{}' could not be found for update.", M::TABLE_NAME))
    }
}

impl<M> IntoTransactionOperation for FindAndUpdate<M>
where
    M: DinocoEntity + DinocoProjection<M> + DinocoRowModel,
{
    fn into_transaction_operation(self) -> TransactionCommand {
        if self.sets.is_empty() {
            return TransactionCommand::invalid(format!(
                "find_and_update::<{}>() requires at least one .update(...) call.",
                M::TABLE_NAME
            ));
        }

        let (sets, connects, disconnects) = split_update_sets(self.sets);
        if !connects.is_empty() || !disconnects.is_empty() {
            return TransactionCommand::invalid(format!(
                "find_and_update::<{}>() does not support connect/disconnect.",
                M::TABLE_NAME
            ));
        }

        TransactionCommand::update_returning_one::<M>(
            UpdateQuery {
                table: M::TABLE_NAME,
                sets,
                conditions: self.conditions,
                returning: Some(<M as DinocoProjection<M>>::FIELDS),
            },
            format!("Record from table '{}' could not be found for update.", M::TABLE_NAME),
        )
    }
}
