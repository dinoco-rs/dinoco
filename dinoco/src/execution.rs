use std::future::Future;

use dinoco_engine::{
    AdapterDialect, DeleteStatement, DinocoAdapter, DinocoClient, DinocoError, DinocoGenericRow, DinocoResult,
    DinocoRow, Expression, InsertStatement, QueryBuilder, SelectStatement, UpdateStatement,
};

use crate::{
    ConnectionUpdatePlan, FieldUpdate, InsertConnectionPayload, InsertModel, InsertNested, InsertPayload,
    InsertRelation, Model, Projection, ReadMode, RelationLinkPlan, RelationWriteAction, RelationWritePlan, UpdateModel,
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

fn should_qualify_query_column(value: &str) -> bool {
    !value.is_empty()
        && value != "*"
        && !value.contains('.')
        && !value.contains(' ')
        && !value.contains('(')
        && !value.contains(')')
        && !value.contains(',')
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

pub fn qualify_select_statement(mut statement: SelectStatement, table_name: &str) -> SelectStatement {
    statement.select = statement.select.into_iter().map(|column| qualify_query_column(&column, table_name)).collect();
    statement.conditions =
        statement.conditions.into_iter().map(|expression| qualify_expression(expression, table_name)).collect();
    statement.order_by = statement
        .order_by
        .into_iter()
        .map(|(column, direction)| (qualify_query_column(&column, table_name), direction))
        .collect();

    statement
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
    async move { execute_insert_result::<M, A>(items, client).await.map(|_| ()) }
}

pub fn execute_insert_returning<'a, M, S, A>(
    items: Vec<M>,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<Vec<S>>> + 'a
where
    M: InsertModel + 'a,
    S: Projection<M> + 'a,
    A: DinocoAdapter,
{
    async move {
        let adapter = client.primary();

        if M::auto_increment_primary_key_column().is_some() && adapter.dialect().supports_insert_returning() {
            if items.is_empty() {
                return Ok(Vec::new());
            }

            for item in &items {
                item.validate_insert()?;
            }

            let statement = InsertStatement::new()
                .into(M::table_name())
                .columns(M::insert_columns())
                .values(items.into_iter().map(M::into_insert_row).collect())
                .returning(S::columns());
            let (sql, params) = adapter.dialect().build_insert(&statement);

            return adapter.query_as::<S>(&sql, &params).await;
        }

        if M::auto_increment_primary_key_column().is_some() {
            let result = execute_insert_result::<M, A>(items, client).await?;
            let first_id = result.last_insert_id.ok_or_else(|| {
                DinocoError::ParseError(format!(
                    "Adapter did not return the generated autoincrement id for table '{}'.",
                    M::table_name()
                ))
            })?;
            let identity_conditions = (0..result.affected_rows)
                .map(|offset| M::auto_increment_identity_conditions(first_id + offset as i64))
                .collect::<Vec<_>>();

            return load_many_by_conditions::<M, S, A>(identity_conditions, client).await;
        }

        let identity_conditions = items.iter().map(InsertModel::insert_identity_conditions).collect::<Vec<_>>();

        execute_insert::<M, A>(items, client).await?;
        load_many_by_conditions::<M, S, A>(identity_conditions, client).await
    }
}

pub fn execute_insert_payload<'a, M, V, A>(
    items: Vec<V>,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<()>> + 'a
where
    M: InsertModel + Projection<M> + 'a,
    V: InsertPayload<M> + 'a,
    A: DinocoAdapter,
{
    async move {
        execute_insert_payload_models::<M, V, A>(items, client).await?;

        Ok(())
    }
}

pub fn execute_insert_payload_returning<'a, M, V, S, A>(
    items: Vec<V>,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<Vec<S>>> + 'a
where
    M: InsertModel + Projection<M> + 'a,
    V: InsertPayload<M> + 'a,
    S: Projection<M> + 'a,
    A: DinocoAdapter,
{
    async move {
        let inserted_items = execute_insert_payload_models::<M, V, A>(items, client).await?;

        execute_reload_many_by_identity::<M, S, A>(&inserted_items, client).await
    }
}

pub fn execute_insert_related_payload<'a, M, R, V, A>(
    parent: &'a M,
    related: V,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<()>> + 'a
where
    M: InsertModel + InsertRelation<R> + 'a,
    R: InsertModel + Projection<R> + 'a,
    V: InsertPayload<R> + 'a,
    A: DinocoAdapter,
{
    async move {
        let (mut related_item, nested) = related.split_insert_payload();

        parent.bind_relation(&mut related_item);

        let mut inserted_related = execute_insert_returning::<R, R, A>(vec![related_item], client).await?;
        let inserted_related = inserted_related.pop().ok_or_else(|| {
            DinocoError::RecordNotFound(format!(
                "Record from table '{}' could not be loaded after insert.",
                R::table_name()
            ))
        })?;

        execute_insert_relation_links(parent.relation_links(&inserted_related), client).await?;
        nested.execute(&inserted_related, client).await
    }
}

pub fn execute_insert_related_payloads<'a, M, R, V, A>(
    parent: &'a M,
    related_items: Vec<V>,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<()>> + 'a
where
    M: InsertModel + InsertRelation<R> + 'a,
    R: InsertModel + Projection<R> + 'a,
    V: InsertPayload<R> + 'a,
    A: DinocoAdapter,
{
    async move {
        let mut related_models = Vec::with_capacity(related_items.len());
        let mut nested_items = Vec::with_capacity(related_items.len());

        for related in related_items {
            let (mut related_item, nested) = related.split_insert_payload();

            parent.bind_relation(&mut related_item);
            related_models.push(related_item);
            nested_items.push(nested);
        }

        let inserted_related = execute_insert_returning::<R, R, A>(related_models, client).await?;
        let mut relation_links = Vec::new();

        for related_item in &inserted_related {
            relation_links.extend(parent.relation_links(related_item));
        }

        execute_insert_relation_links(relation_links, client).await?;

        for (related_item, nested) in inserted_related.iter().zip(nested_items.into_iter()) {
            nested.execute(related_item, client).await?;
        }

        Ok(())
    }
}

pub fn execute_insert_connected_payload<'a, M, V, A>(
    parent: &'a M,
    connected: V,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<()>> + 'a
where
    M: InsertModel + 'a,
    V: InsertConnectionPayload<M> + 'a,
    A: DinocoAdapter,
{
    async move {
        execute_connection_updates(connected.connection_updates(parent), client).await?;
        execute_insert_relation_links(connected.relation_links(parent), client).await
    }
}

pub fn execute_insert_connected_payloads<'a, M, V, A>(
    parent: &'a M,
    connected_items: Vec<V>,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<()>> + 'a
where
    M: InsertModel + 'a,
    V: InsertConnectionPayload<M> + 'a,
    A: DinocoAdapter,
{
    async move {
        let mut connection_updates = Vec::new();
        let mut relation_links = Vec::new();

        for connected in connected_items {
            connection_updates.extend(connected.connection_updates(parent));
            relation_links.extend(connected.relation_links(parent));
        }

        execute_connection_updates(connection_updates, client).await?;
        execute_insert_relation_links(relation_links, client).await
    }
}

async fn execute_insert_result<M, A>(
    items: Vec<M>,
    client: &DinocoClient<A>,
) -> DinocoResult<dinoco_engine::ExecutionResult>
where
    M: InsertModel,
    A: DinocoAdapter,
{
    if items.is_empty() {
        return Ok(dinoco_engine::ExecutionResult::default());
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

    adapter.execute_result(&sql, &params).await
}

async fn execute_insert_payload_models<M, V, A>(items: Vec<V>, client: &DinocoClient<A>) -> DinocoResult<Vec<M>>
where
    M: InsertModel + Projection<M>,
    V: InsertPayload<M>,
    A: DinocoAdapter,
{
    let mut base_items = Vec::with_capacity(items.len());
    let mut nested_items = Vec::with_capacity(items.len());

    for item in items {
        let (base_item, nested) = item.split_insert_payload();

        base_items.push(base_item);
        nested_items.push(nested);
    }

    let inserted_items = execute_insert_returning::<M, M, A>(base_items, client).await?;

    for (item, nested) in inserted_items.iter().zip(nested_items.into_iter()) {
        nested.execute(item, client).await?;
    }

    Ok(inserted_items)
}

pub(crate) fn execute_reload_by_identity<'a, M, S, A>(
    item: &'a M,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<S>> + 'a
where
    M: InsertModel + 'a,
    S: Projection<M> + 'a,
    A: DinocoAdapter,
{
    async move { load_one_by_conditions::<M, S, A>(item.insert_identity_conditions(), client).await }
}

pub(crate) fn execute_reload_many_by_identity<'a, M, S, A>(
    items: &'a [M],
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<Vec<S>>> + 'a
where
    M: InsertModel + 'a,
    S: Projection<M> + 'a,
    A: DinocoAdapter,
{
    async move {
        let identity_conditions = items.iter().map(InsertModel::insert_identity_conditions).collect::<Vec<_>>();

        load_many_by_conditions::<M, S, A>(identity_conditions, client).await
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
) -> impl Future<Output = DinocoResult<u64>> + 'a
where
    A: DinocoAdapter,
{
    async move {
        let adapter = client.primary();
        let (sql, params) = adapter.dialect().build_update(&statement);

        adapter.execute_result(&sql, &params).await.map(|result| result.affected_rows)
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

        execute_update(statement, client).await.map(|_| ())
    }
}

pub fn execute_update_returning<'a, M, S, A>(
    conditions: Vec<dinoco_engine::Expression>,
    item: M,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<Vec<S>>> + 'a
where
    M: UpdateModel + Projection<M> + 'a,
    S: Projection<M> + 'a,
    A: DinocoAdapter,
{
    async move {
        item.validate_update()?;

        let mut before_statement = SelectStatement::new().from(M::table_name()).select(M::columns());

        for condition in conditions.clone() {
            before_statement = before_statement.condition(condition);
        }

        let matched = execute_many::<M, M, A>(before_statement, &[], &[], ReadMode::Primary, client).await?;

        let mut statement = UpdateStatement::new().table(M::table_name());

        for (column, value) in M::update_columns().iter().copied().zip(item.into_update_row().into_iter()) {
            statement = statement.set(column, value);
        }

        for condition in conditions {
            statement = statement.condition(condition);
        }

        execute_update(statement, client).await?;

        let identity_conditions = matched.iter().map(UpdateModel::update_identity_conditions).collect::<Vec<_>>();

        load_many_by_conditions::<M, S, A>(identity_conditions, client).await
    }
}

pub fn execute_update_many_returning<'a, M, S, A>(
    items: Vec<M>,
    conditions: Vec<dinoco_engine::Expression>,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<Vec<S>>> + 'a
where
    M: UpdateModel + 'a,
    S: Projection<M> + 'a,
    A: DinocoAdapter,
{
    async move {
        let identity_conditions = items.iter().map(UpdateModel::update_identity_conditions).collect::<Vec<_>>();

        execute_update_many::<M, A>(items, conditions, client).await?;
        load_many_by_conditions::<M, S, A>(identity_conditions, client).await
    }
}

pub fn execute_find_and_update<'a, M, A>(
    conditions: Vec<dinoco_engine::Expression>,
    updates: Vec<FieldUpdate>,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<M>> + 'a
where
    M: crate::FindAndUpdateModel + 'a,
    A: DinocoAdapter,
{
    async move {
        if conditions.is_empty() {
            return Err(DinocoError::ParseError("find_and_update() requires at least one cond().".to_string()));
        }

        if updates.is_empty() {
            return Err(DinocoError::ParseError("find_and_update() requires at least one update().".to_string()));
        }

        let primary_keys = M::primary_key_columns();

        if primary_keys.len() != 1 {
            return Err(DinocoError::ParseError(
                "find_and_update() currently supports only single-column primary keys.".to_string(),
            ));
        }

        let primary_key = primary_keys[0];
        let adapter = client.primary();
        let target_id = query_first_id(adapter, M::table_name(), primary_key, conditions.clone()).await?;
        let Some(target_id) = target_id else {
            return Err(DinocoError::RecordNotFound(format!(
                "No record matched the condition for table '{}'.",
                M::table_name()
            )));
        };

        let mut statement = UpdateStatement::new().table(M::table_name()).target_first_match(primary_keys);

        for condition in conditions.clone() {
            statement = statement.condition(condition);
        }

        for update in updates {
            statement = match update.operation {
                dinoco_engine::UpdateOperation::Set(value) => statement.set(update.column, value),
                dinoco_engine::UpdateOperation::Increment(value) => statement.increment(update.column, value),
                dinoco_engine::UpdateOperation::Decrement(value) => statement.decrement(update.column, value),
                dinoco_engine::UpdateOperation::Multiply(value) => statement.multiply(update.column, value),
                dinoco_engine::UpdateOperation::Division(value) => statement.division(update.column, value),
            };
        }

        let affected_rows = execute_update(statement, client).await?;

        if affected_rows == 0 {
            return Err(DinocoError::RecordNotFound(format!(
                "No record matched the condition for table '{}'.",
                M::table_name()
            )));
        }

        let statement = SelectStatement::new()
            .from(M::table_name())
            .select(M::columns())
            .condition(dinoco_engine::Expression::Column(primary_key.to_string()).eq(target_id));

        execute_first::<M, M, A>(statement, ReadMode::Primary, client).await?.ok_or_else(|| {
            DinocoError::RecordNotFound(format!(
                "Updated record from table '{}' could not be loaded after write.",
                M::table_name()
            ))
        })
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

async fn query_first_id<A>(
    adapter: &A,
    table_name: &str,
    select_column: &str,
    conditions: Vec<dinoco_engine::Expression>,
) -> DinocoResult<Option<dinoco_engine::DinocoValue>>
where
    A: DinocoAdapter,
{
    let mut statement = SelectStatement::new().from(table_name).select(&[select_column]).limit(1);

    for condition in conditions {
        statement = statement.condition(condition);
    }

    let (sql, params) = adapter.dialect().build_select(&statement);
    let mut rows = adapter.query_as::<DinocoValueRow>(&sql, &params).await?;

    Ok(rows.drain(..).next().map(|row| row.value))
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

async fn load_many_by_conditions<M, S, A>(
    identity_conditions: Vec<Vec<dinoco_engine::Expression>>,
    client: &DinocoClient<A>,
) -> DinocoResult<Vec<S>>
where
    M: Model,
    S: Projection<M>,
    A: DinocoAdapter,
{
    let mut rows = Vec::with_capacity(identity_conditions.len());

    for conditions in identity_conditions {
        let item = load_one_by_conditions::<M, S, A>(conditions, client).await?;
        rows.push(item);
    }

    Ok(rows)
}

async fn load_one_by_conditions<M, S, A>(
    conditions: Vec<dinoco_engine::Expression>,
    client: &DinocoClient<A>,
) -> DinocoResult<S>
where
    M: Model,
    S: Projection<M>,
    A: DinocoAdapter,
{
    let mut statement = SelectStatement::new().from(M::table_name()).select(S::columns());

    for condition in conditions {
        statement = statement.condition(condition);
    }

    execute_first::<M, S, A>(statement, ReadMode::Primary, client).await?.ok_or_else(|| {
        DinocoError::RecordNotFound(format!("Record from table '{}' could not be loaded after write.", M::table_name()))
    })
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
