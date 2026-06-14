use std::future::Future;
use std::pin::Pin;

use dinoco_engine::{DinocoAdapter, DinocoClient, DinocoResult, DinocoTransactionAdapter};

use super::count::Count;
use super::delete::DeleteWithCond;
use super::delete_many::DeleteMany;
use super::find_and_update::FindAndUpdate;
use super::find_first::FindFirst;
use super::find_many::FindMany;
use super::insert_into::{
    Insert, InsertReturning, InsertWithConnection, InsertWithConnectionReturning, InsertWithRelation,
    InsertWithRelationReturning,
};
use super::insert_many::{
    InsertMany, InsertManyReturning, InsertManyWithConnection, InsertManyWithConnectionReturning,
    InsertManyWithConnections, InsertManyWithConnectionsReturning, InsertManyWithRelation,
    InsertManyWithRelationReturning, InsertManyWithRelations, InsertManyWithRelationsReturning,
};
use super::update::{Update, UpdateReturning};
use super::update_many::{UpdateMany, UpdateManyReturning};
use crate::{InsertPayload, Projection, UpdateModel, UpdatePayload};

type TransactionFuture<'a> = Pin<Box<dyn Future<Output = DinocoResult<()>> + Send + 'a>>;

trait TransactionRunnable<A: DinocoAdapter>: Send {
    fn run<'a>(self: Box<Self>, client: &'a DinocoClient<A>) -> TransactionFuture<'a>;
}

struct TransactionClosure<F> {
    closure: Option<F>,
}

impl<A, F> TransactionRunnable<A> for TransactionClosure<F>
where
    A: DinocoAdapter,
    F: for<'a> FnOnce(&'a DinocoClient<A>) -> TransactionFuture<'a> + Send + 'static,
{
    fn run<'a>(mut self: Box<Self>, client: &'a DinocoClient<A>) -> TransactionFuture<'a> {
        let closure = self.closure.take().expect("transaction action already consumed");

        closure(client)
    }
}

pub struct TransactionAction<A: DinocoAdapter> {
    runnable: Box<dyn TransactionRunnable<A>>,
}

impl<A> TransactionAction<A>
where
    A: DinocoAdapter,
{
    fn new<F>(closure: F) -> Self
    where
        F: for<'a> FnOnce(&'a DinocoClient<A>) -> TransactionFuture<'a> + Send + 'static,
    {
        Self { runnable: Box::new(TransactionClosure { closure: Some(closure) }) }
    }

    fn run<'a>(self, client: &'a DinocoClient<A>) -> TransactionFuture<'a> {
        self.runnable.run(client)
    }
}

pub trait IntoTransactionAction<A: DinocoAdapter> {
    fn into_transaction_action(self) -> TransactionAction<A>;
}

pub trait TransactionActionExt<A: DinocoAdapter>: Sized {
    fn tx(self) -> TransactionAction<A>
    where
        Self: IntoTransactionAction<A>,
    {
        self.into_transaction_action()
    }
}

impl<A, T> TransactionActionExt<A> for T where A: DinocoAdapter {}

pub fn tx<A, T>(action: T) -> TransactionAction<A>
where
    A: DinocoAdapter,
    T: IntoTransactionAction<A>,
{
    action.into_transaction_action()
}

pub struct Transactions<A: DinocoAdapter> {
    actions: Vec<TransactionAction<A>>,
}

pub fn transactions<A>(actions: Vec<TransactionAction<A>>) -> Transactions<A>
where
    A: DinocoAdapter,
{
    Transactions { actions }
}

impl<A> Transactions<A>
where
    A: DinocoAdapter + DinocoTransactionAdapter + Send + Sync,
{
    pub fn execute<'a>(self, client: &'a DinocoClient<A>) -> impl Future<Output = DinocoResult<()>> + Send + 'a
    where
        A: 'a,
    {
        async move {
            let mut actions = self.actions;

            client
                .primary()
                .with_transaction(move || {
                    Box::pin(async move {
                        for action in actions.drain(..) {
                            action.run(client).await?;
                        }

                        Ok(())
                    })
                })
                .await
        }
    }
}

impl<A, M> IntoTransactionAction<A> for Count<M>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: crate::Model + Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| {
            Box::pin(async move {
                self.read_in_primary().execute(client).await?;

                Ok(())
            })
        })
    }
}

