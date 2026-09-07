use dinoco_engine::{
    DinocoClient, DinocoEntity, DinocoJson, DinocoRowModel, FindBatchItem, FindBatchQuery, FindQuery, QueryMode,
    RowDecodeError,
};

use crate::{FindFirst, FindMany};

/// One `find_many::<M>()`/`find_first::<M>()` builder that can take part in a
/// `find_batch(...)` call.
///
/// `compile()` and `decode(...)` back the "single query" [`QueryMode`]: the
/// builder's [`FindQuery`] is compiled into a JSON-aggregated subquery, and
/// the resulting JSON array is turned back into rows without a native driver
/// round trip. `execute_alone(...)` backs the default "batch query" mode,
/// where every item just runs its own `.execute(...)`.
#[async_trait::async_trait]
pub trait BatchFindable: Sized + Send {
    type Output: Send;

    fn compile(self) -> anyhow::Result<(FindQuery, bool)>;

    fn decode(values: Vec<dinoco_engine::serde_json::Value>) -> anyhow::Result<Self::Output>;

    async fn execute_alone(self, client: &DinocoClient) -> anyhow::Result<Self::Output>;
}

#[async_trait::async_trait]
impl<M, S> BatchFindable for FindMany<M, S>
where
    M: DinocoEntity + DinocoRowModel + Send,
    S: DinocoRowModel + DinocoJson + Send,
{
    type Output = Vec<S>;

    fn compile(self) -> anyhow::Result<(FindQuery, bool)> {
        self.into_batch_parts()
    }

    fn decode(values: Vec<dinoco_engine::serde_json::Value>) -> anyhow::Result<Self::Output> {
        values
            .iter()
            .map(|value| S::from_json_row(value).ok_or_else(|| RowDecodeError::new(std::any::type_name::<S>()).into()))
            .collect()
    }

    async fn execute_alone(self, client: &DinocoClient) -> anyhow::Result<Self::Output> {
        self.execute(client).await
    }
}

#[async_trait::async_trait]
impl<M, S> BatchFindable for FindFirst<M, S>
where
    M: DinocoEntity + DinocoRowModel + Send,
    S: DinocoRowModel + DinocoJson + Send,
{
    type Output = Option<S>;

    fn compile(self) -> anyhow::Result<(FindQuery, bool)> {
        self.into_batch_parts()
    }

    fn decode(values: Vec<dinoco_engine::serde_json::Value>) -> anyhow::Result<Self::Output> {
        match values.into_iter().next() {
            Some(value) => {
                S::from_json_row(&value).ok_or_else(|| RowDecodeError::new(std::any::type_name::<S>()).into()).map(Some)
            }
            None => Ok(None),
        }
    }

    async fn execute_alone(self, client: &DinocoClient) -> anyhow::Result<Self::Output> {
        self.execute(client).await
    }
}

/// A tuple of [`BatchFindable`] items that `find_batch(...)` can run together.
///
/// Implemented for tuples of 2 to 8 items; see [`find_batch`].
#[async_trait::async_trait]
pub trait FindBatchTuple: Sized + Send {
    type Output: Send;

    async fn execute_batch(self, client: &DinocoClient) -> anyhow::Result<Self::Output>;
}

/// Runs several independent `find_many`/`find_first` queries together.
///
/// ```ignore
/// let (users, offices, businesses) = find_batch((
///     find_many::<User>(),
///     find_many::<UserOffice>(),
///     find_many::<Business>(),
/// ))
/// .execute(&connection)
/// .await?;
/// ```
///
/// By default (`QueryMode::BatchQuery`) each item runs as its own query, same
/// as calling `.execute(...)` on it directly. Set `QueryMode::SingleQuery` —
/// via `DinocoClient::with_query_mode(...)` or `schema.dinoco`'s
/// `config.query_mode`  — to combine every item into a single round trip
/// instead (see [`BatchFindable`]). Items may not use `.includes(...)`.
pub struct FindBatch<T> {
    items: T,
}

impl<T> FindBatch<T>
where
    T: FindBatchTuple,
{
    pub async fn execute(self, client: &DinocoClient) -> anyhow::Result<T::Output> {
        self.items.execute_batch(client).await
    }
}

pub fn find_batch<T>(items: T) -> FindBatch<T>
where
    T: FindBatchTuple,
{
    FindBatch { items }
}

