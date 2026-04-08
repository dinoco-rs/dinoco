use std::marker::PhantomData;

use dinoco_engine::{DinocoAdapter, DinocoClient};

use crate::execution::execute_reload_many_by_identity;
use crate::{InsertConnection, InsertModel, InsertRelation, Projection};
use crate::{execute_connection_updates, execute_insert, execute_insert_relation_links, execute_insert_returning};

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

#[derive(Debug, Clone)]
pub struct InsertManyWithConnection<M, R> {
    items: Vec<M>,
    connected_items: Vec<R>,
}

#[derive(Debug, Clone)]
pub struct InsertManyReturning<M, S> {
    items: Vec<M>,
    marker: PhantomData<fn() -> (M, S)>,
}

#[derive(Debug, Clone)]
pub struct InsertManyWithRelationReturning<M, R, S> {
    items: Vec<M>,
    related_items: Vec<R>,
    marker: PhantomData<fn() -> S>,
}

#[derive(Debug, Clone)]
pub struct InsertManyWithRelationsReturning<M, R, S> {
    items: Vec<M>,
    related_groups: Vec<Vec<R>>,
    marker: PhantomData<fn() -> S>,
}

#[derive(Debug, Clone)]
pub struct InsertManyWithConnectionsReturning<M, R, S> {
    items: Vec<M>,
    connected_groups: Vec<Vec<R>>,
    marker: PhantomData<fn() -> S>,
}

#[derive(Debug, Clone)]
pub struct InsertManyWithConnectionReturning<M, R, S> {
    items: Vec<M>,
    connected_items: Vec<R>,
    marker: PhantomData<fn() -> S>,
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

    pub fn with_connection<R>(self, connected_items: Vec<R>) -> InsertManyWithConnection<M, R>
    where
        M: InsertConnection<R>,
    {
        InsertManyWithConnection { items: self.items, connected_items }
    }

    pub fn returning<S>(self) -> InsertManyReturning<M, S>
    where
        S: Projection<M>,
    {
        InsertManyReturning { items: self.items, marker: PhantomData }
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

impl<M, S> InsertManyReturning<M, S>
where
    M: InsertModel,
    S: Projection<M>,
{
    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<Vec<S>>> + 'a
    where
        M: 'a,
        S: 'a,
        A: DinocoAdapter,
    {
        async move { execute_insert_returning::<M, S, A>(self.items, client).await }
    }
}

impl<M, R> InsertManyWithRelation<M, R>
where
    M: InsertModel + InsertRelation<R> + Projection<M> + Clone,
    R: InsertModel + Projection<R>,
{
    pub fn returning<S>(self) -> InsertManyWithRelationReturning<M, R, S>
    where
        S: Projection<M>,
    {
        InsertManyWithRelationReturning { items: self.items, related_items: self.related_items, marker: PhantomData }
    }

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
            let parent_auto_increment = M::auto_increment_primary_key_column().is_some();
            let related_auto_increment = R::auto_increment_primary_key_column().is_some();

            if items.len() != related_items.len() {
                return Err(dinoco_engine::DinocoError::ParseError(format!(
                    "insert_many relation size mismatch: {} parent items for {} related items",
                    items.len(),
                    related_items.len()
                )));
            }

            let parent_items = if parent_auto_increment {
                let inserted = execute_insert_returning::<M, M, A>(items, client).await?;

                for (item, related_item) in inserted.iter().zip(related_items.iter_mut()) {
                    item.bind_relation(related_item);
                }

                inserted
            } else {
                for (item, related_item) in items.iter().zip(related_items.iter_mut()) {
                    item.bind_relation(related_item);
                }

                execute_insert::<M, A>(items.clone(), client).await?;
                items
            };

            if related_auto_increment {
                let inserted_related = execute_insert_returning::<R, R, A>(related_items, client).await?;
                let mut relation_links = Vec::new();

                for (item, related_item) in parent_items.iter().zip(inserted_related.iter()) {
                    relation_links.extend(item.relation_links(related_item));
                }

                execute_insert_relation_links(relation_links, client).await
            } else {
                let mut relation_links = Vec::new();

                for (item, related_item) in parent_items.iter().zip(related_items.iter()) {
                    relation_links.extend(item.relation_links(related_item));
                }

                execute_insert::<R, A>(related_items, client).await?;
                execute_insert_relation_links(relation_links, client).await
            }
        }
    }
}

