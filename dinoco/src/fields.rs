use std::marker::PhantomData;
use std::ops::Deref;

use dinoco_engine::{Expression, OrderDirection, SelectStatement};

use crate::{
    CountNode, IncludeNode, IntoCountNode, IntoIncludeNode, Model, OrderBy, Projection, RelationMutationTarget,
    ScalarFieldValue,
};

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

#[derive(Debug)]
pub struct RelationScalarField<T> {
    pub relation_name: &'static str,
    pub name: &'static str,
    marker: PhantomData<fn() -> T>,
}

#[derive(Debug)]
pub struct RelationMutationWhere<W> {
    inner: W,
}

#[derive(Debug, Clone)]
pub struct RelationQuery<M, S = M> {
    pub name: &'static str,
    pub statement: SelectStatement,
    pub includes: Vec<IncludeNode>,
    pub counts: Vec<CountNode>,
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

    pub fn in_values<I, V>(self, values: I) -> Expression
    where
        I: IntoIterator<Item = V>,
        V: ScalarFieldValue<T>,
    {
        Expression::Column(self.name.to_string())
            .in_values(values.into_iter().map(ScalarFieldValue::into_scalar_field_value).collect())
    }

    pub fn not_in_values<I, V>(self, values: I) -> Expression
    where
        I: IntoIterator<Item = V>,
        V: ScalarFieldValue<T>,
    {
        Expression::Column(self.name.to_string())
            .not_in_values(values.into_iter().map(ScalarFieldValue::into_scalar_field_value).collect())
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

impl<T> RelationScalarField<T> {
    pub const fn new(relation_name: &'static str, name: &'static str) -> Self {
        Self { relation_name, name, marker: PhantomData }
    }

    pub fn eq<V>(self, value: V) -> RelationMutationTarget
    where
        V: ScalarFieldValue<T>,
    {
        self.wrap(Expression::Column(self.name.to_string()).eq(value.into_scalar_field_value()))
    }

    pub fn neq<V>(self, value: V) -> RelationMutationTarget
    where
        V: ScalarFieldValue<T>,
    {
        self.wrap(Expression::Column(self.name.to_string()).neq(value.into_scalar_field_value()))
    }

    pub fn gt<V>(self, value: V) -> RelationMutationTarget
    where
        V: ScalarFieldValue<T>,
    {
        self.wrap(Expression::Column(self.name.to_string()).gt(value.into_scalar_field_value()))
    }

    pub fn gte<V>(self, value: V) -> RelationMutationTarget
    where
        V: ScalarFieldValue<T>,
    {
        self.wrap(Expression::Column(self.name.to_string()).gte(value.into_scalar_field_value()))
    }

    pub fn lt<V>(self, value: V) -> RelationMutationTarget
    where
        V: ScalarFieldValue<T>,
    {
        self.wrap(Expression::Column(self.name.to_string()).lt(value.into_scalar_field_value()))
    }

    pub fn lte<V>(self, value: V) -> RelationMutationTarget
    where
        V: ScalarFieldValue<T>,
    {
        self.wrap(Expression::Column(self.name.to_string()).lte(value.into_scalar_field_value()))
    }

    pub fn in_values<I, V>(self, values: I) -> RelationMutationTarget
    where
        I: IntoIterator<Item = V>,
        V: ScalarFieldValue<T>,
    {
        self.wrap(
            Expression::Column(self.name.to_string())
                .in_values(values.into_iter().map(ScalarFieldValue::into_scalar_field_value).collect()),
        )
    }

    pub fn not_in_values<I, V>(self, values: I) -> RelationMutationTarget
    where
        I: IntoIterator<Item = V>,
        V: ScalarFieldValue<T>,
    {
        self.wrap(
            Expression::Column(self.name.to_string())
                .not_in_values(values.into_iter().map(ScalarFieldValue::into_scalar_field_value).collect()),
        )
    }

    pub fn is_null(self) -> RelationMutationTarget {
        self.wrap(Expression::Column(self.name.to_string()).is_null())
    }

    pub fn is_not_null(self) -> RelationMutationTarget {
        self.wrap(Expression::Column(self.name.to_string()).is_not_null())
    }

    fn wrap(self, expression: Expression) -> RelationMutationTarget {
        RelationMutationTarget { relation_name: self.relation_name, expression }
    }
}

impl RelationScalarField<String> {
    pub fn includes(self, value: impl Into<String>) -> RelationMutationTarget {
        self.wrap(Expression::Column(self.name.to_string()).like(format!("%{}%", value.into())))
    }

    pub fn starts_with(self, value: impl Into<String>) -> RelationMutationTarget {
        self.wrap(Expression::Column(self.name.to_string()).like(format!("{}%", value.into())))
    }

    pub fn ends_with(self, value: impl Into<String>) -> RelationMutationTarget {
        self.wrap(Expression::Column(self.name.to_string()).like(format!("%{}", value.into())))
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
            counts: Vec::new(),
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
            counts: Vec::new(),
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
            counts: Vec::new(),
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
            counts: Vec::new(),
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

impl<T> Copy for RelationScalarField<T> {}

impl<T> Clone for RelationScalarField<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<W> RelationMutationWhere<W> {
    pub const fn new(inner: W) -> Self {
        Self { inner }
    }
}

impl<W> Deref for RelationMutationWhere<W> {
    type Target = W;

    fn deref(&self) -> &Self::Target {
        &self.inner
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

        RelationQuery {
            name: self.name,
            statement: self.statement,
            includes: self.includes,
            counts: self.counts,
            marker: PhantomData,
        }
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

    pub fn count<F, I>(mut self, closure: F) -> Self
    where
        F: FnOnce(M::Include) -> I,
        I: IntoCountNode,
    {
        self.counts.push(closure(M::Include::default()).into_count_node());

        self
    }
}

impl<T> IntoIncludeNode for RelationField<T> {
    fn into_include_node(self) -> IncludeNode {
        IncludeNode { name: self.name, statement: None, includes: Vec::new(), counts: Vec::new() }
    }
}

impl<M, S> IntoIncludeNode for RelationQuery<M, S> {
    fn into_include_node(self) -> IncludeNode {
        IncludeNode { name: self.name, statement: Some(self.statement), includes: self.includes, counts: self.counts }
    }
}

impl<T> IntoCountNode for RelationField<T> {
    fn into_count_node(self) -> CountNode {
        CountNode { name: self.name, statement: None }
    }
}

impl<M, S> IntoCountNode for RelationQuery<M, S> {
    fn into_count_node(self) -> CountNode {
        CountNode { name: self.name, statement: Some(self.statement) }
    }
}

impl RelationMutationTarget {
    pub fn and(self, other: Self) -> Self {
        assert_eq!(self.relation_name, other.relation_name, "relation mismatch in RelationMutationTarget::and");

        Self { relation_name: self.relation_name, expression: Expression::and(vec![self.expression, other.expression]) }
    }

    pub fn or(self, other: Self) -> Self {
        assert_eq!(self.relation_name, other.relation_name, "relation mismatch in RelationMutationTarget::or");

        Self { relation_name: self.relation_name, expression: Expression::or(vec![self.expression, other.expression]) }
    }
}
