use std::future::Future;
use std::marker::PhantomData;

use dinoco_engine::{
    DinocoEntity, DinocoProjection, DinocoRowModel, FindOrderBy, FindQuery, FindWhere, PluckValue, WhereComplex,
};
use futures_util::TryFutureExt;

use crate::{Field, IncludeLoader, IntoIncludeLoader, ReadExecutor, load_includes, single_field};

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
        self.query.order_by.push(closure(M::OrderBy::default()));

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
        query.fields = single_field(name, M::FIELDS);

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

    // Not an `async fn`: the builder is taken apart first, so the future
    // keeps each part once instead of the builder and a copy of it.
    pub fn execute<C>(self, client: C) -> impl Future<Output = anyhow::Result<Option<S>>>
    where
        C: ReadExecutor,
    {
        let Self { query, includes, read_primary, .. } = self;

        async move {
            let target = client.read_target(read_primary)?;
            let mut rows = target.query::<S>(query).await?;

            load_includes(includes, &target, &mut rows).await?;

            Ok(rows.into_iter().next())
        }
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

    pub fn execute<C>(self, client: C) -> impl Future<Output = anyhow::Result<Option<T>>>
    where
        C: ReadExecutor,
    {
        let Self { query, read_primary, .. } = self;

        async move {
            let target = client.read_target(read_primary)?;
            let rows = target.query::<PluckValue<T>>(query).await?;

            Ok(rows.into_iter().next().map(|value| value.0))
        }
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
    pub fn execute<C>(self, client: C) -> impl Future<Output = anyhow::Result<Option<R>>>
    where
        C: ReadExecutor,
    {
        let transform = self.transform;

        self.inner.execute(client).map_ok(|row| row.map(transform))
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
