use std::marker::PhantomData;

use dinoco_engine::{DinocoAdapter, DinocoClient};

use crate::execution::execute_reload_by_identity;
use crate::{InsertConnection, InsertModel, InsertRelation, Projection};
use crate::{execute_connection_updates, execute_insert, execute_insert_relation_links, execute_insert_returning};

#[derive(Debug, Clone)]
pub struct Insert<M> {
    item: Option<M>,
    marker: PhantomData<fn() -> M>,
}

#[derive(Debug, Clone)]
pub struct InsertWithRelation<M, R> {
    item: M,
    related: R,
}

#[derive(Debug, Clone)]
pub struct InsertWithConnection<M, R> {
    item: M,
    connected: R,
}

#[derive(Debug, Clone)]
pub struct InsertReturning<M, S> {
    item: Option<M>,
    marker: PhantomData<fn() -> (M, S)>,
}

#[derive(Debug, Clone)]
pub struct InsertWithRelationReturning<M, R, S> {
    item: M,
    related: R,
    marker: PhantomData<fn() -> S>,
}

#[derive(Debug, Clone)]
pub struct InsertWithConnectionReturning<M, R, S> {
    item: M,
    connected: R,
    marker: PhantomData<fn() -> S>,
}

pub fn insert_into<M>() -> Insert<M>
where
    M: InsertModel,
{
    Insert { item: None, marker: PhantomData }
}

impl<M> Insert<M>
where
    M: InsertModel,
{
    pub fn values(mut self, item: M) -> Self {
        self.item = Some(item);

        self
    }

    pub fn with_relation<R>(self, related: R) -> InsertWithRelation<M, R>
    where
        M: InsertRelation<R>,
    {
        InsertWithRelation {
            item: self.item.expect("insert_into().values(...) must be called before with_relation()"),
            related,
        }
    }

    pub fn with_connection<R>(self, connected: R) -> InsertWithConnection<M, R>
    where
        M: InsertConnection<R>,
    {
        InsertWithConnection {
            item: self.item.expect("insert_into().values(...) must be called before with_connection()"),
            connected,
        }
    }

    pub fn returning<S>(self) -> InsertReturning<M, S>
    where
        S: Projection<M>,
    {
        InsertReturning { item: self.item, marker: PhantomData }
    }

    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<()>> + 'a
    where
        M: 'a,
        A: DinocoAdapter,
    {
        async move {
            let item = self.item.expect("insert_into().values(...) must be called before execute()");

            execute_insert::<M, A>(vec![item], client).await
        }
    }
}

impl<M, S> InsertReturning<M, S>
where
    M: InsertModel,
    S: Projection<M>,
{
    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<S>> + 'a
    where
        M: 'a,
        S: 'a,
        A: DinocoAdapter,
    {
        async move {
            let item = self.item.expect("insert_into().values(...) must be called before execute()");
            let mut rows: Vec<S> = execute_insert_returning::<M, S, A>(vec![item], client).await?;

            rows.drain(..).next().ok_or_else(|| {
                dinoco_engine::DinocoError::RecordNotFound(format!(
                    "Record from table '{}' could not be loaded after insert.",
                    M::table_name()
                ))
            })
        }
    }
}

impl<M, R> InsertWithRelation<M, R>
where
    M: InsertModel + InsertRelation<R> + Projection<M> + Clone,
    R: InsertModel + Projection<R>,
{
    pub fn returning<S>(self) -> InsertWithRelationReturning<M, R, S>
    where
        S: Projection<M>,
    {
        InsertWithRelationReturning { item: self.item, related: self.related, marker: PhantomData }
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
            let item = self.item;
            let mut related = self.related;
            let parent_auto_increment = M::auto_increment_primary_key_column().is_some();
            let related_auto_increment = R::auto_increment_primary_key_column().is_some();

            let parent_item = if parent_auto_increment {
                let mut inserted_items = execute_insert_returning::<M, M, A>(vec![item], client).await?;

                inserted_items.drain(..).next().ok_or_else(|| {
                    dinoco_engine::DinocoError::RecordNotFound(format!(
                        "Record from table '{}' could not be loaded after insert.",
                        M::table_name()
                    ))
                })?
            } else {
                item.bind_relation(&mut related);
                execute_insert::<M, A>(vec![item.clone()], client).await?;
                item
            };

            if parent_auto_increment {
                parent_item.bind_relation(&mut related);
            }

            let relation_links = if related_auto_increment {
                let mut inserted_related_rows = execute_insert_returning::<R, R, A>(vec![related], client).await?;
                let inserted_related = inserted_related_rows.drain(..).next().ok_or_else(|| {
                    dinoco_engine::DinocoError::RecordNotFound(format!(
                        "Record from table '{}' could not be loaded after insert.",
                        R::table_name()
                    ))
                })?;

                parent_item.relation_links(&inserted_related)
            } else {
                let relation_links = parent_item.relation_links(&related);

                execute_insert::<R, A>(vec![related], client).await?;

                relation_links
            };

            execute_insert_relation_links(relation_links, client).await
        }
    }
}

