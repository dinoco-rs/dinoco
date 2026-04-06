use std::marker::PhantomData;

use dinoco_engine::{DinocoAdapter, DinocoClient, Expression, UpdateStatement};

use crate::{UpdateModel, execute_update};

#[derive(Debug, Clone)]
pub struct Update<M> {
    conditions: Vec<Expression>,
    item: Option<M>,
    marker: PhantomData<fn() -> M>,
}

pub fn update<M>() -> Update<M>
where
    M: UpdateModel,
{
    Update { conditions: Vec::new(), item: None, marker: PhantomData }
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

    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<()>> + 'a
    where
        M: 'a,
        A: DinocoAdapter,
    {
        async move {
            let item = self.item.expect("update().values(...) must be called before execute()");
            item.validate_update()?;
            let mut statement = UpdateStatement::new().table(M::table_name());

            for (column, value) in M::update_columns().iter().copied().zip(item.into_update_row().into_iter()) {
                statement = statement.set(column, value);
            }

            for condition in self.conditions {
                statement = statement.condition(condition);
            }

            execute_update(statement, client).await
        }
    }
}
