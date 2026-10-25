use std::future::Future;
use std::sync::Arc;

use dinoco_engine::{
    Backend, DatabaseError, DinocoClient, DinocoSqlCompiler, DinocoValue, ExecutedQuery, InsertHook, InsertQuery, QueryHooks,
    RowCountHook,
};
use futures_util::FutureExt;
use futures_util::future::Either;

/// The hooks of one client, captured together with the backend used to
/// compile the SQL reported to them.
#[derive(Clone)]
pub(crate) struct Observer {
    hooks: Arc<QueryHooks>,
    backend: Backend,
    in_transaction: bool,
}

#[derive(Clone, Copy)]
pub(crate) enum RowCountKind {
    Update,
    Delete,
    Find,
}

type Compile<Q> = fn(&dyn DinocoSqlCompiler, Q) -> (String, Vec<DinocoValue>);

impl Observer {
    pub(crate) fn for_client(client: &DinocoClient) -> Option<Self> {
        client.query_hooks().map(|hooks| Self { hooks, backend: client.backend.clone(), in_transaction: false })
    }

    pub(crate) fn in_transaction(mut self) -> Self {
        self.in_transaction = true;
        self
    }

    fn executed(&self, table: &'static str, (sql, params): (String, Vec<DinocoValue>)) -> ExecutedQuery {
        ExecutedQuery { table, sql, params, in_transaction: self.in_transaction }
    }

    fn insert_hook(&self, table: &str) -> Option<InsertHook> {
        self.hooks.table(table)?.on_insert.clone()
    }

    fn row_count_hook(&self, kind: RowCountKind, table: &str) -> Option<RowCountHook> {
        let hooks = self.hooks.table(table)?;
        match kind {
            RowCountKind::Update => hooks.on_update.clone(),
            RowCountKind::Delete => hooks.on_delete.clone(),
            RowCountKind::Find => hooks.on_find.clone(),
        }
    }
}

pub(crate) async fn observe_insert<T, F, Fut>(
    observer: Option<&Observer>,
    query: InsertQuery,
    run: F,
) -> anyhow::Result<T>
where
    F: FnOnce(InsertQuery) -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    let Some((observer, hook)) = observer.and_then(|observer| Some((observer, observer.insert_hook(query.table)?)))
    else {
        return run(query).await;
    };

    let executed = observer.executed(query.table, observer.backend.sql_compiler().compile_insert_query(query.clone()));
    let rows = inserted_rows(&query);

    match run(query).await {
        Ok(value) => {
            hook(Some(&rows), &executed, None);
            Ok(value)
        }
        Err(error) => {
            let error = DatabaseError::new(error);
            hook(None, &executed, Some(&error));
            Err(error.into_original())
        }
    }
}

/// `run(query)`, reported to the table's hook of `kind` when it has one.
///
/// Returns the run's future itself, or that future mapped to report its
/// result, instead of awaiting it: an `async fn` here would keep `query`
/// twice (as its argument and as the copy moved into `run`) next to the
/// run's own state in every read's future.
pub(crate) fn observe_rows<Q, T, F, Fut, R>(
    observer: Option<&Observer>,
    kind: RowCountKind,
    table: &'static str,
    query: Q,
    compile: Compile<Q>,
    run: F,
    row_count: R,
) -> impl Future<Output = anyhow::Result<T>> + use<Q, T, F, Fut, R>
where
    Q: Clone,
    F: FnOnce(Q) -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
    R: FnOnce(&T) -> usize,
{
    let Some((observer, hook)) = observer.and_then(|observer| Some((observer, observer.row_count_hook(kind, table)?))) else {
        return Either::Left(run(query));
    };

    let executed = observer.executed(table, compile(observer.backend.sql_compiler(), query.clone()));

    Either::Right(run(query).map(move |result| match result {
        Ok(value) => {
            hook(Some(row_count(&value)), &executed, None);
            Ok(value)
        }
        Err(error) => {
            let error = DatabaseError::new(error);
            hook(None, &executed, Some(&error));
            Err(error.into_original())
        }
    }))
}

fn inserted_rows(query: &InsertQuery) -> Vec<serde_json::Value> {
    query
        .rows
        .iter()
        .map(|row| {
            query
                .fields
                .iter()
                .zip(row)
                .map(|(field, value)| (field.to_string(), value.to_json()))
                .collect::<serde_json::Map<_, _>>()
                .into()
        })
        .collect()
}
