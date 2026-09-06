//! A hand-written `tower` middleware: `LogLayer` wraps any service and prints
//! one line per request with its method, path, status and elapsed time.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use hyper::{Request, Response};
use tower::{Layer, Service};

#[derive(Clone, Copy, Default)]
pub struct LogLayer;

impl<S> Layer<S> for LogLayer {
    type Service = LogService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        LogService { inner }
    }
}

#[derive(Clone)]
pub struct LogService<S> {
    inner: S,
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for LogService<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        let method = request.method().clone();
        let path = request.uri().path().to_string();

        // `poll_ready` was called on `self.inner`, so clone-and-swap to move the
        // readied service into the future and leave a fresh clone behind.
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);

        Box::pin(async move {
            let started = Instant::now();
            let result = inner.call(request).await;

            if let Ok(response) = &result {
                println!("{method} {path} -> {} ({} ms)", response.status(), started.elapsed().as_millis());
            }

            result
        })
    }
}
