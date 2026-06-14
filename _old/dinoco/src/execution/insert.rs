use std::future::Future;

use dinoco_engine::{
    AdapterDialect, DinocoAdapter, DinocoClient, DinocoError, DinocoResult, InsertStatement, QueryBuilder,
};

use crate::{InsertConnectionPayload, InsertModel, InsertNested, InsertPayload, InsertRelation, Projection};

use super::relation::{execute_connection_updates, execute_insert_relation_links};
use super::reload::{execute_reload_many_by_identity, load_many_by_conditions};

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