impl<M, R, S> InsertManyWithRelationReturning<M, R, S>
where
    M: InsertModel + InsertRelation<R> + Projection<M> + Clone,
    R: InsertModel + Projection<R>,
    S: Projection<M>,
{
    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<Vec<S>>> + 'a
    where
        M: 'a,
        R: 'a,
        S: 'a,
        A: DinocoAdapter,
    {
        async move {
            let items = self.items;
            let mut related_items = self.related_items;
            let parent_auto_increment = M::auto_increment_primary_key_column().is_some();
            let related_auto_increment = R::auto_increment_primary_key_column().is_some();

            if items.len() != related_items.len() {
                return Err(dinoco_engine::DinocoError::ParseError(format!(
                    "insert_many relation size mismatch: {} parent items for {} related items",
                    items.len(),
                    related_items.len()
                )));
            }

            let parent_items = if parent_auto_increment {
                let inserted = execute_insert_returning::<M, M, A>(items, client).await?;

                for (item, related_item) in inserted.iter().zip(related_items.iter_mut()) {
                    item.bind_relation(related_item);
                }

                inserted
            } else {
                for (item, related_item) in items.iter().zip(related_items.iter_mut()) {
                    item.bind_relation(related_item);
                }

                execute_insert::<M, A>(items.clone(), client).await?;
                items
            };

            if related_auto_increment {
                let inserted_related = execute_insert_returning::<R, R, A>(related_items, client).await?;
                let mut relation_links = Vec::new();

                for (item, related_item) in parent_items.iter().zip(inserted_related.iter()) {
                    relation_links.extend(item.relation_links(related_item));
                }

                execute_insert_relation_links(relation_links, client).await?;
            } else {
                let mut relation_links = Vec::new();

                for (item, related_item) in parent_items.iter().zip(related_items.iter()) {
                    relation_links.extend(item.relation_links(related_item));
                }

                execute_insert::<R, A>(related_items, client).await?;
                execute_insert_relation_links(relation_links, client).await?;
            }

            execute_reload_many_by_identity::<M, S, A>(&parent_items, client).await
        }
    }
}

impl<M, R> InsertManyWithRelations<M, R>
where
    M: InsertModel + InsertRelation<R> + Projection<M> + Clone,
    R: InsertModel + Projection<R> + Clone,
{
    pub fn returning<S>(self) -> InsertManyWithRelationsReturning<M, R, S>
    where
        S: Projection<M>,
    {
        InsertManyWithRelationsReturning { items: self.items, related_groups: self.related_groups, marker: PhantomData }
    }

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
            let parent_auto_increment = M::auto_increment_primary_key_column().is_some();
            let related_auto_increment = R::auto_increment_primary_key_column().is_some();

            if items.len() != related_groups.len() {
                return Err(dinoco_engine::DinocoError::ParseError(format!(
                    "insert_many relation group mismatch: {} parent items for {} relation groups",
                    items.len(),
                    related_groups.len()
                )));
            }

            let mut related_items = Vec::new();
            let mut parent_indexes = Vec::new();

            let parent_items = if parent_auto_increment {
                let inserted = execute_insert_returning::<M, M, A>(items, client).await?;

                for (index, (item, group)) in inserted.iter().zip(related_groups.into_iter()).enumerate() {
                    for mut related_item in group {
                        item.bind_relation(&mut related_item);
                        parent_indexes.push(index);
                        related_items.push(related_item);
                    }
                }

                inserted
            } else {
                for (index, (item, group)) in items.iter().zip(related_groups.iter()).enumerate() {
                    for related_item in group {
                        let mut related_item = related_item.clone();

                        item.bind_relation(&mut related_item);
                        parent_indexes.push(index);
                        related_items.push(related_item);
                    }
                }

                execute_insert::<M, A>(items.clone(), client).await?;
                items
            };

            if related_auto_increment {
                let inserted_related = execute_insert_returning::<R, R, A>(related_items, client).await?;
                let mut relation_links = Vec::new();

                for (parent_index, related_item) in parent_indexes.into_iter().zip(inserted_related.iter()) {
                    relation_links.extend(parent_items[parent_index].relation_links(related_item));
                }

                execute_insert_relation_links(relation_links, client).await
            } else {
                let mut relation_links = Vec::new();

                for (parent_index, related_item) in parent_indexes.into_iter().zip(related_items.iter()) {
                    relation_links.extend(parent_items[parent_index].relation_links(related_item));
                }

                execute_insert::<R, A>(related_items, client).await?;
                execute_insert_relation_links(relation_links, client).await
            }
        }
    }
}

