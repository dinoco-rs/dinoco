use dinoco_engine::{Expression, SelectStatement};

use super::should_qualify_query_column;

pub fn qualify_query_column(value: &str, table_name: &str) -> String {
    if should_qualify_query_column(value) { format!("{table_name}.{value}") } else { value.to_string() }
}

pub fn qualify_expression(expression: Expression, table_name: &str) -> Expression {
    match expression {
        Expression::Column(name) => Expression::Column(qualify_query_column(&name, table_name)),
        Expression::Value(value) => Expression::Value(value),
        Expression::Raw(value) => Expression::Raw(value),
        Expression::IsNull(inner) => Expression::IsNull(Box::new(qualify_expression(*inner, table_name))),
        Expression::IsNotNull(inner) => Expression::IsNotNull(Box::new(qualify_expression(*inner, table_name))),
        Expression::In { expr, values } => {
            Expression::In { expr: Box::new(qualify_expression(*expr, table_name)), values }
        }
        Expression::NotIn { expr, values } => {
            Expression::NotIn { expr: Box::new(qualify_expression(*expr, table_name)), values }
        }
        Expression::And(expressions) => {
            Expression::And(expressions.into_iter().map(|item| qualify_expression(item, table_name)).collect())
        }
        Expression::Or(expressions) => {
            Expression::Or(expressions.into_iter().map(|item| qualify_expression(item, table_name)).collect())
        }
        Expression::BinaryOp { left, op, right } => Expression::BinaryOp {
            left: Box::new(qualify_expression(*left, table_name)),
            op,
            right: Box::new(qualify_expression(*right, table_name)),
        },
    }
}

fn qualify_expression_in_place(expression: &mut Expression, table_name: &str) {
    match expression {
        Expression::Column(name) => {
            if should_qualify_query_column(name) {
                *name = format!("{table_name}.{name}");
            }
        }
        Expression::Value(_) | Expression::Raw(_) => {}
        Expression::IsNull(inner) | Expression::IsNotNull(inner) => {
            qualify_expression_in_place(inner, table_name);
        }
        Expression::In { expr, .. } | Expression::NotIn { expr, .. } => {
            qualify_expression_in_place(expr, table_name);
        }
        Expression::And(expressions) | Expression::Or(expressions) => {
            for expression in expressions {
                qualify_expression_in_place(expression, table_name);
            }
        }
        Expression::BinaryOp { left, right, .. } => {
            qualify_expression_in_place(left, table_name);
            qualify_expression_in_place(right, table_name);
        }
    }
}

pub fn qualify_select_statement(mut statement: SelectStatement, table_name: &str) -> SelectStatement {
    for column in &mut statement.select {
        if should_qualify_query_column(column) {
            *column = format!("{table_name}.{column}");
        }
    }

    for expression in &mut statement.conditions {
        qualify_expression_in_place(expression, table_name);
    }

    for (column, _) in &mut statement.order_by {
        if should_qualify_query_column(column) {
            *column = format!("{table_name}.{column}");
        }
    }

    statement
}
