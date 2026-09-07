use std::marker::PhantomData;

use dinoco_engine::{
    DinocoClient, DinocoEntity, DinocoProjection, DinocoRowModel, FindOrderBy, FindQuery, FindWhere, PluckValue,
    WhereComplex,
};

use crate::{Field, IncludeLoader, IntoIncludeLoader, load_includes};

pub struct FindFirst<M, S = M> {
    query: FindQuery,
    includes: Vec<Box<dyn IncludeLoader<S>>>,
    read_primary: bool,
    complex_where: bool,

    select_marker: PhantomData<S>,
    marker: PhantomData<M>,
}

impl<M, S> FindFirst<M, S>
where
    M: DinocoEntity + DinocoRowModel,
    S: DinocoRowModel,
{
    pub fn where_<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::Where) -> FindWhere,
    {
        if !self.complex_where {
            self.query.conditions.push(callback(M::Where::default()));
        }

        self
    }

    pub fn where_complex<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::Where, WhereComplex) -> FindWhere,
    {
        self.query.conditions = vec![callback(M::Where::default(), WhereComplex)];
        self.complex_where = true;

        self
    }

    pub fn includes<F, I>(mut self, closure: F) -> Self
    where
        F: FnOnce(M::Include) -> I,
        I: IntoIncludeLoader<M, S>,
    {
        self.includes.push(closure(M::Include::default()).into_include_loader());

        self
    }

    pub fn select<NS>(mut self) -> FindFirst<M, NS>
    where
        NS: DinocoProjection<M>,
    {
        self.query.fields = NS::FIELDS;

        FindFirst {
            query: self.query,
            includes: Vec::new(),
            read_primary: self.read_primary,
            complex_where: self.complex_where,
            select_marker: PhantomData,
            marker: PhantomData,
        }
    }

    pub fn order_by<F>(mut self, closure: F) -> Self
    where
        F: FnOnce(M::OrderBy) -> FindOrderBy,
    {
        self.query.order_by = Some(closure(M::OrderBy::default()));

        self
    }

    pub fn read_in_primary(mut self) -> Self {
        self.read_primary = true;

        self
    }

    /// Projects onto a single column instead of a full row.
    ///
    /// Unlike `.select(...)`, which still builds a row-model struct, this
    /// returns the raw column value directly (e.g. `Option<i64>` for an `id`
    /// field).
    pub fn pluck<F, T, C>(self, callback: F) -> PluckFirst<M, T>
    where
        F: FnOnce(M::Where) -> Field<T, C>,
        PluckValue<T>: DinocoRowModel,
    {
        let name = callback(M::Where::default()).field_name();
        let mut query = self.query;
        query.fields = Box::leak(vec![name].into_boxed_slice());

        PluckFirst { query, read_primary: self.read_primary, marker: PhantomData }
    }

    /// Applies a post-query mapping to the row, if one was found.
    ///
    /// Runs after includes are loaded, on the Rust side — it is not a SQL
    /// projection, so the full row (and any loaded relations) is available to
    /// the callback.
    pub fn transform<F, R>(self, callback: F) -> FindFirstTransform<M, S, F>
    where
        F: FnOnce(S) -> R,
    {
        FindFirstTransform { inner: self, transform: callback }
    }

    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<Option<S>> {
        let mut rows = client.read_backend(self.read_primary).query::<S>(self.query).await?;

        load_includes(self.includes, client, &mut rows, self.read_primary).await?;

        Ok(rows.into_iter().next())
    }

    /// Splits this builder into the parts `find_batch(...)` needs.
    ///
    /// `.includes(...)` isn't supported inside `find_batch(...)` yet (it
    /// requires follow-up queries of its own), so this fails loudly instead
    /// of silently dropping the relation.
    pub(crate) fn into_batch_parts(self) -> anyhow::Result<(FindQuery, bool)> {
        if !self.includes.is_empty() {
            anyhow::bail!("find_batch(...) does not support .includes(...) yet");
        }

        Ok((self.query, self.read_primary))
    }
}

pub struct PluckFirst<M, T> {
    query: FindQuery,
    read_primary: bool,
    marker: PhantomData<fn() -> (M, T)>,
}

impl<M, T> PluckFirst<M, T>
where
    M: DinocoEntity,
    PluckValue<T>: DinocoRowModel,
{
    pub fn read_in_primary(mut self) -> Self {
        self.read_primary = true;

        self
    }

    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<Option<T>> {
        let rows = client.read_backend(self.read_primary).query::<PluckValue<T>>(self.query).await?;

        Ok(rows.into_iter().next().map(|value| value.0))
    }
}

pub struct FindFirstTransform<M, S, F> {
    inner: FindFirst<M, S>,
    transform: F,
}

impl<M, S, F, R> FindFirstTransform<M, S, F>
where
    M: DinocoEntity + DinocoRowModel,
    S: DinocoRowModel,
    F: FnOnce(S) -> R,
{
    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<Option<R>> {
        let row = self.inner.execute(client).await?;

        Ok(row.map(self.transform))
    }
}

pub fn find_first<M: DinocoEntity + DinocoRowModel>() -> FindFirst<M> {
    FindFirst::<M> {
        query: FindQuery::new(M::FIELDS, M::TABLE_NAME, 1, -1),
        includes: Vec::new(),
        read_primary: false,
        complex_where: false,
        select_marker: PhantomData,
        marker: PhantomData,
    }
}
