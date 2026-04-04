use futures::Stream;
use std::pin::Pin;

mod databases;
mod error;
mod migration;
mod sql;
mod traits;
mod value;

pub use databases::*;
pub use error::*;
pub use migration::*;
pub use sql::*;
pub use traits::*;
pub use value::*;

pub type DinocoResult<T> = Result<T, DinocoError>;
pub type DinocoStream<T> = Pin<Box<dyn Stream<Item = DinocoResult<T>> + Send>>;

pub struct DinocoClient<T: DinocoAdapter> {
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

impl DinocoType for Vec<u8> {
    fn from_row<R: DinocoDatabaseRow>(row: &R, idx: usize) -> DinocoResult<Self> {
        row.get_bytes(idx)
    }
}

impl DinocoType for Option<i64> {
    fn from_row<R: DinocoDatabaseRow>(row: &R, idx: usize) -> DinocoResult<Self> {
        match row.get_i64(idx) {
            Ok(v) => Ok(Some(v)),
            Err(_) => Ok(None),
        }
    }
}

impl DinocoType for Option<String> {
    fn from_row<R: DinocoDatabaseRow>(row: &R, idx: usize) -> DinocoResult<Self> {
        match row.get_string(idx) {
            Ok(v) => Ok(Some(v)),
            Err(_) => Ok(None),
        }
    }
}

impl DinocoType for Option<bool> {
    fn from_row<R: DinocoDatabaseRow>(row: &R, idx: usize) -> DinocoResult<Self> {
        match row.get_bool(idx) {
            Ok(v) => Ok(Some(v)),
            Err(_) => Ok(None),
        }
    }
}

impl DinocoType for Option<f64> {
    fn from_row<R: DinocoDatabaseRow>(row: &R, idx: usize) -> DinocoResult<Self> {
        match row.get_f64(idx) {
            Ok(v) => Ok(Some(v)),
            Err(_) => Ok(None),
        }
    }
}

impl DinocoType for serde_json::Value {
    fn from_row<R: DinocoDatabaseRow>(row: &R, idx: usize) -> DinocoResult<Self> {
        let s = row.get_string(idx)?;

        serde_json::from_str(&s).map_err(|e| DinocoError::ParseError(e.to_string()))
    }
}

impl DinocoType for chrono::DateTime<chrono::Utc> {
    fn from_row<R: DinocoDatabaseRow>(row: &R, idx: usize) -> DinocoResult<Self> {
        let s = row.get_string(idx)?;

        s.parse().map_err(|e| DinocoError::ParseError(format!("Invalid datetime: {}", e)))
    }
}
