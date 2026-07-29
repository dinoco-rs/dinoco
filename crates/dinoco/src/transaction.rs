use dinoco_engine::{DinocoClient, TransactionCommand, TransactionResults};

pub trait IntoTransactionOperation {
    fn into_transaction_operation(self) -> TransactionCommand;
}

#[derive(Default)]
pub struct Transaction {
    commands: Vec<TransactionCommand>,
}

pub type Transcation = Transaction;

impl Transaction {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push<O>(&mut self, operation: O)
    where
        O: IntoTransactionOperation,
    {
        self.commands.push(operation.into_transaction_operation());
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

pub struct Transactions {
    transaction: Transaction,
}

pub fn transactions(transaction: Transaction) -> Transactions {
    Transactions { transaction }
}

impl Transactions {
    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<TransactionResults> {
        client.backend.execute_transaction(self.transaction.commands).await
    }
}

#[macro_export]
macro_rules! transaction {
    ($($operation:expr),* $(,)?) => {{
        let mut transaction = $crate::Transaction::new();
        $(transaction.push($operation);)*
        transaction
    }};
}
