use std::marker::PhantomData;

use dinoco_engine::{DinocoAdapter, DinocoResult};

use crate::{
    InsertConnection, InsertModel, Projection, execute_connection_updates, execute_insert,
    execute_insert_relation_links, execute_insert_returning, execution::execute_reload_many_by_identity,
};

use super::{
    InsertManyWithConnection, InsertManyWithConnectionReturning, InsertManyWithConnections,
    InsertManyWithConnectionsReturning, validate_pair_len,
};

impl<M, R> InsertManyWithConnections<M, R>
where
    M: InsertModel + InsertConnection<R> + Projection<M>,
{
    pub fn returning(self) -> InsertManyWithConnectionsReturning<M, R, M> {
        self.returning_as::<M>()
    }

    pub fn returning_as<S>(self) -> InsertManyWithConnectionsReturning<M, R, S>
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
        client: &'a crate::DinocoClient<A>,
    ) -> impl std::future::Future<Output = DinocoResult<()>> + Send + 'a
    where
        M: Send + Sync + 'a,
        R: Send + Sync + 'a,
        A: DinocoAdapter,
    {
        async move {
            let items = self.items;
            let connected_groups = self.connected_groups;
            let mut connection_updates = Vec::new();
            let mut relation_links = Vec::new();

            validate_pair_len(items.len(), connected_groups.len(), "connection group")?;

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
    pub fn returning(self) -> InsertManyWithConnectionReturning<M, R, M> {
        self.returning_as::<M>()
    }

    pub fn returning_as<S>(self) -> InsertManyWithConnectionReturning<M, R, S>
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
        client: &'a crate::DinocoClient<A>,
    ) -> impl std::future::Future<Output = DinocoResult<()>> + Send + 'a
    where
        M: Send + Sync + 'a,
        R: Send + Sync + 'a,
        A: DinocoAdapter,
    {
        async move {
            let items = self.items;
            let connected_items = self.connected_items;
            let mut connection_updates = Vec::new();
            let mut relation_links = Vec::new();

            validate_pair_len(items.len(), connected_items.len(), "connection size")?;

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
        client: &'a crate::DinocoClient<A>,
    ) -> impl std::future::Future<Output = DinocoResult<Vec<S>>> + Send + 'a
    where
        M: Send + Sync + 'a,
        R: Send + Sync + 'a,
        S: Send + Sync + 'a,
        A: DinocoAdapter,
    {
        async move {
            let items = self.items;
            let connected_groups = self.connected_groups;
            let mut connection_updates = Vec::new();
            let mut relation_links = Vec::new();

            validate_pair_len(items.len(), connected_groups.len(), "connection group")?;

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
        client: &'a crate::DinocoClient<A>,
    ) -> impl std::future::Future<Output = DinocoResult<Vec<S>>> + Send + 'a
    where
        M: Send + Sync + 'a,
        R: Send + Sync + 'a,
        S: Send + Sync + 'a,
        A: DinocoAdapter,
    {
        async move {
            let items = self.items;
            let connected_items = self.connected_items;
            let mut connection_updates = Vec::new();
            let mut relation_links = Vec::new();

            validate_pair_len(items.len(), connected_items.len(), "connection size")?;

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
