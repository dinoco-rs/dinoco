use std::collections::HashMap;
use std::future::Future;

use dinoco_engine::{
    DeleteStatement, DinocoAdapter, DinocoClient, DinocoResult, Expression, InsertStatement, QueryBuilder,
    UpdateStatement,
};

use crate::{ConnectionUpdatePlan, RelationLinkPlan, RelationWriteAction, RelationWritePlan};

use super::{lookup::query_ids, lookup::query_pairs};

pub fn execute_insert_relation_links<'a, A>(
    relation_links: Vec<RelationLinkPlan>,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<()>> + 'a
where
    A: DinocoAdapter,
{
    async move {
        if relation_links.is_empty() {
            return Ok(());
        }

        let adapter = client.primary();

        for link_group in group_relation_links(relation_links) {
            let statement =
                InsertStatement::new().into(link_group.table_name).columns(link_group.columns).values(link_group.rows);
            let (sql, params) = adapter.dialect().build_insert(&statement);
            adapter.execute(&sql, &params).await?;
        }

        Ok(())
    }
}

pub fn execute_connection_updates<'a, A>(
    connection_updates: Vec<ConnectionUpdatePlan>,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<()>> + 'a
where
    A: DinocoAdapter,
{
    async move {
        if connection_updates.is_empty() {
            return Ok(());
        }

        let adapter = client.primary();

        for update in connection_updates {
            let mut statement = UpdateStatement::new().table(update.table_name);

            for (column, value) in update.columns.iter().copied().zip(update.row.into_iter()) {
                statement = statement.set(column, value);
            }

            for condition in update.conditions {
                statement = statement.condition(condition);
            }

            let (sql, params) = adapter.dialect().build_update(&statement);
            adapter.execute(&sql, &params).await?;
        }

        Ok(())
    }
}

pub fn execute_relation_writes<'a, A>(
    table_name: &'static str,
    conditions: Vec<Expression>,
    writes: Vec<(RelationWriteAction, RelationWritePlan)>,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<()>> + 'a
where
    A: DinocoAdapter,
{
    async move {
        if writes.is_empty() {
            return Ok(());
        }

        let adapter = client.primary();
        let source_key_column = writes[0].1.source_key_column;
        let source_ids = query_ids(adapter, table_name, Some(source_key_column), conditions).await?;

        if source_ids.is_empty() {
            return Ok(());
        }

        for (action, plan) in writes {
            let target_ids =
                query_ids(adapter, plan.target_table_name, Some(plan.target_key_column), vec![plan.target_expression])
                    .await?;

            if target_ids.is_empty() {
                continue;
            }

            match action {
                RelationWriteAction::Connect => {
                    let existing_rows = query_pairs(
                        adapter,
                        plan.join_table_name,
                        plan.source_join_column,
                        plan.target_join_column,
                        source_ids.clone(),
                        target_ids.clone(),
                    )
                    .await?;
                    let rows = build_missing_relation_rows(source_ids.clone(), target_ids, existing_rows);

                    if rows.is_empty() {
                        continue;
                    }

                    let statement = InsertStatement::new()
                        .into(plan.join_table_name)
                        .columns(&[plan.source_join_column, plan.target_join_column])
                        .values(rows);
                    let (sql, params) = adapter.dialect().build_insert(&statement);

                    adapter.execute(&sql, &params).await?;
                }
                RelationWriteAction::Disconnect => {
                    let statement = DeleteStatement::new()
                        .from(plan.join_table_name)
                        .condition(
                            Expression::Column(plan.source_join_column.to_string()).in_values(source_ids.clone()),
                        )
                        .condition(Expression::Column(plan.target_join_column.to_string()).in_values(target_ids));
                    let (sql, params) = adapter.dialect().build_delete(&statement);

                    adapter.execute(&sql, &params).await?;
                }
            }
        }

        Ok(())
    }
}

pub fn execute_delete<'a, A>(
    statement: DeleteStatement,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<()>> + 'a
where
    A: DinocoAdapter,
{
    async move {
        let adapter = client.primary();
        let (sql, params) = adapter.dialect().build_delete(&statement);

        adapter.execute(&sql, &params).await
    }
}

fn group_relation_links(relation_links: Vec<RelationLinkPlan>) -> Vec<GroupedRelationLinks> {
    let mut grouped = HashMap::<(&'static str, &'static [&'static str]), Vec<Vec<dinoco_engine::DinocoValue>>>::new();

    for link in relation_links {
        grouped.entry((link.table_name, link.columns)).or_default().push(link.row);
    }

    grouped
        .into_iter()
        .map(|((table_name, columns), rows)| GroupedRelationLinks { table_name, columns, rows })
        .collect()
}

fn build_missing_relation_rows(
    left_values: Vec<dinoco_engine::DinocoValue>,
    right_values: Vec<dinoco_engine::DinocoValue>,
    existing_rows: Vec<(dinoco_engine::DinocoValue, dinoco_engine::DinocoValue)>,
) -> Vec<Vec<dinoco_engine::DinocoValue>> {
    let mut rows = Vec::new();

    for left in left_values {
        for right in &right_values {
            if existing_rows
                .iter()
                .any(|(existing_left, existing_right)| existing_left == &left && existing_right == right)
            {
                continue;
            }

            rows.push(vec![left.clone(), right.clone()]);
        }
    }

    rows
}

struct GroupedRelationLinks {
    table_name: &'static str,
    columns: &'static [&'static str],
    rows: Vec<Vec<dinoco_engine::DinocoValue>>,
}
