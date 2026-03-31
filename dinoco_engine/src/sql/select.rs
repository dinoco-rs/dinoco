use crate::{DinocoValue, Expression, QueryDialect};

use super::SqlBuilder;

pub struct SelectStatement<'a, D: QueryDialect> {
    // pub select: Vec<&'a str>,
    pub select: &'a [&'a str],
    pub from: &'a str,
    pub conditions: Vec<Expression>,

    pub dialect: &'a D,
}

impl<'a, D: QueryDialect> SelectStatement<'a, D> {
    pub fn new(dialect: &'a D) -> Self {
        Self {
            select: &[],
            from: "",
            conditions: vec![],
            dialect,
        }
    }

    pub fn select(mut self, columns: &'a [&'a str]) -> Self {
        self.select = columns;

        self
    }

    pub fn from(mut self, table: &'a str) -> Self {
        self.from = table;

        self
    }

    pub fn condition(mut self, cond: Expression) -> Self {
        self.conditions.push(cond);

        self
    }

    pub fn to_sql(&self) -> (String, Vec<DinocoValue>) {
        let estimated_size = self.from.len() + (self.select.len() * 20) + 100;
        let mut builder = SqlBuilder::new(self.dialect, estimated_size);

        builder.push("SELECT ");

        if let Some((first, rest)) = self.select.split_first() {
            builder.push_identifier(first);

            for col in rest {
                builder.push(", ");
                builder.push_identifier(col);
            }
        }

        builder.push(" FROM ");
        builder.push_identifier(self.from);

        if let Some((first, rest)) = self.conditions.split_first() {
            builder.push(" WHERE ");
            Self::parse_expression(first, &mut builder);

            for cond in rest {
                builder.push(" AND ");
                Self::parse_expression(cond, &mut builder);
            }
        }

        builder.finish()
    }

    fn parse_expression(expr: &Expression, builder: &mut SqlBuilder<D>) {
        match expr {
            Expression::Column(name) => builder.push_identifier(&name),
            Expression::Value(val) => builder.push_bind_param(val.clone()),
            Expression::BinaryOp { left, op, right } => {
                builder.push("(");

                Self::parse_expression(left, builder);

                builder.push_operator(&op);

                Self::parse_expression(right, builder);
                builder.push(")");
            }
        }
    }
}
