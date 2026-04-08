use std::marker::PhantomData;

use dinoco_engine::{DeleteStatement, DinocoAdapter, DinocoClient, Expression};

use crate::{Model, execute_delete};

#[derive(Debug, Clone)]
pub struct Delete<M> {
    marker: PhantomData<fn() -> M>,
}

#[derive(Debug, Clone)]
pub struct DeleteWithCond<M> {
    conditions: Vec<Expression>,
    marker: PhantomData<fn() -> M>,
}

pub fn delete<M>() -> Delete<M>
where
    M: Model,
{
    Delete { marker: PhantomData }
}

impl<M> Delete<M>
where
    M: Model,
{
    pub fn cond<F>(self, closure: F) -> DeleteWithCond<M>
    where
        F: FnOnce(M::Where) -> Expression,
    {
        DeleteWithCond { conditions: vec![closure(M::Where::default())], marker: PhantomData }
    }
}

impl<M> DeleteWithCond<M>
where
    M: Model,
{
    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<()>> + 'a
    where
        A: DinocoAdapter,
    {
        async move {
            let mut statement = DeleteStatement::new().from(M::table_name());

            for condition in self.conditions {
                statement = statement.condition(condition);
            }

            execute_delete(statement, client).await
        }
    }
}
