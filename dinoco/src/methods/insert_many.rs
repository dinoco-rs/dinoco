use std::marker::PhantomData;

use dinoco_engine::{DinocoAdapter, DinocoClient};

use crate::{InsertConnection, InsertModel, InsertRelation};
use crate::{execute_connection_updates, execute_insert, execute_insert_relation_links};

#[derive(Debug, Clone)]
pub struct InsertMany<M> {
    items: Vec<M>,
    marker: PhantomData<fn() -> M>,
}

#[derive(Debug, Clone)]
pub struct InsertManyWithRelation<M, R> {
    items: Vec<M>,
    related_items: Vec<R>,
}

#[derive(Debug, Clone)]
pub struct InsertManyWithRelations<M, R> {
    items: Vec<M>,
    related_groups: Vec<Vec<R>>,
}

#[derive(Debug, Clone)]
pub struct InsertManyWithConnections<M, R> {
    items: Vec<M>,
    connected_groups: Vec<Vec<R>>,
}

pub fn insert_many<M>() -> InsertMany<M>
where
    M: InsertModel,
{
    InsertMany { items: Vec::new(), marker: PhantomData }
}

impl<M> InsertMany<M>
where
    M: InsertModel,
{
    pub fn values(mut self, items: Vec<M>) -> Self {
        self.items = items;

        self
    }

    pub fn with_relation<R>(self, related_items: Vec<R>) -> InsertManyWithRelation<M, R>
    where
        M: InsertRelation<R>,
        R: InsertModel,
    {
        InsertManyWithRelation { items: self.items, related_items }
    }

    pub fn with_relations<R>(self, related_groups: Vec<Vec<R>>) -> InsertManyWithRelations<M, R>
    where
        M: InsertRelation<R>,
        R: InsertModel,
    {
        InsertManyWithRelations { items: self.items, related_groups }
    }

    pub fn with_connections<R>(self, connected_groups: Vec<Vec<R>>) -> InsertManyWithConnections<M, R>
    where
        M: InsertConnection<R>,
    {
        InsertManyWithConnections { items: self.items, connected_groups }
    }

    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<()>> + 'a
    where
        M: 'a,
        A: DinocoAdapter,
    {
        async move { execute_insert::<M, A>(self.items, client).await }
    }
}

impl<M, R> InsertManyWithRelation<M, R>
where
    M: InsertModel + InsertRelation<R>,
    R: InsertModel,
{
    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<()>> + 'a
    where
        M: 'a,
        R: 'a,
        A: DinocoAdapter,
    {
        async move {
            let items = self.items;
            let mut related_items = self.related_items;
            let mut relation_links = Vec::new();

            if items.len() != related_items.len() {
                return Err(dinoco_engine::DinocoError::ParseError(format!(
                    "insert_many relation size mismatch: {} parent items for {} related items",
                    items.len(),
                    related_items.len()
                )));
            }

            for (item, related_item) in items.iter().zip(related_items.iter_mut()) {
                item.bind_relation(related_item);
                relation_links.extend(item.relation_links(related_item));
            }

            execute_insert::<M, A>(items, client).await?;
            execute_insert::<R, A>(related_items, client).await?;
            execute_insert_relation_links(relation_links, client).await
        }
    }
}

impl<M, R> InsertManyWithRelations<M, R>
where
    M: InsertModel + InsertRelation<R>,
    R: InsertModel,
{
    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<()>> + 'a
    where
        M: 'a,
        R: 'a,
        A: DinocoAdapter,
    {
        async move {
            let items = self.items;
            let related_groups = self.related_groups;
            let mut relation_links = Vec::new();

            if items.len() != related_groups.len() {
                return Err(dinoco_engine::DinocoError::ParseError(format!(
                    "insert_many relation group mismatch: {} parent items for {} relation groups",
                    items.len(),
                    related_groups.len()
                )));
            }

            let mut related_items = Vec::new();

            for (item, group) in items.iter().zip(related_groups.into_iter()) {
                for mut related_item in group {
                    item.bind_relation(&mut related_item);
                    relation_links.extend(item.relation_links(&related_item));
                    related_items.push(related_item);
                }
            }

            execute_insert::<M, A>(items, client).await?;
            execute_insert::<R, A>(related_items, client).await?;
            execute_insert_relation_links(relation_links, client).await
        }
    }
}

impl<M, R> InsertManyWithConnections<M, R>
where
    M: InsertModel + InsertConnection<R>,
{
    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<()>> + 'a
    where
        M: 'a,
        R: 'a,
        A: DinocoAdapter,
    {
        async move {
            let items = self.items;
            let connected_groups = self.connected_groups;
            let mut connection_updates = Vec::new();
            let mut relation_links = Vec::new();

            if items.len() != connected_groups.len() {
                return Err(dinoco_engine::DinocoError::ParseError(format!(
                    "insert_many connection group mismatch: {} parent items for {} connection groups",
                    items.len(),
                    connected_groups.len()
                )));
            }

            for (item, group) in items.iter().zip(connected_groups.into_iter()) {
                for connected in group {
                    connection_updates.extend(item.connection_updates(&connected));
                    relation_links.extend(item.connection_links(&connected));
                }
            }

            execute_insert::<M, A>(items, client).await?;
            execute_connection_updates(connection_updates, client).await?;
            execute_insert_relation_links(relation_links, client).await
        }
    }
}
