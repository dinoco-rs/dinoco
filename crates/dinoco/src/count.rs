use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;

use dinoco_engine::{
    CountQuery, DinocoClient, DinocoEntity, FindWhere, ManyToManyRelationCountQuery, RelationCountQuery,
};

pub trait DinocoCountModel<M>: Default {
    fn dinoco_set_total(&mut self, total: i64);
}

pub trait DinocoRelationCountApply {
    fn dinoco_apply_count(&mut self, relation: &'static str, count: i64);
}

pub type CountLoaderFuture<'a> = Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>>;

pub trait CountLoader<S>: Send + Sync {
    fn load_count<'a>(
        &'a self,
        parent_table: &'static str,
        _parent_field: &'static str,
        parent_conditions: &'a [FindWhere],
        client: &'a DinocoClient,
        target: &'a mut S,
    ) -> CountLoaderFuture<'a>;
}

pub trait IntoCountLoader<M, S> {
    fn into_count_loader(self) -> Box<dyn CountLoader<S>>;
}

pub struct RelationCount<M, C>
where
    C: DinocoEntity,
{
    relation: &'static str,
    parent_field: &'static str,
    child_field: &'static str,
    many_to_many: Option<ManyToManyCountMetadata>,
    conditions: Vec<FindWhere>,
    marker: PhantomData<fn() -> (M, C)>,
}

#[derive(Clone, Copy)]
struct ManyToManyCountMetadata {
    join_table: &'static str,
    join_parent_field: &'static str,
    join_child_field: &'static str,
}

impl<M, C> RelationCount<M, C>
where
    C: DinocoEntity,
{
    pub fn new(relation: &'static str, parent_field: &'static str, child_field: &'static str) -> Self {
        Self { relation, parent_field, child_field, many_to_many: None, conditions: Vec::new(), marker: PhantomData }
    }

    pub fn many_to_many(
        relation: &'static str,
        parent_field: &'static str,
        child_field: &'static str,
        join_table: &'static str,
        join_parent_field: &'static str,
        join_child_field: &'static str,
    ) -> Self {
        Self {
            relation,
            parent_field,
            child_field,
            many_to_many: Some(ManyToManyCountMetadata { join_table, join_parent_field, join_child_field }),
            conditions: Vec::new(),
            marker: PhantomData,
        }
    }
}

impl<M, C> RelationCount<M, C>
where
    C: DinocoEntity,
{
    pub fn where_<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(C::Where) -> FindWhere,
    {
        self.conditions.push(callback(C::Where::default()));

        self
    }
}

impl<M, S, C> IntoCountLoader<M, S> for RelationCount<M, C>
where
    M: DinocoEntity,
    S: DinocoRelationCountApply + Send,
    C: DinocoEntity,
{
    fn into_count_loader(self) -> Box<dyn CountLoader<S>> {
        Box::new(self)
    }
}

impl<M, S, C> CountLoader<S> for RelationCount<M, C>
where
    S: DinocoRelationCountApply + Send,
    C: DinocoEntity,
{
    fn load_count<'a>(
        &'a self,
        parent_table: &'static str,
        _parent_field: &'static str,
        parent_conditions: &'a [FindWhere],
        client: &'a DinocoClient,
        target: &'a mut S,
    ) -> CountLoaderFuture<'a> {
        Box::pin(async move {
            let total = if let Some(many_to_many) = self.many_to_many {
                client
                    .read_backend(false)
                    .count_many_to_many_relation(ManyToManyRelationCountQuery {
                        parent_table,
                        child_table: C::TABLE_NAME,
                        join_table: many_to_many.join_table,
                        parent_field: self.parent_field,
                        child_field: self.child_field,
                        join_parent_field: many_to_many.join_parent_field,
                        join_child_field: many_to_many.join_child_field,
                        parent_conditions: parent_conditions.to_vec(),
                        child_conditions: self.conditions.clone(),
                    })
                    .await?
            } else {
                client
                    .read_backend(false)
                    .count_relation(RelationCountQuery {
                        parent_table,
                        child_table: C::TABLE_NAME,
                        parent_field: self.parent_field,
                        child_field: self.child_field,
                        parent_conditions: parent_conditions.to_vec(),
                        child_conditions: self.conditions.clone(),
                    })
                    .await?
            };
            target.dinoco_apply_count(self.relation, total);

            Ok(())
        })
    }
}

pub async fn execute_count<M>(
    conditions: Vec<FindWhere>,
    loaders: Vec<Box<dyn CountLoader<M::Count>>>,
    client: &DinocoClient,
) -> anyhow::Result<M::Count>
where
    M: DinocoEntity,
    M::Count: DinocoCountModel<M>,
{
    let total =
        client.read_backend(false).count(CountQuery { table: M::TABLE_NAME, conditions: conditions.clone() }).await?;
    let mut count = M::Count::default();
    count.dinoco_set_total(total);

    for loader in loaders {
        loader.load_count(M::TABLE_NAME, "id", &conditions, client, &mut count).await?;
    }

    Ok(count)
}