#[async_trait::async_trait]
impl<A, B> FindBatchTuple for (A, B,)
where
    A: BatchFindable + Send,
    B: BatchFindable + Send,
{
    type Output = (A::Output, B::Output,);

    async fn execute_batch(self, client: &DinocoClient) -> anyhow::Result<Self::Output> {
        let (i0, i1,) = self;

        match client.query_mode() {
            QueryMode::BatchQuery => Ok(futures::try_join!(i0.execute_alone(client), i1.execute_alone(client))?),
            QueryMode::SingleQuery => {
                let (q0, p0) = i0.compile()?;
                let (q1, p1) = i1.compile()?;
                let read_primary = p0 || p1;
                let mut items = Vec::new();
                items.push(FindBatchItem { query: q0 });
                items.push(FindBatchItem { query: q1 });

                let columns =
                    client.read_backend(read_primary).find_batch(FindBatchQuery { items }).await?;
                let mut columns = columns.into_iter();

                let r0 = A::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r1 = B::decode(columns.next().expect("find_batch returned fewer columns than items"))?;

                Ok((r0, r1,))
            }
        }
    }
}

#[async_trait::async_trait]
impl<A, B, C> FindBatchTuple for (A, B, C,)
where
    A: BatchFindable + Send,
    B: BatchFindable + Send,
    C: BatchFindable + Send,
{
    type Output = (A::Output, B::Output, C::Output,);

    async fn execute_batch(self, client: &DinocoClient) -> anyhow::Result<Self::Output> {
        let (i0, i1, i2,) = self;

        match client.query_mode() {
            QueryMode::BatchQuery => Ok(futures::try_join!(i0.execute_alone(client), i1.execute_alone(client), i2.execute_alone(client))?),
            QueryMode::SingleQuery => {
                let (q0, p0) = i0.compile()?;
                let (q1, p1) = i1.compile()?;
                let (q2, p2) = i2.compile()?;
                let read_primary = p0 || p1 || p2;
                let mut items = Vec::new();
                items.push(FindBatchItem { query: q0 });
                items.push(FindBatchItem { query: q1 });
                items.push(FindBatchItem { query: q2 });

                let columns =
                    client.read_backend(read_primary).find_batch(FindBatchQuery { items }).await?;
                let mut columns = columns.into_iter();

                let r0 = A::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r1 = B::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r2 = C::decode(columns.next().expect("find_batch returned fewer columns than items"))?;

                Ok((r0, r1, r2,))
            }
        }
    }
}

#[async_trait::async_trait]
impl<A, B, C, D> FindBatchTuple for (A, B, C, D,)
where
    A: BatchFindable + Send,
    B: BatchFindable + Send,
    C: BatchFindable + Send,
    D: BatchFindable + Send,
{
    type Output = (A::Output, B::Output, C::Output, D::Output,);

    async fn execute_batch(self, client: &DinocoClient) -> anyhow::Result<Self::Output> {
        let (i0, i1, i2, i3,) = self;

        match client.query_mode() {
            QueryMode::BatchQuery => Ok(futures::try_join!(i0.execute_alone(client), i1.execute_alone(client), i2.execute_alone(client), i3.execute_alone(client))?),
            QueryMode::SingleQuery => {
                let (q0, p0) = i0.compile()?;
                let (q1, p1) = i1.compile()?;
                let (q2, p2) = i2.compile()?;
                let (q3, p3) = i3.compile()?;
                let read_primary = p0 || p1 || p2 || p3;
                let mut items = Vec::new();
                items.push(FindBatchItem { query: q0 });
                items.push(FindBatchItem { query: q1 });
                items.push(FindBatchItem { query: q2 });
                items.push(FindBatchItem { query: q3 });

                let columns =
                    client.read_backend(read_primary).find_batch(FindBatchQuery { items }).await?;
                let mut columns = columns.into_iter();

                let r0 = A::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r1 = B::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r2 = C::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r3 = D::decode(columns.next().expect("find_batch returned fewer columns than items"))?;

                Ok((r0, r1, r2, r3,))
            }
        }
    }
}

