use std::marker::PhantomData;

use dinoco_engine::{DeleteStatement, DinocoAdapter, DinocoClient, Expression};

use crate::{Model, execute_delete};

#[derive(Debug, Clone)]
pub struct DeleteMany<M> {
    conditions: Vec<Expression>,
    marker: PhantomData<fn() -> M>,
}

pub fn delete_many<M>() -> DeleteMany<M>
where
    M: Model,
{
    DeleteMany { conditions: Vec::new(), marker: PhantomData }
}

impl<M> DeleteMany<M>
where
    M: Model,
{
    pub fn cond<F>(mut self, closure: F) -> Self
    where
        F: FnOnce(M::Where) -> Expression,
    {
        self.conditions.push(closure(M::Where::default()));

        self
    }

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
