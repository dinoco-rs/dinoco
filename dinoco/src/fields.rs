use std::marker::PhantomData;

use dinoco_engine::{Expression, OrderDirection, SelectStatement};

use crate::{IncludeNode, IntoIncludeNode, Model, OrderBy, Projection, ScalarFieldValue};

#[derive(Debug)]
pub struct ScalarField<T> {
    pub name: &'static str,
    marker: PhantomData<fn() -> T>,
}

#[derive(Debug)]
pub struct RelationField<T> {
    pub name: &'static str,
    marker: PhantomData<fn() -> T>,
}

#[derive(Debug, Clone)]
pub struct RelationQuery<M, S = M> {
    pub name: &'static str,
    pub statement: SelectStatement,
    pub includes: Vec<IncludeNode>,
    marker: PhantomData<fn() -> (M, S)>,
}

impl<T> ScalarField<T> {
    pub const fn new(name: &'static str) -> Self {
        Self { name, marker: PhantomData }
    }

    pub fn eq<V>(self, value: V) -> Expression
    where
        V: ScalarFieldValue<T>,
    {
        Expression::Column(self.name.to_string()).eq(value.into_scalar_field_value())
    }

    pub fn neq<V>(self, value: V) -> Expression
    where
        V: ScalarFieldValue<T>,
    {
        Expression::Column(self.name.to_string()).neq(value.into_scalar_field_value())
    }

    pub fn gt<V>(self, value: V) -> Expression
    where
        V: ScalarFieldValue<T>,
    {
        Expression::Column(self.name.to_string()).gt(value.into_scalar_field_value())
    }

    pub fn gte<V>(self, value: V) -> Expression
    where
        V: ScalarFieldValue<T>,
    {
        Expression::Column(self.name.to_string()).gte(value.into_scalar_field_value())
    }

    pub fn lt<V>(self, value: V) -> Expression
    where
        V: ScalarFieldValue<T>,
    {
        Expression::Column(self.name.to_string()).lt(value.into_scalar_field_value())
    }

    pub fn lte<V>(self, value: V) -> Expression
    where
        V: ScalarFieldValue<T>,
    {
        Expression::Column(self.name.to_string()).lte(value.into_scalar_field_value())
    }

    pub fn asc(self) -> OrderBy {
        OrderBy { column: self.name, direction: OrderDirection::Asc }
    }

    pub fn desc(self) -> OrderBy {
        OrderBy { column: self.name, direction: OrderDirection::Desc }
    }

    pub fn is_null(self) -> Expression {
        Expression::Column(self.name.to_string()).is_null()
    }

    pub fn is_not_null(self) -> Expression {
        Expression::Column(self.name.to_string()).is_not_null()
    }
}

impl ScalarField<String> {
    pub fn includes(self, value: impl Into<String>) -> Expression {
        Expression::Column(self.name.to_string()).like(format!("%{}%", value.into()))
    }

    pub fn starts_with(self, value: impl Into<String>) -> Expression {
        Expression::Column(self.name.to_string()).like(format!("{}%", value.into()))
    }

    pub fn ends_with(self, value: impl Into<String>) -> Expression {
        Expression::Column(self.name.to_string()).like(format!("%{}", value.into()))
    }
}

impl<T> Copy for ScalarField<T> {}

impl<T> Clone for ScalarField<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> RelationField<T> {
    pub const fn new(name: &'static str) -> Self {
        Self { name, marker: PhantomData }
    }

    pub fn select<NS>(self) -> RelationQuery<T, NS>
    where
        T: Model,
        NS: Projection<T>,
    {
        RelationQuery {
            name: self.name,
            statement: SelectStatement::new().from(T::table_name()).select(NS::columns()),
            includes: Vec::new(),
            marker: PhantomData,
        }
    }

    pub fn cond<F>(self, closure: F) -> RelationQuery<T>
    where
        T: Model,
        F: FnOnce(T::Where) -> Expression,
    {
        RelationQuery {
            name: self.name,
            statement: SelectStatement::new().from(T::table_name()).condition(closure(T::Where::default())),
            includes: Vec::new(),
            marker: PhantomData,
        }
    }

    pub fn take(self, value: usize) -> RelationQuery<T>
    where
        T: Model,
    {
        RelationQuery {
            name: self.name,
            statement: SelectStatement::new().from(T::table_name()).limit(value),
            includes: Vec::new(),
            marker: PhantomData,
        }
    }

    pub fn skip(self, value: usize) -> RelationQuery<T>
    where
        T: Model,
    {
        RelationQuery {
            name: self.name,
            statement: SelectStatement::new().from(T::table_name()).skip(value),
            includes: Vec::new(),
            marker: PhantomData,
        }
    }
}

impl<T> Copy for RelationField<T> {}

impl<T> Clone for RelationField<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<M, S> RelationQuery<M, S>
where
    M: Model,
    S: Projection<M>,
{
    pub fn select<NS>(mut self) -> RelationQuery<M, NS>
    where
        NS: Projection<M>,
    {
        self.statement = self.statement.select(NS::columns());

        RelationQuery { name: self.name, statement: self.statement, includes: self.includes, marker: PhantomData }
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

    pub fn include<F, I>(mut self, closure: F) -> Self
    where
        F: FnOnce(M::Include) -> I,
        I: IntoIncludeNode,
    {
        self.includes.push(closure(M::Include::default()).into_include_node());

        self
    }

    pub fn includes<F, I>(self, closure: F) -> Self
    where
        F: FnOnce(M::Include) -> I,
        I: IntoIncludeNode,
    {
        self.include(closure)
    }
}

impl<T> IntoIncludeNode for RelationField<T> {
    fn into_include_node(self) -> IncludeNode {
        IncludeNode { name: self.name, statement: None, includes: Vec::new() }
    }
}

impl<M, S> IntoIncludeNode for RelationQuery<M, S> {
    fn into_include_node(self) -> IncludeNode {
        IncludeNode { name: self.name, statement: Some(self.statement), includes: self.includes }
    }
}
