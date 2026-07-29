use std::marker::PhantomData;

use dinoco_engine::{CountQuery, DinocoClient, DinocoEntity, FindWhere, TransactionCommand};

use crate::{CountLoader, DinocoCountModel, IntoCountLoader, IntoTransactionOperation, execute_count};

pub struct Count<M>
where
    M: DinocoEntity,
{
    conditions: Vec<FindWhere>,
    counts: Vec<Box<dyn CountLoader<M::Count>>>,
    marker: PhantomData<M>,
}

pub fn count<M>() -> Count<M>
where
    M: DinocoEntity,
{
    Count { conditions: Vec::new(), counts: Vec::new(), marker: PhantomData }
}

impl<M> Count<M>
where
    M: DinocoEntity,
    M::Count: 'static,
{
    pub fn where_<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::Where) -> FindWhere,
    {
        self.conditions.push(callback(M::Where::default()));

        self
    }

    pub fn includes<F, I>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::CountInclude) -> I,
        I: IntoCountLoader<M, M::Count>,
    {
        self.counts.push(callback(M::CountInclude::default()).into_count_loader());

        self
    }

    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<M::Count>
    where
        M::Count: crate::DinocoCountModel<M>,
    {
        execute_count::<M>(self.conditions, self.counts, client).await
    }
}

impl<M> IntoTransactionOperation for Count<M>
where
    M: DinocoEntity,
    M::Count: DinocoCountModel<M> + Send + 'static,
{
    fn into_transaction_operation(self) -> TransactionCommand {
        if !self.counts.is_empty() {
            return TransactionCommand::invalid(
                "count().includes(...) is not supported inside a transaction batch yet.",
            );
        }

        TransactionCommand::count(
            CountQuery { table: M::TABLE_NAME, conditions: self.conditions },
            transaction_count_result::<M>,
        )
    }
}

fn transaction_count_result<M>(total: i64) -> M::Count
where
    M: DinocoEntity,
    M::Count: DinocoCountModel<M>,
{
    let mut count = M::Count::default();
    count.dinoco_set_total(total);
    count
}
