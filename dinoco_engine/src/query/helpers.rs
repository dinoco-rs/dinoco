use crate::{AdapterDialect, BinaryOperator, DinocoValue, Expression};

pub fn push_joined<I, T, F>(buf: &mut String, iter: I, sep: &str, mut f: F)
where
    I: IntoIterator<Item = T>,
    F: FnMut(&mut String, T),
{
    let mut first = true;

    for item in iter {
        if !first {
            buf.push_str(sep);
        }

        f(buf, item);

        first = false;
    }
}

pub fn append_limit_skip<D: AdapterDialect + ?Sized>(
    dialect: &D,
    sql: &mut String,
    params: &mut Vec<DinocoValue>,
    limit: Option<usize>,
    skip: Option<usize>,
) {
    match (limit, skip) {
        (Some(limit), Some(skip)) => {
            params.push(DinocoValue::Integer(limit as i64));
            sql.push_str(&format!(" LIMIT {}", dialect.bind_param(params.len())));

            params.push(DinocoValue::Integer(skip as i64));
            sql.push_str(&format!(" OFFSET {}", dialect.bind_param(params.len())));
        }
        (Some(limit), None) => {
            params.push(DinocoValue::Integer(limit as i64));
            sql.push_str(&format!(" LIMIT {}", dialect.bind_param(params.len())));
        }
        (None, Some(skip)) => {
            sql.push_str(&format!(" LIMIT {}", dialect.offset_without_limit()));
            params.push(DinocoValue::Integer(skip as i64));

            sql.push_str(&format!(" OFFSET {}", dialect.bind_param(params.len())));
        }
        (None, None) => {}
    }
}

pub fn render_condition_group_into<D: AdapterDialect + ?Sized>(
    dialect: &D,
    conditions: &[Expression],
    params: &mut Vec<DinocoValue>,
    joiner: &str,
    buf: &mut String,
) {
    push_joined(buf, conditions, joiner, |b, condition| {
        render_expression_into(dialect, condition, params, b);
    });
}

pub fn render_expression_into<D: AdapterDialect + ?Sized>(
    dialect: &D,
    expression: &Expression,
    params: &mut Vec<DinocoValue>,
    buf: &mut String,
) {
    match expression {
        Expression::Column(name) => render_query_identifier_into(dialect, name, buf),
        Expression::Value(value) => {
            params.push(value.clone());

            buf.push_str(&dialect.bind_param(params.len()));
        }
        Expression::Raw(value) => buf.push_str(value),
        Expression::IsNull(inner) => {
            buf.push('(');

            render_expression_into(dialect, inner, params, buf);

            buf.push_str(" IS NULL)");
        }
        Expression::IsNotNull(inner) => {
            buf.push('(');

            render_expression_into(dialect, inner, params, buf);

            buf.push_str(" IS NOT NULL)");
        }
        Expression::In { expr, values } => {
            if values.is_empty() {
                buf.push_str("(1 = 0)");

                return;
            }

            buf.push('(');

            render_expression_into(dialect, expr, params, buf);

            buf.push_str(" IN (");

            push_joined(buf, values, ", ", |b, value| {
                params.push(value.clone());

                b.push_str(&dialect.bind_param(params.len()));
            });

            buf.push_str("))");
        }
        Expression::And(expressions) => {
            buf.push('(');

            render_condition_group_into(dialect, expressions, params, " AND ", buf);

            buf.push(')');
        }
        Expression::Or(expressions) => {
            buf.push('(');

            render_condition_group_into(dialect, expressions, params, " OR ", buf);

            buf.push(')');
        }
        Expression::BinaryOp { left, op, right } => {
            buf.push('(');

            render_expression_into(dialect, left, params, buf);

            let op_str = match op {
                BinaryOperator::Eq => " = ",
                BinaryOperator::Neq => " <> ",
                BinaryOperator::Gt => " > ",
                BinaryOperator::Lt => " < ",
                BinaryOperator::Gte => " >= ",
                BinaryOperator::Lte => " <= ",
                BinaryOperator::Like => " LIKE ",
            };

            buf.push_str(op_str);

            render_expression_into(dialect, right, params, buf);

            buf.push(')');
        }
    }
}

pub fn render_query_identifier_into<D: AdapterDialect + ?Sized>(dialect: &D, value: &str, buf: &mut String) {
    if value == "*" || value.contains(' ') || value.contains('(') || value.contains(')') || value.contains(',') {
        buf.push_str(value);

        return;
    }

    let mut first = true;

    for part in value.split('.') {
        if !first {
            buf.push('.');
        }

        if part == "*" {
            buf.push('*');
        } else {
            buf.push_str(&dialect.identifier(part));
        }

        first = false;
    }
}
