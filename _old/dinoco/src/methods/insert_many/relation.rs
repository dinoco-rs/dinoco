use std::marker::PhantomData;

use dinoco_engine::{DinocoAdapter, DinocoResult};

use crate::{
    InsertModel, InsertRelation, Projection, execute_insert, execute_insert_related_payloads,
    execute_insert_relation_links, execute_insert_returning, execution::execute_reload_many_by_identity,
};

use super::{
    InsertManyWithRelation, InsertManyWithRelationReturning, InsertManyWithRelations, InsertManyWithRelationsReturning,
    validate_pair_len,
};

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
        client: &'a crate::DinocoClient<A>,
    ) -> impl std::future::Future<Output = DinocoResult<()>> + Send + 'a
    where
        M: Send + Sync + 'a,
        R: Send + Sync + 'a,
        A: DinocoAdapter,
    {
        async move {
            let items = self.items;
            let mut related_items = self.related_items;
            let parent_auto_increment = M::auto_increment_primary_key_column().is_some();
            let related_auto_increment = R::auto_increment_primary_key_column().is_some();

            validate_pair_len(items.len(), related_items.len(), "relation size")?;

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
            let mut related_items = self.related_items;
            let parent_auto_increment = M::auto_increment_primary_key_column().is_some();
            let related_auto_increment = R::auto_increment_primary_key_column().is_some();

            validate_pair_len(items.len(), related_items.len(), "relation size")?;

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
        client: &'a crate::DinocoClient<A>,
    ) -> impl std::future::Future<Output = DinocoResult<()>> + Send + 'a
    where
        M: Send + Sync + 'a,
        R: Send + Sync + 'a,
        A: DinocoAdapter,
    {
        async move {
            let items = self.items;
            let related_groups = self.related_groups;
            let parent_auto_increment = M::auto_increment_primary_key_column().is_some();

            validate_pair_len(items.len(), related_groups.len(), "relation group")?;

            let parent_items = if parent_auto_increment {
                execute_insert_returning::<M, M, A>(items, client).await?
            } else {
                execute_insert::<M, A>(items.clone(), client).await?;
                items
            };

            for (parent_item, related_group) in parent_items.iter().zip(related_groups.into_iter()) {
                execute_insert_related_payloads::<M, R, R, A>(parent_item, related_group, client).await?;
            }

            Ok(())
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
            let related_groups = self.related_groups;
            let parent_auto_increment = M::auto_increment_primary_key_column().is_some();

            validate_pair_len(items.len(), related_groups.len(), "relation group")?;

            let parent_items = if parent_auto_increment {
                execute_insert_returning::<M, M, A>(items, client).await?
            } else {
                execute_insert::<M, A>(items.clone(), client).await?;
                items
            };

            for (parent_item, related_group) in parent_items.iter().zip(related_groups.into_iter()) {
                execute_insert_related_payloads::<M, R, R, A>(parent_item, related_group, client).await?;
            }

            execute_reload_many_by_identity::<M, S, A>(&parent_items, client).await
        }
    }
}
