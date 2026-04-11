use std::collections::HashMap;
use std::future::Future;
use std::marker::PhantomData;
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;

use chrono::{DateTime, Utc};

use dinoco_engine::{DinocoAdapter, DinocoClient, DinocoError, DinocoResult, Expression, SelectStatement};

use futures::FutureExt;

use serde::{Deserialize, Serialize};

use crate::{Model, Projection, ReadMode, UpdateModel, execute_first, execute_many};

const DEFAULT_QUEUE_POLL_INTERVAL_MS: u64 = 1_000;
const DEFAULT_QUEUE_RETRY_DELAY_MS: u64 = 5 * 60 * 1_000;
const QUEUE_JOBS_KEY: &str = "dinoco:queue:jobs";
const QUEUE_PAYLOADS_KEY: &str = "dinoco:queue:payloads";

#[derive(Debug, Clone)]
pub(crate) struct QueueDispatch {
    pub event: String,
    pub execute_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum QueueLookup {
    Conditions { conditions: Vec<Expression> },
    ManyConditions { conditions: Vec<Vec<Expression>> },
    Statement { statement: SelectStatement, take_first: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QueueJob {
    id: String,
    event: String,
    lookup: QueueLookup,
    retry_delay_ms: u64,
}

#[derive(Debug, Clone)]
enum QueueJobControl {
    Complete,
    RetryAt(DateTime<Utc>),
}

pub struct QueueWorkerContext<T, A>
where
    A: DinocoAdapter + Clone,
{
    control: Arc<Mutex<Option<QueueJobControl>>>,
    pub client: DinocoClient<A>,
    pub data: T,
}

pub struct QueueWorkers<A>
where
    A: DinocoAdapter + Clone + Send + Sync + 'static,
{
    handlers: HashMap<String, Box<dyn QueueWorkerHandler<A>>>,
    poll_interval: Duration,
}

#[async_trait]
trait QueueWorkerData<A>: Send + Sized + 'static
where
    A: DinocoAdapter + Clone + Send + Sync + 'static,
{
    async fn load(job: &QueueJob, client: &DinocoClient<A>) -> DinocoResult<Option<Self>>;
}

#[async_trait]
trait QueueWorkerHandler<A>: Send + Sync
where
    A: DinocoAdapter + Clone + Send + Sync + 'static,
{
    async fn handle(&self, job: &QueueJob, client: &DinocoClient<A>) -> DinocoResult<QueueJobControl>;
    fn clone_box(&self) -> Box<dyn QueueWorkerHandler<A>>;
}

struct TypedQueueWorkerHandler<A, T, F> {
    handler: F,
    marker: PhantomData<fn() -> (A, T)>,
}

impl<A> Clone for Box<dyn QueueWorkerHandler<A>>
where
    A: DinocoAdapter + Clone + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

impl<A, T, F> Clone for TypedQueueWorkerHandler<A, T, F>
where
    F: Clone,
{
    fn clone(&self) -> Self {
        Self { handler: self.handler.clone(), marker: PhantomData }
    }
}

pub fn workers<A>() -> QueueWorkers<A>
where
    A: DinocoAdapter + Clone + Send + Sync + 'static,
{
    QueueWorkers::new()
}

impl QueueDispatch {
    pub(crate) fn immediate(event: impl Into<String>) -> Self {
        Self { event: event.into(), execute_at: Utc::now() }
    }

    pub(crate) fn in_milliseconds(event: impl Into<String>, delay_ms: u64) -> Self {
        let delay_ms = i64::try_from(delay_ms).unwrap_or(i64::MAX);

        Self { event: event.into(), execute_at: Utc::now() + chrono::Duration::milliseconds(delay_ms) }
    }

    pub(crate) fn at(event: impl Into<String>, execute_at: DateTime<Utc>) -> Self {
        Self { event: event.into(), execute_at }
    }
}

impl<T, A> QueueWorkerContext<T, A>
where
    A: DinocoAdapter + Clone,
{
    pub fn success(&self) {
        self.set_control(QueueJobControl::Complete);
    }

    pub fn remove(&self) {
        self.set_control(QueueJobControl::Complete);
    }

    pub fn fail(&self) {
        self.retry_in(DEFAULT_QUEUE_RETRY_DELAY_MS);
    }

    pub fn retry_in(&self, delay_ms: u64) {
        let delay_ms = i64::try_from(delay_ms).unwrap_or(i64::MAX);
        let execute_at = Utc::now() + chrono::Duration::milliseconds(delay_ms);

        self.set_control(QueueJobControl::RetryAt(execute_at));
    }

    pub fn retry_at(&self, execute_at: DateTime<Utc>) {
        self.set_control(QueueJobControl::RetryAt(execute_at));
    }

    fn set_control(&self, control: QueueJobControl) {
        let mut current = self.control.lock().expect("queue worker control mutex poisoned");
        *current = Some(control);
    }
}

impl<A> QueueWorkers<A>
where
    A: DinocoAdapter + Clone + Send + Sync + 'static,
{
    pub fn new() -> Self {
        Self { handlers: HashMap::new(), poll_interval: Duration::from_millis(DEFAULT_QUEUE_POLL_INTERVAL_MS) }
    }

    pub fn with_poll_interval(mut self, poll_interval: Duration) -> Self {
        self.poll_interval = poll_interval;

        self
    }

    #[allow(private_bounds)]
    pub fn on<T, F, Fut>(mut self, event: impl Into<String>, handler: F) -> Self
    where
        T: QueueWorkerData<A>,
        F: Fn(QueueWorkerContext<T, A>) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.handlers
            .insert(event.into(), Box::new(TypedQueueWorkerHandler::<A, T, F> { handler, marker: PhantomData }));

        self
    }

    pub async fn run(&self) -> DinocoResult<tokio::task::JoinHandle<DinocoResult<()>>> {
        let worker_client = DinocoClient::<A>::registered_worker_client().await?;
        let handlers = self.handlers.clone();
        let poll_interval = self.poll_interval;

        Ok(tokio::spawn(async move {
            loop {
                let processed = match AssertUnwindSafe(process_next_job(&handlers, &worker_client)).catch_unwind().await {
                    Ok(Ok(processed)) => processed,
                    Ok(Err(_)) | Err(_) => {
                        tokio::time::sleep(poll_interval).await;
                        continue;
                    }
                };

                if !processed {
                    tokio::time::sleep(poll_interval).await;
                }
            }
        }))
    }
}

impl<A> Clone for QueueWorkers<A>
where
    A: DinocoAdapter + Clone + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self { handlers: self.handlers.clone(), poll_interval: self.poll_interval }
    }
}

