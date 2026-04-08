use std::marker::PhantomData;

use dinoco_engine::{DinocoAdapter, DinocoClient, Expression, SelectStatement};

use crate::{Model, ReadMode, execute_count};

#[derive(Debug, Clone)]
pub struct Count<M> {
    statement: SelectStatement,
    read_mode: ReadMode,
    marker: PhantomData<fn() -> M>,
}

pub fn count<M>() -> Count<M>
where
    M: Model,
{
    Count {
        statement: SelectStatement::new().from(M::table_name()),
        read_mode: ReadMode::ReplicaPreferred,
        marker: PhantomData,
    }
}

impl<M> Count<M>
where
    M: Model,
{
    pub fn cond<F>(mut self, closure: F) -> Self
    where
        F: FnOnce(M::Where) -> Expression,
    {
        self.statement = self.statement.condition(closure(M::Where::default()));

        self
    }

    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<usize>> + 'a
    where
        A: DinocoAdapter,
    {
        async move { execute_count(self.statement, self.read_mode, client).await }
    }
}