impl<A, M> IntoTransactionAction<A> for DeleteWithCond<M>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: crate::Model + Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| Box::pin(async move { self.execute(client).await }))
    }
}

impl<A, M> IntoTransactionAction<A> for DeleteMany<M>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: crate::Model + Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| Box::pin(async move { self.execute(client).await }))
    }
}

impl<A, M> IntoTransactionAction<A> for FindAndUpdate<M>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: crate::FindAndUpdateModel + Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| {
            Box::pin(async move {
                self.execute(client).await?;

                Ok(())
            })
        })
    }
}

impl<A, M, S> IntoTransactionAction<A> for FindFirst<M, S>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: crate::Model + Send + Sync + 'static,
    S: Projection<M> + Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| {
            Box::pin(async move {
                self.read_in_primary().execute(client).await?;

                Ok(())
            })
        })
    }
}

impl<A, M, S> IntoTransactionAction<A> for FindMany<M, S>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: crate::Model + Send + Sync + 'static,
    S: Projection<M> + Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| {
            Box::pin(async move {
                self.read_in_primary().execute(client).await?;

                Ok(())
            })
        })
    }
}

impl<A, M, V> IntoTransactionAction<A> for Insert<M, V>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: crate::InsertModel + Projection<M> + Send + Sync + 'static,
    V: InsertPayload<M> + Send + 'static,
    <V as InsertPayload<M>>::Nested: Send + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| Box::pin(async move { self.execute(client).await }))
    }
}

impl<A, M, V, S> IntoTransactionAction<A> for InsertReturning<M, V, S>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: crate::InsertModel + Projection<M> + Send + Sync + 'static,
    V: InsertPayload<M> + Send + 'static,
    <V as InsertPayload<M>>::Nested: Send + 'static,
    S: Projection<M> + Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| {
            Box::pin(async move {
                self.execute(client).await?;

                Ok(())
            })
        })
    }
}

impl<A, M, R> IntoTransactionAction<A> for InsertWithRelation<M, R>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: crate::InsertModel + crate::InsertRelation<R> + Projection<M> + Clone + Send + Sync + 'static,
    R: crate::InsertModel + Projection<R> + Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| Box::pin(async move { self.execute(client).await }))
    }
}

impl<A, M, R, S> IntoTransactionAction<A> for InsertWithRelationReturning<M, R, S>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: crate::InsertModel + crate::InsertRelation<R> + Projection<M> + Clone + Send + Sync + 'static,
    R: crate::InsertModel + Projection<R> + Send + Sync + 'static,
    S: Projection<M> + Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| {
            Box::pin(async move {
                self.execute(client).await?;

                Ok(())
            })
        })
    }
}

impl<A, M, R> IntoTransactionAction<A> for InsertWithConnection<M, R>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: crate::InsertModel + crate::InsertConnection<R> + Projection<M> + Send + Sync + 'static,
    R: Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| Box::pin(async move { self.execute(client).await }))
    }
}

impl<A, M, R, S> IntoTransactionAction<A> for InsertWithConnectionReturning<M, R, S>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: crate::InsertModel + crate::InsertConnection<R> + Projection<M> + Clone + Send + Sync + 'static,
    R: Send + Sync + 'static,
    S: Projection<M> + Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| {
            Box::pin(async move {
                self.execute(client).await?;

                Ok(())
            })
        })
    }
}

impl<A, M, V> IntoTransactionAction<A> for InsertMany<M, V>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: crate::InsertModel + Projection<M> + Send + Sync + 'static,
    V: InsertPayload<M> + Send + 'static,
    <V as InsertPayload<M>>::Nested: Send + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| Box::pin(async move { self.execute(client).await }))
    }
}

impl<A, M, V, S> IntoTransactionAction<A> for InsertManyReturning<M, V, S>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: crate::InsertModel + Projection<M> + Send + Sync + 'static,
    V: InsertPayload<M> + Send + 'static,
    <V as InsertPayload<M>>::Nested: Send + 'static,
    S: Projection<M> + Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| {
            Box::pin(async move {
                self.execute(client).await?;

                Ok(())
            })
        })
    }
}

