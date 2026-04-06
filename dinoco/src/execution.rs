use std::future::Future;

use dinoco_engine::{
    DeleteStatement, DinocoAdapter, DinocoClient, DinocoResult, InsertStatement, QueryBuilder, SelectStatement,
    UpdateStatement,
};

use crate::{InsertModel, Model, Projection, ReadMode, UpdateModel};

pub fn execute_many<'a, M, S, A>(
    statement: SelectStatement,
    includes: &'a [crate::IncludeNode],
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

        let mut rows = execute_many::<M, S, A>(statement, &[], read_mode, client).await?;

        Ok(rows.drain(..).next())
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
