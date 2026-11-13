//! What a service is, and the list of those there are.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use bytes::Bytes;
use hyper::{HeaderMap, Method};

use crate::protocol::{Fault, Reply};
use crate::route::Route;
use crate::store::Store;

/// One request to a service.
#[derive(Debug, Clone)]
pub struct Call {
    pub method: Method,
    /// The path, percent-encoded as it was sent.
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub route: Route,
    pub request_id: String,
}

/// What a service's `handle` returns, boxed so services can be listed
/// together.
pub type Handled<'a> = Pin<Box<dyn Future<Output = Result<Reply, Fault>> + Send + 'a>>;

/// One emulated service, e.g. S3.
pub trait Service: Send + Sync {
    /// Its short name, as in an ARN and in the request log: `s3`.
    fn name(&self) -> &'static str;

    /// Its name for people: `S3`.
    fn title(&self) -> &'static str;

    /// The names requests for it are signed with. Usually just its name.
    fn signing_names(&self) -> &'static [&'static str];

    /// The first part of `X-Amz-Target` in its requests, if it uses one:
    /// `DynamoDB_20120810`.
    fn target_prefixes(&self) -> &'static [&'static str] {
        &[]
    }

    /// The operations it supports, for the docs and the app.
    fn operations(&self) -> &'static [&'static str];

    /// Sets up its tables. Called at start, and again after a reset.
    fn init(&self, _store: &Store) -> Result<(), String> {
        Ok(())
    }

    /// Whether it takes requests that name no service at all (not
    /// signed, no target, no action), as S3 does for public reads.
    fn takes_unsigned(&self) -> bool {
        false
    }

    /// Whether this request's body should reach the service as it
    /// arrives, not read whole first: an upload that may be large.
    fn streams(&self, _method: &Method, _headers: &HeaderMap) -> bool {
        false
    }

    fn handle<'a>(&'a self, call: Call, store: &'a Store) -> Handled<'a>;

    /// Handles a request for which [`Service::streams`] said yes; `call`
    /// has no body, `upload` is it.
    fn handle_stream<'a>(&'a self, _call: Call, _upload: Upload, _store: &'a Store) -> Handled<'a> {
        Box::pin(async { Err(Fault::internal("this service takes no streamed bodies")) })
    }
}

/// A request body, read piece by piece.
pub type Upload = hyper::body::Incoming;

/// The services this emulator has.
#[derive(Clone, Default)]
pub struct Registry {
    services: Vec<Arc<dyn Service>>,
}

impl Registry {
    /// Every service ferrocloud comes with.
    pub fn standard() -> Self {
        Self::default()
            .with(crate::sts::Sts)
            .with(crate::iam::Iam)
            .with(crate::s3::S3)
    }

    pub fn with(mut self, service: impl Service + 'static) -> Self {
        self.services.push(Arc::new(service));
        self
    }

    pub fn services(&self) -> &[Arc<dyn Service>] {
        &self.services
    }

    /// The service a request is for: by what it was signed for, else by
    /// its target, else the one that takes unsigned requests.
    pub fn find(&self, route: &Route) -> Option<&Arc<dyn Service>> {
        let by_signing = route.signing_name.as_deref().and_then(|name| {
            self.services
                .iter()
                .find(|s| s.signing_names().contains(&name))
        });
        let by_target = || {
            let prefix = route.target_prefix.as_deref()?;
            self.services
                .iter()
                .find(|s| s.target_prefixes().contains(&prefix))
        };
        // Naming nothing at all, it's for whoever takes such requests.
        let unsigned = || {
            (route.signing_name.is_none()
                && route.target_prefix.is_none()
                && route.operation.is_none())
            .then(|| self.services.iter().find(|s| s.takes_unsigned()))
            .flatten()
        };
        by_signing.or_else(by_target).or_else(unsigned)
    }

    pub fn init(&self, store: &Store) -> Result<(), String> {
        self.services.iter().try_for_each(|s| s.init(store))
    }
}
