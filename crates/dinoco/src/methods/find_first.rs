use std::marker::PhantomData;

use dinoco_engine::{
    DinocoClient, DinocoEntity, DinocoProjection, DinocoRowModel, FindOrderBy, FindQuery, FindWhere, WhereComplex,
};

use crate::{IncludeLoader, IntoIncludeLoader, IntoTransactionOperation, load_includes};

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

    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<Option<S>> {
        let mut rows = client.read_backend(self.read_primary).query::<S>(self.query).await?;

        load_includes(self.includes, client, &mut rows, self.read_primary).await?;

        Ok(rows.into_iter().next())
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

impl<M, S> IntoTransactionOperation for FindFirst<M, S>
where
    M: DinocoEntity + DinocoRowModel,
    S: DinocoRowModel,
{
    fn into_transaction_operation(self) -> dinoco_engine::TransactionCommand {
        if !self.includes.is_empty() {
            return dinoco_engine::TransactionCommand::invalid(
                "find_first().includes(...) is not supported inside a transaction batch yet.",
            );
        }

        dinoco_engine::TransactionCommand::find_first::<S>(self.query)
    }
}