impl<M, R, S> InsertWithRelationReturning<M, R, S>
where
    M: InsertModel + InsertRelation<R> + Projection<M> + Clone,
    R: InsertModel + Projection<R>,
    S: Projection<M>,
{
    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<S>> + 'a
    where
        M: 'a,
        R: 'a,
        S: 'a,
        A: DinocoAdapter,
    {
        async move {
            let item = self.item;
            let mut related = self.related;
            let parent_auto_increment = M::auto_increment_primary_key_column().is_some();
            let related_auto_increment = R::auto_increment_primary_key_column().is_some();

            let parent_item = if parent_auto_increment {
                let mut inserted_items = execute_insert_returning::<M, M, A>(vec![item], client).await?;

                inserted_items.drain(..).next().ok_or_else(|| {
                    dinoco_engine::DinocoError::RecordNotFound(format!(
                        "Record from table '{}' could not be loaded after insert.",
                        M::table_name()
                    ))
                })?
            } else {
                item.bind_relation(&mut related);
                execute_insert::<M, A>(vec![item.clone()], client).await?;
                item
            };

            if parent_auto_increment {
                parent_item.bind_relation(&mut related);
            }

            let relation_links = if related_auto_increment {
                let mut inserted_related_rows = execute_insert_returning::<R, R, A>(vec![related], client).await?;
                let inserted_related = inserted_related_rows.drain(..).next().ok_or_else(|| {
                    dinoco_engine::DinocoError::RecordNotFound(format!(
                        "Record from table '{}' could not be loaded after insert.",
                        R::table_name()
                    ))
                })?;

                parent_item.relation_links(&inserted_related)
            } else {
                let relation_links = parent_item.relation_links(&related);

                execute_insert::<R, A>(vec![related], client).await?;

                relation_links
            };

            execute_insert_relation_links(relation_links, client).await?;

            execute_reload_by_identity::<M, S, A>(&parent_item, client).await
        }
    }
}

impl<M, R> InsertWithConnection<M, R>
where
    M: InsertModel + InsertConnection<R> + Projection<M>,
{
    pub fn returning<S>(self) -> InsertWithConnectionReturning<M, R, S>
    where
        S: Projection<M>,
    {
        InsertWithConnectionReturning { item: self.item, connected: self.connected, marker: PhantomData }
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
            let item = self.item;
            let connected = self.connected;
            let parent_auto_increment = M::auto_increment_primary_key_column().is_some();
            let parent_item = if parent_auto_increment {
                let mut inserted_items = execute_insert_returning::<M, M, A>(vec![item], client).await?;

                inserted_items.drain(..).next().ok_or_else(|| {
                    dinoco_engine::DinocoError::RecordNotFound(format!(
                        "Record from table '{}' could not be loaded after insert.",
                        M::table_name()
                    ))
                })?
            } else {
                let connection_updates = item.connection_updates(&connected);
                let relation_links = item.connection_links(&connected);

                execute_insert::<M, A>(vec![item], client).await?;
                execute_connection_updates(connection_updates, client).await?;

                return execute_insert_relation_links(relation_links, client).await;
            };
            let connection_updates = parent_item.connection_updates(&connected);
            let relation_links = parent_item.connection_links(&connected);

            execute_connection_updates(connection_updates, client).await?;
            execute_insert_relation_links(relation_links, client).await
        }
    }
}

impl<M, R, S> InsertWithConnectionReturning<M, R, S>
where
    M: InsertModel + InsertConnection<R> + Projection<M> + Clone,
    S: Projection<M>,
{
    pub fn execute<'a, A>(
        self,
        client: &'a DinocoClient<A>,
    ) -> impl std::future::Future<Output = dinoco_engine::DinocoResult<S>> + 'a
    where
        M: 'a,
        R: 'a,
        S: 'a,
        A: DinocoAdapter,
    {
        async move {
            let item = self.item;
            let connected = self.connected;
            let parent_auto_increment = M::auto_increment_primary_key_column().is_some();
            let parent_item = if parent_auto_increment {
                let mut inserted_items = execute_insert_returning::<M, M, A>(vec![item], client).await?;

                inserted_items.drain(..).next().ok_or_else(|| {
                    dinoco_engine::DinocoError::RecordNotFound(format!(
                        "Record from table '{}' could not be loaded after insert.",
                        M::table_name()
                    ))
                })?
            } else {
                let connection_updates = item.connection_updates(&connected);
                let relation_links = item.connection_links(&connected);

                execute_insert::<M, A>(vec![item.clone()], client).await?;
                execute_connection_updates(connection_updates, client).await?;
                execute_insert_relation_links(relation_links, client).await?;

                return execute_reload_by_identity::<M, S, A>(&item, client).await;
            };
            let connection_updates = parent_item.connection_updates(&connected);
            let relation_links = parent_item.connection_links(&connected);

            execute_connection_updates(connection_updates, client).await?;
            execute_insert_relation_links(relation_links, client).await?;

            execute_reload_by_identity::<M, S, A>(&parent_item, client).await
        }
    }
}