#[async_trait]
impl<M, A> QueueWorkerData<A> for M
where
    M: Model + Projection<M> + Send + 'static,
    A: DinocoAdapter + Clone + Send + Sync + 'static,
{
    async fn load(job: &QueueJob, client: &DinocoClient<A>) -> DinocoResult<Option<Self>> {
        load_single_model::<M, A>(&job.lookup, client).await
    }
}

#[async_trait]
impl<M, A> QueueWorkerData<A> for Option<M>
where
    M: Model + Projection<M> + Send + 'static,
    A: DinocoAdapter + Clone + Send + Sync + 'static,
{
    async fn load(job: &QueueJob, client: &DinocoClient<A>) -> DinocoResult<Option<Self>> {
        load_single_model::<M, A>(&job.lookup, client).await.map(|item| item.map(Some))
    }
}

#[async_trait]
impl<M, A> QueueWorkerData<A> for Vec<M>
where
    M: Model + Projection<M> + Send + 'static,
    A: DinocoAdapter + Clone + Send + Sync + 'static,
{
    async fn load(job: &QueueJob, client: &DinocoClient<A>) -> DinocoResult<Option<Self>> {
        let rows = match &job.lookup {
            QueueLookup::Conditions { conditions } => {
                let Some(item) = load_single_model_by_conditions::<M, A>(conditions.clone(), client).await? else {
                    return Ok(None);
                };

                vec![item]
            }
            QueueLookup::ManyConditions { conditions } => {
                let mut rows = Vec::with_capacity(conditions.len());

                for item_conditions in conditions {
                    if let Some(item) = load_single_model_by_conditions::<M, A>(item_conditions.clone(), client).await?
                    {
                        rows.push(item);
                    }
                }

                rows
            }
            QueueLookup::Statement { statement, .. } => {
                let statement = build_projection_statement::<M>(statement.clone());
                execute_many::<M, M, A>(statement, &[], &[], ReadMode::Primary, client).await?
            }
        };

        if rows.is_empty() { Ok(None) } else { Ok(Some(rows)) }
    }
}

