use std::future::Future;

use dinoco_engine::{DinocoAdapter, DinocoClient, DinocoResult, QueryBuilder, SelectStatement};

use crate::{Model, Projection, ReadMode};

pub fn execute_many<'a, M, S, A>(
    statement: SelectStatement,
    includes: &'a [crate::IncludeNode],
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
        // statement.limit = Some(1);

        let mut rows = execute_many::<M, S, A>(statement, &[], read_mode, client).await?;

        Ok(rows.drain(..).next())
    }
}
