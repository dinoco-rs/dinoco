use crate::{DinocoValue, Expression, OrderDirection, QueryDialect, SqlBuilder};

pub struct SelectStatement<'a, D: QueryDialect> {
    pub select: &'a [&'a str],
    pub from: &'a str,
    pub conditions: Vec<Expression>,

    pub limit: Option<usize>,
    pub skip: Option<usize>,

    pub order_by: Vec<(&'a str, OrderDirection)>,

    pub dialect: &'a D,
}

impl<'a, D: QueryDialect> SelectStatement<'a, D> {
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

    pub fn to_sql(&self) -> (String, Vec<DinocoValue>) {
        let estimated_size = self.from.len() + (self.select.len() * 20) + (self.conditions.len() * 30) + (self.order_by.len() * 20) + 150;

        let mut builder = SqlBuilder::new(self.dialect, estimated_size);

        builder.push("SELECT ");

        if let Some((first, rest)) = self.select.split_first() {
            builder.push(first);

            for col in rest {
                builder.push(", ");
                builder.push(col);
            }
        } else {
            builder.push("*");
        }

        builder.push(" FROM ");
        builder.push(self.from);

        if let Some((first, rest)) = self.conditions.split_first() {
            builder.push(" WHERE ");
            Self::parse_expression(first, &mut builder);

            for cond in rest {
                builder.push(" AND ");
                Self::parse_expression(cond, &mut builder);
            }
        }

        if let Some((first, rest)) = self.order_by.split_first() {
            builder.push(" ORDER BY ");

            builder.push_identifier(first.0);
            builder.push(if first.1 == OrderDirection::Asc { " ASC" } else { " DESC" });

            for col in rest {
                builder.push(", ");
                builder.push_identifier(col.0);
                builder.push(if col.1 == OrderDirection::Asc { " ASC" } else { " DESC" });
            }
        }

        if let Some(limit) = self.limit {
            builder.push(" LIMIT ");

            let limit_str = limit.to_string();
            builder.push(&limit_str);
        }

        if let Some(skip) = self.skip {
            builder.push(" OFFSET ");

            let skip_str = skip.to_string();
            builder.push(&skip_str);
        }

        builder.finish()
    }

    fn parse_expression(expr: &Expression, builder: &mut SqlBuilder<D>) {
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
                // builder.push("(");

                Self::parse_expression(left, builder);

                builder.push_operator(&op);

                Self::parse_expression(right, builder);
                // builder.push(")");
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
