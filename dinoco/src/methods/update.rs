use std::marker::PhantomData;

use dinoco_engine::{DinocoAdapter, DinocoClient, Expression, UpdateStatement};

use crate::{
    Projection, RelationMutationModel, RelationMutationTarget, RelationWriteAction, RelationWritePlan, UpdateModel,
    execute_relation_writes, execute_update, execute_update_returning,
};

#[derive(Debug, Clone)]
pub struct Update<M> {
    conditions: Vec<Expression>,
    relation_writes: Vec<(RelationWriteAction, RelationWritePlan)>,
    item: Option<M>,
    marker: PhantomData<fn() -> M>,
}

#[derive(Debug, Clone)]
pub struct UpdateReturning<M, S> {
    conditions: Vec<Expression>,
    relation_writes: Vec<(RelationWriteAction, RelationWritePlan)>,
    item: Option<M>,
    marker: PhantomData<fn() -> (M, S)>,
}

pub fn update<M>() -> Update<M>
where
    M: UpdateModel,
{
    Update { conditions: Vec::new(), relation_writes: Vec::new(), item: None, marker: PhantomData }
}

impl<M> Update<M>
where
    M: UpdateModel,
{
    pub fn cond<F>(mut self, closure: F) -> Self
    where
        F: FnOnce(M::Where) -> Expression,
    {
        self.conditions.push(closure(M::Where::default()));

        self
    }

    pub fn values(mut self, item: M) -> Self {
        self.item = Some(item);

        self
    }

    pub fn returning<S>(self) -> UpdateReturning<M, S>
    where
        M: Projection<M>,
        S: Projection<M>,
    {
        UpdateReturning {
            conditions: self.conditions,
            relation_writes: self.relation_writes,
            item: self.item,
            marker: PhantomData,
        }
    }

    pub fn connect<F>(mut self, closure: F) -> Self
    where
        M: RelationMutationModel,
        F: FnOnce(M::Relations) -> RelationMutationTarget,
    {
        let target = closure(M::Relations::default());
        let plan = M::relation_write_plan(target).expect("unsupported relation in update().connect()");
        self.relation_writes.push((RelationWriteAction::Connect, plan));

        self
    }

    pub fn disconnect<F>(mut self, closure: F) -> Self
    where
        M: RelationMutationModel,
        F: FnOnce(M::Relations) -> RelationMutationTarget,
    {
        let target = closure(M::Relations::default());
        let plan = M::relation_write_plan(target).expect("unsupported relation in update().disconnect()");
        self.relation_writes.push((RelationWriteAction::Disconnect, plan));

        self
    }

    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<()>> + 'a
    where
        M: 'a,
        A: DinocoAdapter,
    {
        async move {
            if let Some(item) = self.item {
                item.validate_update()?;
                let mut statement = UpdateStatement::new().table(M::table_name());

                for (column, value) in M::update_columns().iter().copied().zip(item.into_update_row().into_iter()) {
                    statement = statement.set(column, value);
                }

                for condition in self.conditions.clone() {
                    statement = statement.condition(condition);
                }

                execute_update(statement, client).await?;
            }

            if !self.relation_writes.is_empty() {
                execute_relation_writes(M::table_name(), self.conditions, self.relation_writes, client).await?;
            }

            Ok(())
        }
    }
}

impl<M, S> UpdateReturning<M, S>
where
    M: UpdateModel + Projection<M>,
    S: Projection<M>,
{
    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<Vec<S>>> + 'a
    where
        M: 'a,
        S: 'a,
        A: DinocoAdapter,
    {
        async move {
            if !self.relation_writes.is_empty() {
                return Err(dinoco_engine::DinocoError::ParseError(
                    "update().returning() does not support relation writes.".to_string(),
                ));
            }

            let item = self.item.ok_or_else(|| {
                dinoco_engine::DinocoError::ParseError(
                    "update().returning() requires values(...) before execute().".to_string(),
                )
            })?;

            execute_update_returning::<M, S, A>(self.conditions, item, client).await
        }
    }
}