impl<A, M, R> IntoTransactionAction<A> for InsertManyWithRelation<M, R>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: crate::InsertModel + crate::InsertRelation<R> + Projection<M> + Clone + Send + Sync + 'static,
    R: crate::InsertModel + Projection<R> + Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| Box::pin(async move { self.execute(client).await }))
    }
}

impl<A, M, R, S> IntoTransactionAction<A> for InsertManyWithRelationReturning<M, R, S>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: crate::InsertModel + crate::InsertRelation<R> + Projection<M> + Clone + Send + Sync + 'static,
    R: crate::InsertModel + Projection<R> + Send + Sync + 'static,
    S: Projection<M> + Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| {
            Box::pin(async move {
                self.execute(client).await?;

                Ok(())
            })
        })
    }
}

impl<A, M, R> IntoTransactionAction<A> for InsertManyWithRelations<M, R>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: crate::InsertModel + crate::InsertRelation<R> + Projection<M> + Clone + Send + Sync + 'static,
    R: crate::InsertModel + Projection<R> + Clone + Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| Box::pin(async move { self.execute(client).await }))
    }
}

impl<A, M, R, S> IntoTransactionAction<A> for InsertManyWithRelationsReturning<M, R, S>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: crate::InsertModel + crate::InsertRelation<R> + Projection<M> + Clone + Send + Sync + 'static,
    R: crate::InsertModel + Projection<R> + Clone + Send + Sync + 'static,
    S: Projection<M> + Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| {
            Box::pin(async move {
                self.execute(client).await?;

                Ok(())
            })
        })
    }
}

impl<A, M, R> IntoTransactionAction<A> for InsertManyWithConnections<M, R>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: crate::InsertModel + crate::InsertConnection<R> + Projection<M> + Send + Sync + 'static,
    R: Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| Box::pin(async move { self.execute(client).await }))
    }
}

impl<A, M, R, S> IntoTransactionAction<A> for InsertManyWithConnectionsReturning<M, R, S>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: crate::InsertModel + crate::InsertConnection<R> + Projection<M> + Clone + Send + Sync + 'static,
    R: Send + Sync + 'static,
    S: Projection<M> + Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| {
            Box::pin(async move {
                self.execute(client).await?;

                Ok(())
            })
        })
    }
}

impl<A, M, R> IntoTransactionAction<A> for InsertManyWithConnection<M, R>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: crate::InsertModel + crate::InsertConnection<R> + Projection<M> + Send + Sync + 'static,
    R: Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| Box::pin(async move { self.execute(client).await }))
    }
}

impl<A, M, R, S> IntoTransactionAction<A> for InsertManyWithConnectionReturning<M, R, S>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: crate::InsertModel + crate::InsertConnection<R> + Projection<M> + Clone + Send + Sync + 'static,
    R: Send + Sync + 'static,
    S: Projection<M> + Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| {
            Box::pin(async move {
                self.execute(client).await?;

                Ok(())
            })
        })
    }
}

impl<A, M, V> IntoTransactionAction<A> for Update<M, V>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: UpdateModel + Send + Sync + 'static,
    V: UpdatePayload<M> + Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| Box::pin(async move { self.execute(client).await }))
    }
}

impl<A, M, V, S> IntoTransactionAction<A> for UpdateReturning<M, V, S>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: UpdateModel + Projection<M> + Send + Sync + 'static,
    V: UpdatePayload<M> + Send + Sync + 'static,
    S: Projection<M> + Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| {
            Box::pin(async move {
                self.execute(client).await?;

                Ok(())
            })
        })
    }
}

impl<A, M, V> IntoTransactionAction<A> for UpdateMany<M, V>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: UpdateModel + Send + Sync + 'static,
    V: UpdatePayload<M> + Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| Box::pin(async move { self.execute(client).await }))
    }
}

impl<A, M, V, S> IntoTransactionAction<A> for UpdateManyReturning<M, V, S>
where
    A: DinocoAdapter + Send + Sync + 'static,
    M: UpdateModel + Send + Sync + 'static,
    V: UpdatePayload<M> + Send + Sync + 'static,
    S: Projection<M> + Send + Sync + 'static,
{
    fn into_transaction_action(self) -> TransactionAction<A> {
        TransactionAction::new(move |client| {
            Box::pin(async move {
                self.execute(client).await?;

                Ok(())
            })
        })
    }
}
