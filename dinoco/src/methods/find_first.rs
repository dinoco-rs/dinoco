use dinoco_engine::{DinocoAdapter, DinocoClient, Expression};

use crate::{FindMany, IntoCountNode, IntoIncludeNode, Model, OrderBy, Projection};

#[derive(Debug, Clone)]
pub struct FindFirst<M, S = M> {
    pub inner: FindMany<M, S>,
}

pub fn find_first<M>() -> FindFirst<M>
where
    M: Model + Projection<M>,
{
    FindFirst { inner: crate::find_many::<M>().take(1) }
}

impl<M, S> FindFirst<M, S>
where
    M: Model,
    S: Projection<M>,
{
    pub fn select<NS>(self) -> FindFirst<M, NS>
    where
        NS: Projection<M>,
    {
        FindFirst { inner: self.inner.select::<NS>() }
    }

    pub fn cond<F>(self, closure: F) -> Self
    where
        F: FnOnce(M::Where) -> Expression,
    {
        Self { inner: self.inner.cond(closure) }
    }

    pub fn take(self, value: usize) -> Self {
        Self { inner: self.inner.take(value) }
    }

    pub fn skip(self, value: usize) -> Self {
        Self { inner: self.inner.skip(value) }
    }

    pub fn order_by<F>(self, closure: F) -> Self
    where
        F: FnOnce(M::Where) -> OrderBy,
    {
        Self { inner: self.inner.order_by(closure) }
    }

    pub fn includes<F, I>(self, closure: F) -> Self
    where
        F: FnOnce(M::Include) -> I,
        I: IntoIncludeNode,
    {
        Self { inner: self.inner.includes(closure) }
    }

    pub fn count<F, I>(self, closure: F) -> Self
    where
        F: FnOnce(M::Include) -> I,
        I: IntoCountNode,
    {
        Self { inner: self.inner.count(closure) }
    }

    pub fn read_in_primary(self) -> Self {
        Self { inner: self.inner.read_in_primary() }
    }

    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<Option<S>>> + 'a
    where
        A: DinocoAdapter,
    {
        async move {
            let mut rows = crate::execute_many::<M, S, A>(
                self.inner.statement.limit(1),
                &self.inner.includes,
                &self.inner.counts,
                self.inner.read_mode,
                client,
            )
            .await?;

            Ok(rows.drain(..).next())
        }
    }
}
