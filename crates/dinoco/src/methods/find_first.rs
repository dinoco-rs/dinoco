use std::marker::PhantomData;

use dinoco_engine::{DinocoClient, DinocoEntity, DinocoProjection, DinocoSqlite, FindOrderBy, FindQuery, FindWhere};

use crate::{IncludeLoader, IntoIncludeLoader, load_includes};

pub struct FindFirst<M, S = M> {
    query: FindQuery,
    includes: Vec<Box<dyn IncludeLoader<S>>>,

    select_marker: PhantomData<S>,
    marker: PhantomData<M>,
}

impl<M, S> FindFirst<M, S>
where
    M: DinocoEntity + DinocoSqlite,
    S: DinocoSqlite,
{
    pub fn where_<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::Where) -> FindWhere,
    {
        self.query.conditions.push(callback(M::Where::default()));

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

        FindFirst { query: self.query, includes: Vec::new(), select_marker: PhantomData, marker: PhantomData }
    }

    pub fn order_by<F>(mut self, closure: F) -> Self
    where
        F: FnOnce(M::OrderBy) -> FindOrderBy,
    {
        self.query.order_by = Some(closure(M::OrderBy::default()));

        self
    }

    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<Option<S>> {
        let mut rows = client.backend.query::<S>(self.query).await?;

        load_includes(self.includes, client, &mut rows).await?;

        Ok(rows.into_iter().next())
    }
}

pub fn find_first<M: DinocoEntity + DinocoSqlite>() -> FindFirst<M> {
    FindFirst::<M> {
        query: FindQuery::new(M::FIELDS, M::TABLE_NAME, 1, -1),
        includes: Vec::new(),
        select_marker: PhantomData,
        marker: PhantomData,
    }
}