#[async_trait::async_trait]
impl<A, B, C, D, E> FindBatchTuple for (A, B, C, D, E,)
where
    A: BatchFindable + Send,
    B: BatchFindable + Send,
    C: BatchFindable + Send,
    D: BatchFindable + Send,
    E: BatchFindable + Send,
{
    type Output = (A::Output, B::Output, C::Output, D::Output, E::Output,);

    async fn execute_batch(self, client: &DinocoClient) -> anyhow::Result<Self::Output> {
        let (i0, i1, i2, i3, i4,) = self;

        match client.query_mode() {
            QueryMode::BatchQuery => Ok(futures::try_join!(i0.execute_alone(client), i1.execute_alone(client), i2.execute_alone(client), i3.execute_alone(client), i4.execute_alone(client))?),
            QueryMode::SingleQuery => {
                let (q0, p0) = i0.compile()?;
                let (q1, p1) = i1.compile()?;
                let (q2, p2) = i2.compile()?;
                let (q3, p3) = i3.compile()?;
                let (q4, p4) = i4.compile()?;
                let read_primary = p0 || p1 || p2 || p3 || p4;
                let mut items = Vec::new();
                items.push(FindBatchItem { query: q0 });
                items.push(FindBatchItem { query: q1 });
                items.push(FindBatchItem { query: q2 });
                items.push(FindBatchItem { query: q3 });
                items.push(FindBatchItem { query: q4 });

                let columns =
                    client.read_backend(read_primary).find_batch(FindBatchQuery { items }).await?;
                let mut columns = columns.into_iter();

                let r0 = A::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r1 = B::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r2 = C::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r3 = D::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r4 = E::decode(columns.next().expect("find_batch returned fewer columns than items"))?;

                Ok((r0, r1, r2, r3, r4,))
            }
        }
    }
}

#[async_trait::async_trait]
impl<A, B, C, D, E, F> FindBatchTuple for (A, B, C, D, E, F,)
where
    A: BatchFindable + Send,
    B: BatchFindable + Send,
    C: BatchFindable + Send,
    D: BatchFindable + Send,
    E: BatchFindable + Send,
    F: BatchFindable + Send,
{
    type Output = (A::Output, B::Output, C::Output, D::Output, E::Output, F::Output,);

    async fn execute_batch(self, client: &DinocoClient) -> anyhow::Result<Self::Output> {
        let (i0, i1, i2, i3, i4, i5,) = self;

        match client.query_mode() {
            QueryMode::BatchQuery => Ok(futures::try_join!(i0.execute_alone(client), i1.execute_alone(client), i2.execute_alone(client), i3.execute_alone(client), i4.execute_alone(client), i5.execute_alone(client))?),
            QueryMode::SingleQuery => {
                let (q0, p0) = i0.compile()?;
                let (q1, p1) = i1.compile()?;
                let (q2, p2) = i2.compile()?;
                let (q3, p3) = i3.compile()?;
                let (q4, p4) = i4.compile()?;
                let (q5, p5) = i5.compile()?;
                let read_primary = p0 || p1 || p2 || p3 || p4 || p5;
                let mut items = Vec::new();
                items.push(FindBatchItem { query: q0 });
                items.push(FindBatchItem { query: q1 });
                items.push(FindBatchItem { query: q2 });
                items.push(FindBatchItem { query: q3 });
                items.push(FindBatchItem { query: q4 });
                items.push(FindBatchItem { query: q5 });

                let columns =
                    client.read_backend(read_primary).find_batch(FindBatchQuery { items }).await?;
                let mut columns = columns.into_iter();

                let r0 = A::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r1 = B::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r2 = C::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r3 = D::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r4 = E::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r5 = F::decode(columns.next().expect("find_batch returned fewer columns than items"))?;

                Ok((r0, r1, r2, r3, r4, r5,))
            }
        }
    }
}

