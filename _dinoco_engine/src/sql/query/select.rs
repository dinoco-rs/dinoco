use crate::{Expression, OrderDirection, SqlBuilder, SqlDialect};

pub struct SelectStatement<'a, D: SqlDialect> {
    pub select: &'a [&'a str],
    pub from: &'a str,
    pub conditions: Vec<Expression>,

    pub limit: Option<usize>,
    pub skip: Option<usize>,

    pub order_by: Vec<(&'a str, OrderDirection)>,

    pub dialect: &'a D,
}

impl<'a, D: SqlDialect> SelectStatement<'a, D> {
    pub fn new(dialect: &'a D) -> Self {
        Self {
            select: &[],
            from: "",
            conditions: vec![],
            limit: None,
            skip: None,
            order_by: vec![],
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

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);

        self
    }

    pub fn skip(mut self, skip: usize) -> Self {
        self.skip = Some(skip);

        self
    }

    pub fn order_by(mut self, column: &'a str, direction: OrderDirection) -> Self {
        self.order_by.push((column, direction));

        self
    }

    pub fn parse_expression(expr: &Expression, builder: &mut SqlBuilder<D>) {
        match expr {
            Expression::Column(name) => builder.push_identifier(&name),
            Expression::Value(val) => builder.push_bind_param(val.clone()),
            Expression::String(val) => {
                if val.ends_with("()") {
                    return builder.push(val);
                }

                builder.push_string(val);
            }
            Expression::BinaryOp { left, op, right } => {
                Self::parse_expression(left, builder);

                builder.push_operator(&op);

                Self::parse_expression(right, builder);
            }
            Expression::IsNull(inner_expr) => {
                Self::parse_expression(inner_expr, builder);
                builder.push(" IS NULL");
            }
            Expression::IsNotNull(inner_expr) => {
                Self::parse_expression(inner_expr, builder);
                builder.push(" IS NOT NULL");
            }
        }
    }
}