impl<M, R, S> InsertManyWithRelationsReturning<M, R, S>
where
    M: InsertModel + InsertRelation<R> + Projection<M> + Clone,
    R: InsertModel + Projection<R> + Clone,
    S: Projection<M>,
{
    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<Vec<S>>> + 'a
    where
        M: 'a,
        R: 'a,
        S: 'a,
        A: DinocoAdapter,
    {
        async move {
            let items = self.items;
            let related_groups = self.related_groups;
            let parent_auto_increment = M::auto_increment_primary_key_column().is_some();
            let related_auto_increment = R::auto_increment_primary_key_column().is_some();

            if items.len() != related_groups.len() {
                return Err(dinoco_engine::DinocoError::ParseError(format!(
                    "insert_many relation group mismatch: {} parent items for {} relation groups",
                    items.len(),
                    related_groups.len()
                )));
            }

            let mut related_items = Vec::new();
            let mut parent_indexes = Vec::new();

            let parent_items = if parent_auto_increment {
                let inserted = execute_insert_returning::<M, M, A>(items, client).await?;

                for (index, (item, group)) in inserted.iter().zip(related_groups.into_iter()).enumerate() {
                    for mut related_item in group {
                        item.bind_relation(&mut related_item);
                        parent_indexes.push(index);
                        related_items.push(related_item);
                    }
                }

                inserted
            } else {
                for (index, (item, group)) in items.iter().zip(related_groups.iter()).enumerate() {
                    for related_item in group {
                        let mut related_item = related_item.clone();

                        item.bind_relation(&mut related_item);
                        parent_indexes.push(index);
                        related_items.push(related_item);
                    }
                }

                execute_insert::<M, A>(items.clone(), client).await?;
                items
            };

            if related_auto_increment {
                let inserted_related = execute_insert_returning::<R, R, A>(related_items, client).await?;
                let mut relation_links = Vec::new();

                for (parent_index, related_item) in parent_indexes.into_iter().zip(inserted_related.iter()) {
                    relation_links.extend(parent_items[parent_index].relation_links(related_item));
                }

                execute_insert_relation_links(relation_links, client).await?;
            } else {
                let mut relation_links = Vec::new();

                for (parent_index, related_item) in parent_indexes.into_iter().zip(related_items.iter()) {
                    relation_links.extend(parent_items[parent_index].relation_links(related_item));
                }

                execute_insert::<R, A>(related_items, client).await?;
                execute_insert_relation_links(relation_links, client).await?;
            }

            execute_reload_many_by_identity::<M, S, A>(&parent_items, client).await
        }
    }
}

impl<M, R> InsertManyWithConnections<M, R>
where
    M: InsertModel + InsertConnection<R> + Projection<M>,
{
    pub fn returning<S>(self) -> InsertManyWithConnectionsReturning<M, R, S>
    where
        S: Projection<M>,
    {
        InsertManyWithConnectionsReturning {
            items: self.items,
            connected_groups: self.connected_groups,
            marker: PhantomData,
        }
    }

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

            if M::auto_increment_primary_key_column().is_some() {
                let inserted_items = execute_insert_returning::<M, M, A>(items, client).await?;

                for (item, group) in inserted_items.iter().zip(connected_groups.into_iter()) {
                    for connected in group {
                        connection_updates.extend(item.connection_updates(&connected));
                        relation_links.extend(item.connection_links(&connected));
                    }
                }
            } else {
                for (item, group) in items.iter().zip(connected_groups.iter()) {
                    for connected in group {
                        connection_updates.extend(item.connection_updates(connected));
                        relation_links.extend(item.connection_links(connected));
                    }
                }

                execute_insert::<M, A>(items, client).await?;
            }

            execute_connection_updates(connection_updates, client).await?;
            execute_insert_relation_links(relation_links, client).await
        }
    }
}

