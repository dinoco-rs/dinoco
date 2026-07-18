use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;

use dinoco_engine::{CountQuery, DinocoClient, DinocoEntity, FindWhere, RelationCountQuery};

pub trait DinocoCountModel<M>: Default {
    fn dinoco_set_total(&mut self, total: i64);
}

pub trait DinocoRelationCountApply<C> {
    fn dinoco_apply_count(&mut self, relation: &'static str, count: C);
}

pub type CountLoaderFuture<'a> = Pin<Box<dyn Future<Output = anyhow::Result<()>> + 'a>>;

pub trait CountLoader<S> {
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

pub struct RelationCount<M, C, CC = <C as DinocoEntity>::Count>
where
    C: DinocoEntity,
{
    relation: &'static str,
    parent_field: &'static str,
    child_field: &'static str,
    conditions: Vec<FindWhere>,
    includes: Vec<Box<dyn CountLoader<C::Count>>>,
    marker: PhantomData<fn() -> (M, C, CC)>,
}

impl<M, C> RelationCount<M, C, <C as DinocoEntity>::Count>
where
    C: DinocoEntity,
{
    pub fn new(relation: &'static str, parent_field: &'static str, child_field: &'static str) -> Self {
        Self { relation, parent_field, child_field, conditions: Vec::new(), includes: Vec::new(), marker: PhantomData }
    }
}

impl<M, C> RelationCount<M, C, <C as DinocoEntity>::Count>
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

    pub fn includes<F, I>(mut self, callback: F) -> Self
    where
        F: FnOnce(C::Count) -> I,
        I: IntoCountLoader<C, C::Count>,
    {
        self.includes.push(callback(C::Count::default()).into_count_loader());

        self
    }

    pub fn count<F, I>(self, callback: F) -> Self
    where
        F: FnOnce(C::Count) -> I,
        I: IntoCountLoader<C, C::Count>,
    {
        self.includes(callback)
    }
}

impl<M, S, C> IntoCountLoader<M, S> for RelationCount<M, C, <C as DinocoEntity>::Count>
where
    M: DinocoEntity,
    S: DinocoRelationCountApply<C::Count>,
    C: DinocoEntity,
    C::Count: DinocoCountModel<C> + 'static,
{
    fn into_count_loader(self) -> Box<dyn CountLoader<S>> {
        Box::new(self)
    }
}

impl<M, S, C> CountLoader<S> for RelationCount<M, C, <C as DinocoEntity>::Count>
where
    S: DinocoRelationCountApply<C::Count>,
    C: DinocoEntity,
    C::Count: DinocoCountModel<C>,
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
            let total = client
                .read_backend(false)
                .count_relation(RelationCountQuery {
                    parent_table,
                    child_table: C::TABLE_NAME,
                    parent_field: self.parent_field,
                    child_field: self.child_field,
                    parent_conditions: parent_conditions.to_vec(),
                    child_conditions: self.conditions.clone(),
                })
                .await?;
            let mut count = C::Count::default();
            count.dinoco_set_total(total);

            for include in &self.includes {
                let nested_parent_conditions = relation_conditions(parent_conditions, self.child_field)
                    .into_iter()
                    .chain(self.conditions.clone())
                    .collect::<Vec<_>>();

                include
                    .load_count(C::TABLE_NAME, self.parent_field, &nested_parent_conditions, client, &mut count)
                    .await?;
            }

            target.dinoco_apply_count(self.relation, count);

            Ok(())
        })
    }
}

fn relation_conditions(parent_conditions: &[FindWhere], child_field: &'static str) -> Vec<FindWhere> {
    parent_conditions
        .iter()
        .filter_map(|condition| match condition {
            FindWhere::Eq(_, value) => Some(FindWhere::Eq(child_field, value.clone())),
            FindWhere::Batch(_, values) => Some(FindWhere::Batch(child_field, values.clone())),
            _ => None,
        })
        .collect()
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
