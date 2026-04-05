use std::marker::PhantomData;

use dinoco_engine::{DinocoAdapter, DinocoClient, Expression, SelectStatement};

use crate::{IncludeNode, IntoIncludeNode, Model, OrderBy, Projection, ReadMode, execute_many};

#[derive(Debug, Clone)]
pub struct FindMany<M, S = M> {
    pub statement: SelectStatement,
    pub includes: Vec<IncludeNode>,
    pub read_mode: ReadMode,
    marker: PhantomData<fn() -> (M, S)>,
}

pub fn find_many<M>() -> FindMany<M>
where
    M: Model + Projection<M>,
{
    FindMany {
        statement: SelectStatement::new().from(M::table_name()).select(M::columns()),
        includes: Vec::new(),
        read_mode: ReadMode::ReplicaPreferred,
        marker: PhantomData,
    }
}

impl<M, S> FindMany<M, S>
where
    M: Model,
    S: Projection<M>,
{
    pub fn select<NS>(mut self) -> FindMany<M, NS>
    where
        NS: Projection<M>,
    {
        self.statement = self.statement.select(NS::columns());

        FindMany { statement: self.statement, includes: self.includes, read_mode: self.read_mode, marker: PhantomData }
    }

    pub fn cond<F>(mut self, closure: F) -> Self
    where
        F: FnOnce(M::Where) -> Expression,
    {
        self.statement = self.statement.condition(closure(M::Where::default()));

        self
    }

    pub fn take(mut self, value: usize) -> Self {
        self.statement = self.statement.limit(value);

        self
    }

    pub fn skip(mut self, value: usize) -> Self {
        self.statement = self.statement.skip(value);

        self
    }

    pub fn order_by<F>(mut self, closure: F) -> Self
    where
        F: FnOnce(M::Where) -> OrderBy,
    {
        let order_by = closure(M::Where::default());

        self.statement = self.statement.order_by(order_by.column, order_by.direction);

        self
    }

    pub fn includes<F, I>(mut self, closure: F) -> Self
    where
        F: FnOnce(M::Include) -> I,
        I: IntoIncludeNode,
    {
        self.includes.push(closure(M::Include::default()).into_include_node());

        self
    }

    pub fn read_in_primary(mut self) -> Self {
        self.read_mode = ReadMode::Primary;

        self
    }

    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<Vec<S>>> + 'a
    where
        A: DinocoAdapter,
    {
        async move { execute_many::<M, S, A>(self.statement, &self.includes, self.read_mode, client).await }
    }
}
