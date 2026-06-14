use std::future::Future;

use dinoco_engine::{DinocoAdapter, DinocoClient, DinocoError, DinocoResult, SelectStatement};

use crate::{InsertModel, Model, Projection, ReadMode};

use super::execute_first;

pub(crate) fn execute_reload_by_identity<'a, M, S, A>(
    item: &'a M,
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<S>> + 'a
where
    M: InsertModel + 'a,
    S: Projection<M> + 'a,
    A: DinocoAdapter,
{
    async move { load_one_by_conditions::<M, S, A>(item.insert_identity_conditions(), client).await }
}

pub(crate) fn execute_reload_many_by_identity<'a, M, S, A>(
    items: &'a [M],
    client: &'a DinocoClient<A>,
) -> impl Future<Output = DinocoResult<Vec<S>>> + 'a
where
    M: InsertModel + 'a,
    S: Projection<M> + 'a,
    A: DinocoAdapter,
{
    async move {
        let identity_conditions = items.iter().map(InsertModel::insert_identity_conditions).collect::<Vec<_>>();

        load_many_by_conditions::<M, S, A>(identity_conditions, client).await
    }
}

pub(super) async fn load_many_by_conditions<M, S, A>(
    identity_conditions: Vec<Vec<dinoco_engine::Expression>>,
    client: &DinocoClient<A>,
) -> DinocoResult<Vec<S>>
where
    M: Model,
    S: Projection<M>,
    A: DinocoAdapter,
{
    let mut rows = Vec::with_capacity(identity_conditions.len());

    for conditions in identity_conditions {
        let item = load_one_by_conditions::<M, S, A>(conditions, client).await?;
        rows.push(item);
    }

    Ok(rows)
}

async fn load_one_by_conditions<M, S, A>(
    conditions: Vec<dinoco_engine::Expression>,
    client: &DinocoClient<A>,
) -> DinocoResult<S>
where
    M: Model,
    S: Projection<M>,
    A: DinocoAdapter,
{
    let mut statement = SelectStatement::new().from(M::table_name()).select(S::columns());

    for condition in conditions {
        statement = statement.condition(condition);
    }

    execute_first::<M, S, A>(statement, ReadMode::Primary, client).await?.ok_or_else(|| {
        DinocoError::RecordNotFound(format!("Record from table '{}' could not be loaded after write.", M::table_name()))
    })
}
