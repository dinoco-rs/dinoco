use std::future::Future;

use dinoco_engine::{
    DinocoAdapter, DinocoClient, DinocoError, DinocoResult, QueryBuilder, SelectStatement, UpdateStatement,
};

use crate::{FieldUpdate, FindAndUpdateModel, Projection, ReadMode, UpdateModel, UpdatePayload};

use super::lookup::query_first_id;
use super::read::execute_first;
use super::reload::load_many_by_conditions;

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

pub fn execute_update_fields<'a, M, A>(
    conditions: Vec<dinoco_engine::Expression>,
    updates: Vec<FieldUpdate>,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<()>> + 'a
where
    M: UpdateModel + 'a,
    A: DinocoAdapter,
{
    async move {
        if updates.is_empty() {
            return Err(DinocoError::ParseError("update() requires at least one update().".to_string()));
        }

        let mut statement = UpdateStatement::new().table(M::table_name());

        for condition in conditions {
            statement = statement.condition(condition);
        }

        for update in updates {
            statement = apply_field_update(statement, update);
        }

        execute_update(statement, client).await.map(|_| ())
    }
}

pub fn execute_update_fields_returning<'a, M, S, A>(
    conditions: Vec<dinoco_engine::Expression>,
    updates: Vec<FieldUpdate>,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<Vec<S>>> + 'a
where
    M: UpdateModel + Projection<M> + 'a,
    S: Projection<M> + 'a,
    A: DinocoAdapter,
{
    async move {
        if updates.is_empty() {
            return Err(DinocoError::ParseError("update() requires at least one update().".to_string()));
        }

        let mut before_statement = SelectStatement::new().from(M::table_name()).select(M::columns());

        for condition in &conditions {
            before_statement = before_statement.condition(condition.clone());
        }

        let matched =
            super::read::execute_many::<M, M, A>(before_statement, &[], &[], ReadMode::Primary, client).await?;

        execute_update_fields::<M, A>(conditions, updates, client).await?;

        let identity_conditions = matched.iter().map(UpdateModel::update_identity_conditions).collect::<Vec<_>>();

        load_many_by_conditions::<M, S, A>(identity_conditions, client).await
    }
}

fn apply_field_update(statement: UpdateStatement, update: FieldUpdate) -> UpdateStatement {
    match update.operation {
        dinoco_engine::UpdateOperation::Set(value) => statement.set(update.column, value),
        dinoco_engine::UpdateOperation::Increment(value) => statement.increment(update.column, value),
        dinoco_engine::UpdateOperation::Decrement(value) => statement.decrement(update.column, value),
        dinoco_engine::UpdateOperation::Multiply(value) => statement.multiply(update.column, value),
        dinoco_engine::UpdateOperation::Division(value) => statement.division(update.column, value),
    }
}

pub fn execute_update_many<'a, M, V, A>(
    items: Vec<V>,
    conditions: Vec<dinoco_engine::Expression>,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<()>> + 'a
where
    M: UpdateModel + 'a,
    V: UpdatePayload<M> + 'a,
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

pub fn execute_update_returning<'a, M, V, S, A>(
    conditions: Vec<dinoco_engine::Expression>,
    item: V,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<Vec<S>>> + 'a
where
    M: UpdateModel + Projection<M> + 'a,
    V: UpdatePayload<M> + 'a,
    S: Projection<M> + 'a,
    A: DinocoAdapter,
{
    async move {
        item.validate_update()?;

        let mut before_statement = SelectStatement::new().from(M::table_name()).select(M::columns());

        for condition in conditions.clone() {
            before_statement = before_statement.condition(condition);
        }

        let matched =
            super::read::execute_many::<M, M, A>(before_statement, &[], &[], ReadMode::Primary, client).await?;

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

pub fn execute_update_many_returning<'a, M, V, S, A>(
    items: Vec<V>,
    conditions: Vec<dinoco_engine::Expression>,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<Vec<S>>> + 'a
where
    M: UpdateModel + 'a,
    V: UpdatePayload<M> + 'a,
    S: Projection<M> + 'a,
    A: DinocoAdapter,
{
    async move {
        let identity_conditions = items.iter().map(UpdatePayload::update_identity_conditions).collect::<Vec<_>>();

        execute_update_many::<M, V, A>(items, conditions, client).await?;
        load_many_by_conditions::<M, S, A>(identity_conditions, client).await
    }
}

pub fn execute_find_and_update<'a, M, A>(
    conditions: Vec<dinoco_engine::Expression>,
    updates: Vec<FieldUpdate>,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<M>> + 'a
where
    M: FindAndUpdateModel + 'a,
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
        let target_id = query_first_id(adapter, M::table_name(), primary_key, &conditions).await?;
        let Some(target_id) = target_id else {
            return Err(DinocoError::RecordNotFound(format!(
                "No record matched the condition for table '{}'.",
                M::table_name()
            )));
        };

        let mut statement = UpdateStatement::new().table(M::table_name()).target_first_match(primary_keys);

        for condition in conditions {
            statement = statement.condition(condition);
        }

        for update in updates {
            statement = apply_field_update(statement, update);
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