#[async_trait]
impl<A, T, F, Fut> QueueWorkerHandler<A> for TypedQueueWorkerHandler<A, T, F>
where
    A: DinocoAdapter + Clone + Send + Sync + 'static,
    T: QueueWorkerData<A>,
    F: Fn(QueueWorkerContext<T, A>) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    async fn handle(&self, job: &QueueJob, client: &DinocoClient<A>) -> DinocoResult<QueueJobControl> {
        let Some(data) = T::load(job, client).await? else {
            return Ok(QueueJobControl::Complete);
        };

        let control = Arc::new(Mutex::new(None));
        let context = QueueWorkerContext { control: control.clone(), client: client.clone(), data };

        (self.handler)(context).await;

        Ok(control.lock().expect("queue worker control mutex poisoned").clone().unwrap_or(QueueJobControl::Complete))
    }

    fn clone_box(&self) -> Box<dyn QueueWorkerHandler<A>> {
        Box::new(self.clone())
    }
}

pub(crate) async fn enqueue_find_statement<A>(
    client: &DinocoClient<A>,
    dispatch: &QueueDispatch,
    statement: SelectStatement,
    take_first: bool,
) -> DinocoResult<()>
where
    A: DinocoAdapter,
{
    enqueue_job(client, dispatch, QueueLookup::Statement { statement, take_first }).await
}

pub(crate) async fn enqueue_single_conditions<A>(
    client: &DinocoClient<A>,
    dispatch: &QueueDispatch,
    conditions: Vec<Expression>,
) -> DinocoResult<()>
where
    A: DinocoAdapter,
{
    enqueue_job(client, dispatch, QueueLookup::Conditions { conditions }).await
}

pub(crate) async fn enqueue_many_conditions<A>(
    client: &DinocoClient<A>,
    dispatch: &QueueDispatch,
    conditions: Vec<Vec<Expression>>,
) -> DinocoResult<()>
where
    A: DinocoAdapter,
{
    enqueue_job(client, dispatch, QueueLookup::ManyConditions { conditions }).await
}

pub(crate) fn dispatch_insert_lookup<M>(item: &M) -> Vec<Expression>
where
    M: crate::InsertModel,
{
    item.insert_identity_conditions()
}

pub(crate) fn dispatch_update_lookup<M>(item: &M, conditions: &[Expression]) -> Vec<Expression>
where
    M: UpdateModel,
{
    let mut output = item.update_identity_conditions();
    output.extend(conditions.iter().cloned());

    output
}

fn build_projection_statement<M>(mut statement: SelectStatement) -> SelectStatement
where
    M: Model + Projection<M>,
{
    statement.select = M::columns().iter().map(|column| (*column).to_string()).collect();
    statement.from = M::table_name().to_string();

    statement
}

async fn enqueue_job<A>(client: &DinocoClient<A>, dispatch: &QueueDispatch, lookup: QueueLookup) -> DinocoResult<()>
where
    A: DinocoAdapter,
{
    let cache = client.cache_store().ok_or_else(missing_queue_error)?;
    let job = QueueJob {
        id: crate::Uuid::now_v7().to_string(),
        event: dispatch.event.clone(),
        lookup,
        retry_delay_ms: DEFAULT_QUEUE_RETRY_DELAY_MS,
    };

    cache.hash_set(QUEUE_PAYLOADS_KEY, &job.id, &job).await?;
    schedule_job(client, job, dispatch.execute_at).await
}

