use std::future::Future;

use dinoco_engine::{
    DeleteStatement, DinocoAdapter, DinocoClient, DinocoError, DinocoGenericRow, DinocoResult, DinocoRow,
    InsertStatement, QueryBuilder, SelectStatement, UpdateStatement,
};

use crate::{
    ConnectionUpdatePlan, InsertModel, Model, Projection, ReadMode, RelationLinkPlan, RelationWriteAction,
    RelationWritePlan, UpdateModel,
};

struct DinocoCountRow {
    count: i64,
}

struct DinocoValueRow {
    value: dinoco_engine::DinocoValue,
}

struct DinocoPairRow {
    left: dinoco_engine::DinocoValue,
    right: dinoco_engine::DinocoValue,
}

impl DinocoRow for DinocoCountRow {
    fn from_row<R: DinocoGenericRow>(row: &R) -> DinocoResult<Self> {
        Ok(Self { count: row.get(0)? })
    }
}

impl DinocoRow for DinocoValueRow {
    fn from_row<R: DinocoGenericRow>(row: &R) -> DinocoResult<Self> {
        Ok(Self { value: row.get_value(0)? })
    }
}

impl DinocoRow for DinocoPairRow {
    fn from_row<R: DinocoGenericRow>(row: &R) -> DinocoResult<Self> {
        Ok(Self { left: row.get_value(0)?, right: row.get_value(1)? })
    }
}

pub fn execute_many<'a, M, S, A>(
    statement: SelectStatement,
    includes: &'a [crate::IncludeNode],
    counts: &'a [crate::CountNode],
    read_mode: ReadMode,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<Vec<S>>> + 'a
where
    M: Model,
    S: Projection<M>,
    A: DinocoAdapter,
{
    async move {
        let adapter = client.read_adapter(matches!(read_mode, ReadMode::Primary));
        let (sql, params) = adapter.dialect().build_select(&statement);
        let mut rows = adapter.query_as::<S>(&sql, &params).await?;

        S::load_includes(&mut rows, includes, client, read_mode).await?;
        S::load_counts(&mut rows, counts, client, read_mode).await?;

        Ok(rows)
    }
}

pub fn execute_first<'a, M, S, A>(
    statement: SelectStatement,
    read_mode: ReadMode,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<Option<S>>> + 'a
where
    M: Model,
    S: Projection<M>,
    A: DinocoAdapter,
{
    async move {
        // statement.limit = Some(1);

        let mut rows = execute_many::<M, S, A>(statement, &[], &[], read_mode, client).await?;

        Ok(rows.drain(..).next())
    }
}

pub fn execute_count<'a, A>(
    statement: SelectStatement,
    read_mode: ReadMode,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<usize>> + 'a
where
    A: DinocoAdapter,
{
    async move {
        let adapter = client.read_adapter(matches!(read_mode, ReadMode::Primary));
        let (sql, params) = adapter.dialect().build_count(&statement);
        let mut rows = adapter.query_as::<DinocoCountRow>(&sql, &params).await?;
        let count = rows.drain(..).next().map(|row| row.count).unwrap_or_default();

        usize::try_from(count).map_err(|_| DinocoError::ParseError(format!("Expected non-negative count, got {count}")))
    }
}

pub fn execute_insert<'a, M, A>(
    items: Vec<M>,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<()>> + 'a
where
    M: InsertModel + 'a,
    A: DinocoAdapter,
{
    async move {
        if items.is_empty() {
            return Ok(());
        }

        for item in &items {
            item.validate_insert()?;
        }

        let statement = InsertStatement::new()
            .into(M::table_name())
            .columns(M::insert_columns())
            .values(items.into_iter().map(M::into_insert_row).collect());

        let adapter = client.primary();
        let (sql, params) = adapter.dialect().build_insert(&statement);

        adapter.execute(&sql, &params).await
    }
}

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

pub fn execute_update<'a, A>(
    statement: UpdateStatement,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<()>> + 'a
where
    A: DinocoAdapter,
{
    async move {
        let adapter = client.primary();
        let (sql, params) = adapter.dialect().build_update(&statement);

        adapter.execute(&sql, &params).await
    }
}

pub fn execute_update_many<'a, M, A>(
    items: Vec<M>,
    conditions: Vec<dinoco_engine::Expression>,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<()>> + 'a
where
    M: UpdateModel + 'a,
    A: DinocoAdapter,
{
    async move {
        for item in &items {
            item.validate_update()?;
        }

        if items.is_empty() {
            return Ok(());
        }

        let mut statement = UpdateStatement::new().table(M::table_name());

        for item in items {
            let mut batch_conditions = item.update_identity_conditions();
            batch_conditions.extend(conditions.clone());

            statement = statement.batch(dinoco_engine::UpdateBatchItem {
                conditions: batch_conditions,
                values: M::update_columns()
                    .iter()
                    .copied()
                    .zip(item.into_update_row().into_iter())
                    .map(|(column, value)| (column.to_string(), value))
                    .collect(),
            });
        }

        execute_update(statement, client).await
    }
}

pub fn execute_relation_writes<'a, A>(
    table_name: &'static str,
    conditions: Vec<dinoco_engine::Expression>,
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
                            dinoco_engine::Expression::Column(plan.source_join_column.to_string())
                                .in_values(source_ids.clone()),
                        )
                        .condition(
                            dinoco_engine::Expression::Column(plan.target_join_column.to_string())
                                .in_values(target_ids),
                        );
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

struct GroupedRelationLinks {
    table_name: &'static str,
    columns: &'static [&'static str],
    rows: Vec<Vec<dinoco_engine::DinocoValue>>,
}

fn group_relation_links(relation_links: Vec<RelationLinkPlan>) -> Vec<GroupedRelationLinks> {
    let mut groups = Vec::<GroupedRelationLinks>::new();

    for link in relation_links {
        if let Some(group) =
            groups.iter_mut().find(|group| group.table_name == link.table_name && group.columns == link.columns)
        {
            group.rows.push(link.row);
            continue;
        }

        groups.push(GroupedRelationLinks { table_name: link.table_name, columns: link.columns, rows: vec![link.row] });
    }

    groups
}

async fn query_ids<A>(
    adapter: &A,
    table_name: &str,
    select_column: Option<&str>,
    conditions: Vec<dinoco_engine::Expression>,
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

async fn query_pairs<A>(
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
        .condition(dinoco_engine::Expression::Column(left_column.to_string()).in_values(left_values))
        .condition(dinoco_engine::Expression::Column(right_column.to_string()).in_values(right_values));
    let (sql, params) = adapter.dialect().build_select(&statement);
    let rows = adapter.query_as::<DinocoPairRow>(&sql, &params).await?;

    Ok(rows.into_iter().map(|row| (row.left, row.right)).collect())
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
