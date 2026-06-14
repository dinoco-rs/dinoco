use std::future::Future;

use dinoco_engine::{DinocoAdapter, DinocoClient, DinocoError, DinocoResult, QueryBuilder, SelectStatement};

use crate::{Model, Projection, ReadMode};

use super::DinocoCountRow;

pub fn execute_many<'a, M, S, A>(
    statement: SelectStatement,
    includes: &'a [crate::IncludeNode],
    counts: &'a [crate::CountNode],
    read_mode: ReadMode,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<Vec<S>>> + 'a
where
    M: Model,
    S: Projection<M>,
    A: DinocoAdapter,
{
    async move {
        let adapter = client.read_adapter(matches!(read_mode, ReadMode::Primary));
        let (sql, params) = adapter.dialect().build_select(&statement);
        let mut rows = adapter.query_as::<S>(&sql, &params).await?;

        S::load_includes(&mut rows, includes, client, read_mode).await?;
        S::load_counts(&mut rows, counts, client, read_mode).await?;

        Ok(rows)
    }
}

pub fn execute_first<'a, M, S, A>(
    statement: SelectStatement,
    read_mode: ReadMode,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<Option<S>>> + 'a
where
    M: Model,
    S: Projection<M>,
    A: DinocoAdapter,
{
    async move {
        let statement = statement.limit(1);
        let rows = execute_many::<M, S, A>(statement, &[], &[], read_mode, client).await?;

        Ok(rows.into_iter().next())
    }
}

pub fn execute_count<'a, A>(
    statement: SelectStatement,
    read_mode: ReadMode,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<usize>> + 'a
where
    A: DinocoAdapter,
{
    async move {
        let adapter = client.read_adapter(matches!(read_mode, ReadMode::Primary));
        let (sql, params) = adapter.dialect().build_count(&statement);
        let rows = adapter.query_as::<DinocoCountRow>(&sql, &params).await?;
        let count = rows.into_iter().next().map(|row| row.count).unwrap_or_default();

        usize::try_from(count).map_err(|_| DinocoError::ParseError(format!("Expected non-negative count, got {count}")))
    }
}