#[async_trait::async_trait]
impl<A, B, C, D, E, F, G> FindBatchTuple for (A, B, C, D, E, F, G,)
where
    A: BatchFindable + Send,
    B: BatchFindable + Send,
    C: BatchFindable + Send,
    D: BatchFindable + Send,
    E: BatchFindable + Send,
    F: BatchFindable + Send,
    G: BatchFindable + Send,
{
    type Output = (A::Output, B::Output, C::Output, D::Output, E::Output, F::Output, G::Output,);

    async fn execute_batch(self, client: &DinocoClient) -> anyhow::Result<Self::Output> {
        let (i0, i1, i2, i3, i4, i5, i6,) = self;

        match client.query_mode() {
            QueryMode::BatchQuery => Ok(futures::try_join!(i0.execute_alone(client), i1.execute_alone(client), i2.execute_alone(client), i3.execute_alone(client), i4.execute_alone(client), i5.execute_alone(client), i6.execute_alone(client))?),
            QueryMode::SingleQuery => {
                let (q0, p0) = i0.compile()?;
                let (q1, p1) = i1.compile()?;
                let (q2, p2) = i2.compile()?;
                let (q3, p3) = i3.compile()?;
                let (q4, p4) = i4.compile()?;
                let (q5, p5) = i5.compile()?;
                let (q6, p6) = i6.compile()?;
                let read_primary = p0 || p1 || p2 || p3 || p4 || p5 || p6;
                let mut items = Vec::new();
                items.push(FindBatchItem { query: q0 });
                items.push(FindBatchItem { query: q1 });
                items.push(FindBatchItem { query: q2 });
                items.push(FindBatchItem { query: q3 });
                items.push(FindBatchItem { query: q4 });
                items.push(FindBatchItem { query: q5 });
                items.push(FindBatchItem { query: q6 });

                let columns =
                    client.read_backend(read_primary).find_batch(FindBatchQuery { items }).await?;
                let mut columns = columns.into_iter();

                let r0 = A::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r1 = B::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r2 = C::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r3 = D::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r4 = E::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r5 = F::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r6 = G::decode(columns.next().expect("find_batch returned fewer columns than items"))?;

                Ok((r0, r1, r2, r3, r4, r5, r6,))
            }
        }
    }
}

#[async_trait::async_trait]
impl<A, B, C, D, E, F, G, H> FindBatchTuple for (A, B, C, D, E, F, G, H,)
where
    A: BatchFindable + Send,
    B: BatchFindable + Send,
    C: BatchFindable + Send,
    D: BatchFindable + Send,
    E: BatchFindable + Send,
    F: BatchFindable + Send,
    G: BatchFindable + Send,
    H: BatchFindable + Send,
{
    type Output = (A::Output, B::Output, C::Output, D::Output, E::Output, F::Output, G::Output, H::Output,);

    async fn execute_batch(self, client: &DinocoClient) -> anyhow::Result<Self::Output> {
        let (i0, i1, i2, i3, i4, i5, i6, i7,) = self;

        match client.query_mode() {
            QueryMode::BatchQuery => Ok(futures::try_join!(i0.execute_alone(client), i1.execute_alone(client), i2.execute_alone(client), i3.execute_alone(client), i4.execute_alone(client), i5.execute_alone(client), i6.execute_alone(client), i7.execute_alone(client))?),
            QueryMode::SingleQuery => {
                let (q0, p0) = i0.compile()?;
                let (q1, p1) = i1.compile()?;
                let (q2, p2) = i2.compile()?;
                let (q3, p3) = i3.compile()?;
                let (q4, p4) = i4.compile()?;
                let (q5, p5) = i5.compile()?;
                let (q6, p6) = i6.compile()?;
                let (q7, p7) = i7.compile()?;
                let read_primary = p0 || p1 || p2 || p3 || p4 || p5 || p6 || p7;
                let mut items = Vec::new();
                items.push(FindBatchItem { query: q0 });
                items.push(FindBatchItem { query: q1 });
                items.push(FindBatchItem { query: q2 });
                items.push(FindBatchItem { query: q3 });
                items.push(FindBatchItem { query: q4 });
                items.push(FindBatchItem { query: q5 });
                items.push(FindBatchItem { query: q6 });
                items.push(FindBatchItem { query: q7 });

                let columns =
                    client.read_backend(read_primary).find_batch(FindBatchQuery { items }).await?;
                let mut columns = columns.into_iter();

                let r0 = A::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r1 = B::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r2 = C::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r3 = D::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r4 = E::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r5 = F::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r6 = G::decode(columns.next().expect("find_batch returned fewer columns than items"))?;
                let r7 = H::decode(columns.next().expect("find_batch returned fewer columns than items"))?;

                Ok((r0, r1, r2, r3, r4, r5, r6, r7,))
            }
        }
    }
}
