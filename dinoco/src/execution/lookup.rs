use dinoco_engine::{DinocoAdapter, DinocoResult, Expression, QueryBuilder, SelectStatement};

use super::{DinocoPairRow, DinocoValueRow};

pub(super) async fn query_ids<A>(
    adapter: &A,
    table_name: &str,
    select_column: Option<&str>,
    conditions: Vec<Expression>,
) -> DinocoResult<Vec<dinoco_engine::DinocoValue>>
where
    A: DinocoAdapter,
{
    let select_column = select_column.unwrap_or("id");
    let mut statement = SelectStatement::new().from(table_name).select(&[select_column]);

    for condition in conditions {
        statement = statement.condition(condition);
    }

    let (sql, params) = adapter.dialect().build_select(&statement);
    let rows = adapter.query_as::<DinocoValueRow>(&sql, &params).await?;

    Ok(rows.into_iter().map(|row| row.value).collect())
}

pub(super) async fn query_first_id<A>(
    adapter: &A,
    table_name: &str,
    select_column: &str,
    conditions: &[Expression],
) -> DinocoResult<Option<dinoco_engine::DinocoValue>>
where
    A: DinocoAdapter,
{
    let mut statement = SelectStatement::new().from(table_name).select(&[select_column]).limit(1);

    for condition in conditions {
        statement = statement.condition(condition.clone());
    }

    let (sql, params) = adapter.dialect().build_select(&statement);
    let rows = adapter.query_as::<DinocoValueRow>(&sql, &params).await?;

    Ok(rows.into_iter().next().map(|row| row.value))
}

pub(super) async fn query_pairs<A>(
    adapter: &A,
    table_name: &str,
    left_column: &str,
    right_column: &str,
    left_values: Vec<dinoco_engine::DinocoValue>,
    right_values: Vec<dinoco_engine::DinocoValue>,
) -> DinocoResult<Vec<(dinoco_engine::DinocoValue, dinoco_engine::DinocoValue)>>
where
    A: DinocoAdapter,
{
    let statement = SelectStatement::new()
        .from(table_name)
        .select(&[left_column, right_column])
        .condition(Expression::Column(left_column.to_string()).in_values(left_values))
        .condition(Expression::Column(right_column.to_string()).in_values(right_values));
    let (sql, params) = adapter.dialect().build_select(&statement);
    let rows = adapter.query_as::<DinocoPairRow>(&sql, &params).await?;

    Ok(rows.into_iter().map(|row| (row.left, row.right)).collect())
}
