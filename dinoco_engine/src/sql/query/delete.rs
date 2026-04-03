use crate::{Expression, SqlBuilder, SqlDialect};

pub struct DeleteStatement<'a, D: SqlDialect> {
    pub table: &'a str,
    pub conditions: Vec<Expression>,
    pub dialect: &'a D,
}

impl<'a, D: SqlDialect> DeleteStatement<'a, D> {
    pub fn new(dialect: &'a D) -> Self {
        Self {
            table: "",
            conditions: vec![],
            dialect,
        }
    }

    pub fn from(mut self, table: &'a str) -> Self {
        self.table = table;

        self
    }

    pub fn condition(mut self, cond: Expression) -> Self {
        self.conditions.push(cond);

        self
    }

    pub fn parse_expression(expr: &Expression, builder: &mut SqlBuilder<'_, D>) {
        match expr {
            Expression::Column(name) => builder.push_identifier(name),
            Expression::Value(val) => builder.push_bind_param(val.clone()),
            Expression::String(val) => {
                if val.ends_with("()") {
                    return builder.push(val);
                }

                builder.push_string(val);
            }
            Expression::BinaryOp { left, op, right } => {
                Self::parse_expression(left, builder);
                builder.push_operator(op);
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