impl<M, R> InsertManyWithConnection<M, R>
where
    M: InsertModel + InsertConnection<R> + Projection<M>,
{
    pub fn returning<S>(self) -> InsertManyWithConnectionReturning<M, R, S>
    where
        S: Projection<M>,
    {
        InsertManyWithConnectionReturning {
            items: self.items,
            connected_items: self.connected_items,
            marker: PhantomData,
        }
    }

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
            let connected_items = self.connected_items;
            let mut connection_updates = Vec::new();
            let mut relation_links = Vec::new();

            if items.len() != connected_items.len() {
                return Err(dinoco_engine::DinocoError::ParseError(format!(
                    "insert_many connection size mismatch: {} parent items for {} connected items",
                    items.len(),
                    connected_items.len()
                )));
            }

            if M::auto_increment_primary_key_column().is_some() {
                let inserted_items = execute_insert_returning::<M, M, A>(items, client).await?;

                for (item, connected) in inserted_items.iter().zip(connected_items.iter()) {
                    connection_updates.extend(item.connection_updates(connected));
                    relation_links.extend(item.connection_links(connected));
                }
            } else {
                for (item, connected) in items.iter().zip(connected_items.iter()) {
                    connection_updates.extend(item.connection_updates(connected));
                    relation_links.extend(item.connection_links(connected));
                }

                execute_insert::<M, A>(items, client).await?;
            }

            execute_connection_updates(connection_updates, client).await?;
            execute_insert_relation_links(relation_links, client).await
        }
    }
}

impl<M, R, S> InsertManyWithConnectionsReturning<M, R, S>
where
    M: InsertModel + InsertConnection<R> + Projection<M> + Clone,
    S: Projection<M>,
{
    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<Vec<S>>> + 'a
    where
        M: 'a,
        R: 'a,
        S: 'a,
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

            let parent_items = if M::auto_increment_primary_key_column().is_some() {
                let inserted = execute_insert_returning::<M, M, A>(items, client).await?;

                for (item, group) in inserted.iter().zip(connected_groups.into_iter()) {
                    for connected in group {
                        connection_updates.extend(item.connection_updates(&connected));
                        relation_links.extend(item.connection_links(&connected));
                    }
                }

                inserted
            } else {
                for (item, group) in items.iter().zip(connected_groups.iter()) {
                    for connected in group {
                        connection_updates.extend(item.connection_updates(connected));
                        relation_links.extend(item.connection_links(connected));
                    }
                }

                execute_insert::<M, A>(items.clone(), client).await?;
                items
            };

            execute_connection_updates(connection_updates, client).await?;
            execute_insert_relation_links(relation_links, client).await?;

            execute_reload_many_by_identity::<M, S, A>(&parent_items, client).await
        }
    }
}

impl<M, R, S> InsertManyWithConnectionReturning<M, R, S>
where
    M: InsertModel + InsertConnection<R> + Projection<M> + Clone,
    S: Projection<M>,
{
    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<Vec<S>>> + 'a
    where
        M: 'a,
        R: 'a,
        S: 'a,
        A: DinocoAdapter,
    {
        async move {
            let items = self.items;
            let connected_items = self.connected_items;
            let mut connection_updates = Vec::new();
            let mut relation_links = Vec::new();

            if items.len() != connected_items.len() {
                return Err(dinoco_engine::DinocoError::ParseError(format!(
                    "insert_many connection size mismatch: {} parent items for {} connected items",
                    items.len(),
                    connected_items.len()
                )));
            }

            let parent_items = if M::auto_increment_primary_key_column().is_some() {
                let inserted = execute_insert_returning::<M, M, A>(items, client).await?;

                for (item, connected) in inserted.iter().zip(connected_items.iter()) {
                    connection_updates.extend(item.connection_updates(connected));
                    relation_links.extend(item.connection_links(connected));
                }

                inserted
            } else {
                for (item, connected) in items.iter().zip(connected_items.iter()) {
                    connection_updates.extend(item.connection_updates(connected));
                    relation_links.extend(item.connection_links(connected));
                }

                execute_insert::<M, A>(items.clone(), client).await?;
                items
            };

            execute_connection_updates(connection_updates, client).await?;
            execute_insert_relation_links(relation_links, client).await?;

            execute_reload_many_by_identity::<M, S, A>(&parent_items, client).await
        }
    }
}
