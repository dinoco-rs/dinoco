use std::marker::PhantomData;

use dinoco_engine::{DinocoAdapter, DinocoClient};

use crate::{InsertConnection, InsertModel, InsertRelation};
use crate::{execute_connection_updates, execute_insert, execute_insert_relation_links};

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

impl<M, R> InsertWithRelation<M, R>
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
            let item = self.item;
            let mut related = self.related;

            item.bind_relation(&mut related);
            let relation_links = item.relation_links(&related);

            execute_insert::<M, A>(vec![item], client).await?;
            execute_insert::<R, A>(vec![related], client).await?;
            execute_insert_relation_links(relation_links, client).await
        }
    }
}

impl<M, R> InsertWithConnection<M, R>
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
            let item = self.item;
            let connected = self.connected;
            let connection_updates = item.connection_updates(&connected);
            let relation_links = item.connection_links(&connected);

            execute_insert::<M, A>(vec![item], client).await?;
            execute_connection_updates(connection_updates, client).await?;
            execute_insert_relation_links(relation_links, client).await
        }
    }
}
