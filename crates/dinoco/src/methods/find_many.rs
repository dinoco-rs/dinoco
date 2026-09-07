use std::marker::PhantomData;

use dinoco_engine::{
    DinocoClient, DinocoEntity, DinocoProjection, DinocoRowModel, FindOrderBy, FindQuery, FindWhere, PluckValue,
    WhereComplex,
};

use crate::{Field, IncludeLoader, IntoIncludeLoader, load_includes};

pub struct FindMany<M, S = M> {
    query: FindQuery,
    includes: Vec<Box<dyn IncludeLoader<S>>>,
    read_primary: bool,
    complex_where: bool,

    select_marker: PhantomData<S>,
    marker: PhantomData<M>,
}

impl<M, S> FindMany<M, S>
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

    pub fn select<NS>(mut self) -> FindMany<M, NS>
    where
        NS: DinocoProjection<M>,
    {
        self.query.fields = NS::FIELDS;

        FindMany {
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

    pub fn includes<F, I>(mut self, closure: F) -> Self
    where
        F: FnOnce(M::Include) -> I,
        I: IntoIncludeLoader<M, S>,
    {
        self.includes.push(closure(M::Include::default()).into_include_loader());

        self
    }

    pub fn take(mut self, value: i32) -> Self {
        self.query.limit = value;

        self
    }

    pub fn skip(mut self, value: i32) -> Self {
        self.query.skip = value;

        self
    }

    pub fn read_in_primary(mut self) -> Self {
        self.read_primary = true;

        self
    }

    /// Projects onto a single column instead of a full row.
    ///
    /// Unlike `.select(...)`, which still builds a row-model struct, this
    /// returns the raw column values directly (e.g. `Vec<i64>` for an `id`
    /// field).
    pub fn pluck<F, T, C>(self, callback: F) -> Pluck<M, T>
    where
        F: FnOnce(M::Where) -> Field<T, C>,
        PluckValue<T>: DinocoRowModel,
    {
        let name = callback(M::Where::default()).field_name();
        let mut query = self.query;
        query.fields = Box::leak(vec![name].into_boxed_slice());

        Pluck { query, read_primary: self.read_primary, marker: PhantomData }
    }

    /// Applies a post-query mapping to every row.
    ///
    /// Runs after includes are loaded, on the Rust side — it is not a SQL
    /// projection, so the full row (and any loaded relations) is available to
    /// the callback.
    pub fn transform<F, R>(self, callback: F) -> FindManyTransform<M, S, F>
    where
        F: FnMut(S) -> R,
    {
        FindManyTransform { inner: self, transform: callback }
    }

    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<Vec<S>> {
        let mut rows = client.read_backend(self.read_primary).query::<S>(self.query).await?;

        load_includes(self.includes, client, &mut rows, self.read_primary).await?;

        Ok(rows)
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

pub struct Pluck<M, T> {
    query: FindQuery,
    read_primary: bool,
    marker: PhantomData<fn() -> (M, T)>,
}

impl<M, T> Pluck<M, T>
where
    M: DinocoEntity,
    PluckValue<T>: DinocoRowModel,
{
    pub fn read_in_primary(mut self) -> Self {
        self.read_primary = true;

        self
    }

    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<Vec<T>> {
        let rows = client.read_backend(self.read_primary).query::<PluckValue<T>>(self.query).await?;

        Ok(rows.into_iter().map(|value| value.0).collect())
    }
}

pub struct FindManyTransform<M, S, F> {
    inner: FindMany<M, S>,
    transform: F,
}

impl<M, S, F, R> FindManyTransform<M, S, F>
where
    M: DinocoEntity + DinocoRowModel,
    S: DinocoRowModel,
    F: FnMut(S) -> R,
{
    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<Vec<R>> {
        let rows = self.inner.execute(client).await?;
        let mut transform = self.transform;

        Ok(rows.into_iter().map(|row| transform(row)).collect())
    }
}

pub fn find_many<M: DinocoEntity + DinocoRowModel>() -> FindMany<M> {
    FindMany::<M> {
        query: FindQuery::new(M::FIELDS, M::TABLE_NAME, -1, -1),
        includes: Vec::new(),
        read_primary: false,
        complex_where: false,
        select_marker: PhantomData,
        marker: PhantomData,
    }
}