async fn pop_due_job<A>(client: &DinocoClient<A>) -> DinocoResult<Option<QueueJob>>
where
    A: DinocoAdapter,
{
    let cache = client.cache_store().ok_or_else(missing_queue_error)?;
    let now = Utc::now().timestamp_millis();

    loop {
        let Some(job_id) = cache.sorted_set_pop_min_by_score(QUEUE_JOBS_KEY, now).await? else {
            return Ok(None);
        };

        let job = cache.hash_get::<QueueJob>(QUEUE_PAYLOADS_KEY, &job_id).await?;
        cache.hash_delete(QUEUE_PAYLOADS_KEY, &job_id).await?;

        if let Some(job) = job {
            return Ok(Some(job));
        }
    }
}

async fn process_next_job<A>(
    handlers: &HashMap<String, Box<dyn QueueWorkerHandler<A>>>,
    client: &DinocoClient<A>,
) -> DinocoResult<bool>
where
    A: DinocoAdapter + Clone + Send + Sync + 'static,
{
    let Some(job) = pop_due_job(client).await? else {
        return Ok(false);
    };

    let Some(handler) = handlers.get(&job.event) else {
        return Ok(true);
    };

    let control = match AssertUnwindSafe(handler.handle(&job, client)).catch_unwind().await {
        Ok(Ok(control)) => control,
        Ok(Err(_)) | Err(_) => QueueJobControl::RetryAt(default_retry_at(job.retry_delay_ms)),
    };

    if let QueueJobControl::RetryAt(execute_at) = control {
        schedule_job(client, job, execute_at).await?;
    }

    Ok(true)
}

async fn schedule_job<A>(client: &DinocoClient<A>, job: QueueJob, execute_at: DateTime<Utc>) -> DinocoResult<()>
where
    A: DinocoAdapter,
{
    let cache = client.cache_store().ok_or_else(missing_queue_error)?;

    cache.hash_set(QUEUE_PAYLOADS_KEY, &job.id, &job).await?;
    cache.sorted_set_add(QUEUE_JOBS_KEY, &job.id, execute_at.timestamp_millis()).await
}

async fn load_single_model<M, A>(lookup: &QueueLookup, client: &DinocoClient<A>) -> DinocoResult<Option<M>>
where
    M: Model + Projection<M>,
    A: DinocoAdapter + Clone + Send + Sync + 'static,
{
    match lookup {
        QueueLookup::Conditions { conditions } => {
            load_single_model_by_conditions::<M, A>(conditions.clone(), client).await
        }
        QueueLookup::ManyConditions { conditions } => {
            let Some(first_conditions) = conditions.first() else {
                return Ok(None);
            };

            load_single_model_by_conditions::<M, A>(first_conditions.clone(), client).await
        }
        QueueLookup::Statement { statement, take_first } => {
            let statement = build_projection_statement::<M>(statement.clone());

            if *take_first {
                execute_first::<M, M, A>(statement.limit(1), ReadMode::Primary, client).await
            } else {
                let mut rows = execute_many::<M, M, A>(statement, &[], &[], ReadMode::Primary, client).await?;

                Ok(rows.drain(..).next())
            }
        }
    }
}

async fn load_single_model_by_conditions<M, A>(
    conditions: Vec<Expression>,
    client: &DinocoClient<A>,
) -> DinocoResult<Option<M>>
where
    M: Model + Projection<M>,
    A: DinocoAdapter + Clone + Send + Sync + 'static,
{
    let mut statement = SelectStatement::new().from(M::table_name()).select(M::columns());

    for condition in conditions {
        statement = statement.condition(condition);
    }

    execute_first::<M, M, A>(statement, ReadMode::Primary, client).await
}

fn default_retry_at(delay_ms: u64) -> DateTime<Utc> {
    let delay_ms = i64::try_from(delay_ms).unwrap_or(i64::MAX);

    Utc::now() + chrono::Duration::milliseconds(delay_ms)
}

fn missing_queue_error() -> DinocoError {
    DinocoError::ConnectionError("Redis is not configured for Dinoco queues.".to_string())
}
