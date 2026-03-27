use futures::Stream;
use std::pin::Pin;

mod databases;
mod error;
mod traits;
mod value;

pub use databases::*;
pub use error::*;
pub use traits::*;
pub use value::*;

pub type DinocoResult<T> = Result<T, DinocoError>;
pub type DinocoStream<T> = Pin<Box<dyn Stream<Item = DinocoResult<T>> + Send>>;

pub struct DinocoClient<T>
where
    T: DinocoAdapter,
    T: DinocoAdapterStream,
{
    pub adapter: T,
    pub read_replicas: Vec<T>,
}

impl DinocoClient<PostgresAdapter> {
    pub async fn new(url: String, reads: Vec<String>) -> DinocoResult<Self> {
        let adapter = PostgresAdapter::connect(url).await?;

        let mut read_replicas: Vec<PostgresAdapter> = Vec::with_capacity(reads.len());

        for read in reads {
            let adapter = PostgresAdapter::connect(read).await?;

            read_replicas.push(adapter);
        }

        Ok(Self { adapter, read_replicas })
    }
}

impl DinocoClient<MySqlAdapter> {
    pub async fn new(url: String, reads: Vec<String>) -> DinocoResult<Self> {
        let adapter = MySqlAdapter::connect(url).await?;

        let mut read_replicas: Vec<MySqlAdapter> = Vec::with_capacity(reads.len());

        for read in reads {
            let adapter = MySqlAdapter::connect(read).await?;

            read_replicas.push(adapter);
        }

        Ok(Self { adapter, read_replicas })
    }
}

impl DinocoType for i64 {
    fn from_row<R: DinocoDatabaseRow>(row: &R, idx: usize) -> DinocoResult<Self> {
        row.get_i64(idx)
    }
}

impl DinocoType for String {
    fn from_row<R: DinocoDatabaseRow>(row: &R, idx: usize) -> DinocoResult<Self> {
        row.get_string(idx)
    }
}

impl DinocoType for bool {
    fn from_row<R: DinocoDatabaseRow>(row: &R, idx: usize) -> DinocoResult<Self> {
        row.get_bool(idx)
    }
}

impl DinocoType for f64 {
    fn from_row<R: DinocoDatabaseRow>(row: &R, idx: usize) -> DinocoResult<Self> {
        row.get_f64(idx)
    }
}
