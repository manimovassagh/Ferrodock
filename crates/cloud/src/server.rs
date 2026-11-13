//! Accepts connections and turns each request into a call on a service,
//! then records how it went.

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;

use bytes::Bytes;
use http_body_util::{BodyExt, Limited};
use hyper::body::{Frame, Incoming};
use hyper::service::service_fn;
use hyper::{Request, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use crate::admin;
use crate::capture::{self, Detail};
use crate::clock;
use crate::events::{Event, Events};
use crate::protocol::{Fault, Protocol, Reply, request_id};
use crate::route::{parse_pairs, route};
use crate::service::{Call, Registry};
use crate::store::Store;

/// The most a request's body may be while bodies are read whole. Services
/// that take large uploads (S3) read theirs as a stream instead.
const MAX_BODY: usize = 64 * 1024 * 1024;

/// Everything a request needs: the services, the data, the record.
pub struct App {
    pub registry: Registry,
    pub store: Store,
    pub events: Events,
}

impl App {
    pub fn new(registry: Registry, store: Store) -> Result<Arc<Self>, String> {
        registry.init(&store)?;
        Ok(Arc::new(Self {
            registry,
            store,
            events: Events::default(),
        }))
    }
}

pub type Response = hyper::Response<Body>;

/// A response body: all there, or arriving in pieces (the request feed).
pub enum Body {
    Full(Option<Bytes>),
    Stream(mpsc::Receiver<Bytes>),
}

impl Body {
    pub fn full(bytes: Bytes) -> Self {
        Body::Full(Some(bytes))
    }

    pub fn stream(pieces: mpsc::Receiver<Bytes>) -> Self {
        Body::Stream(pieces)
    }
}

impl hyper::body::Body for Body {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        match self.get_mut() {
            Body::Full(bytes) => Poll::Ready(bytes.take().map(|b| Ok(Frame::data(b)))),
            Body::Stream(pieces) => pieces.poll_recv(cx).map(|p| p.map(|b| Ok(Frame::data(b)))),
        }
    }

    // A body that's all there is sent with its length, not in chunks.
    fn is_end_stream(&self) -> bool {
        matches!(self, Body::Full(None))
    }

    fn size_hint(&self) -> hyper::body::SizeHint {
        match self {
            Body::Full(bytes) => {
                hyper::body::SizeHint::with_exact(bytes.as_ref().map_or(0, |b| b.len() as u64))
            }
            Body::Stream(_) => hyper::body::SizeHint::default(),
        }
    }
}

/// Serves requests on `listener` until `stop` completes.
pub async fn serve(listener: TcpListener, app: Arc<App>, stop: impl Future<Output = ()>) {
    let mut stop = std::pin::pin!(stop);
    loop {
        let accepted = tokio::select! {
            accepted = listener.accept() => accepted,
            () = &mut stop => return,
        };
        let Ok((stream, _)) = accepted else {
            // Out of file descriptors, most likely: not straight round
            // again, which would spin a core.
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            continue;
        };
        let app = app.clone();
        tokio::spawn(async move {
            let service = service_fn(move |request| {
                let app = app.clone();
                async move { Ok::<_, Infallible>(handle(&app, request).await) }
            });
            // A client that goes away mid-request is its own business.
            let _ = hyper::server::conn::http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .await;
        });
    }
}

async fn handle(app: &Arc<App>, request: Request<Incoming>) -> Response {
    let started = Instant::now();
    let time = clock::now_millis();
    let (head, body) = request.into_parts();
    let path = head.uri.path().to_owned();
    if path.starts_with(admin::PREFIX) {
        return admin::handle(app, &head.method, &path, head.uri.query());
    }

    let query = parse_pairs(head.uri.query().unwrap_or_default());
    let id = request_id();
    let body = match Limited::new(body, MAX_BODY).collect().await {
        Ok(body) => body.to_bytes(),
        Err(_) => {
            let fault = Fault::new(
                413,
                "RequestEntityTooLarge",
                "the request body is too large",
            );
            return respond(fault.render(Protocol::RestXml, &id), &id);
        }
    };
    let route = route(&head.headers, &query, &body);
    let user_agent = head
        .headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();

    // For the request log, before the service takes them.
    let mut detail = Detail {
        request_headers: capture::headers(&head.headers),
        request_body: capture::body(&body),
        ..Detail::default()
    };
    let resource = capture::resource(route.protocol, &query, &body);

    let service = app.registry.find(&route).cloned();
    let outcome = match &service {
        Some(service) => {
            let call = Call {
                method: head.method.clone(),
                path: path.clone(),
                query,
                headers: head.headers,
                body,
                route: route.clone(),
                request_id: id.clone(),
            };
            service.handle(call, &app.store).await
        }
        None => Err(unsupported(&route)),
    };
    let (reply, error, message) = match outcome {
        Ok(reply) => (reply, String::new(), String::new()),
        Err(fault) => (fault.render(route.protocol, &id), fault.code, fault.message),
    };
    detail.response_headers.clone_from(&reply.headers);
    detail.response_body = capture::body(&reply.body);
    let event = Event {
        id: 0,
        time,
        // A service that isn't here yet is still named, as it was asked for.
        service: service
            .map(|s| s.name().to_owned())
            .or(route.signing_name)
            .unwrap_or_default(),
        operation: route
            .operation
            .or(reply.operation.map(str::to_owned))
            .unwrap_or_default(),
        resource,
        method: head.method.to_string(),
        path,
        status: reply.status,
        error,
        message,
        duration_ms: started.elapsed().as_millis() as u64,
        region: route.region,
        user_agent,
        request_id: id.clone(),
    };
    app.events.record(event, detail);
    respond(reply, &id)
}

/// The error for a request no service here takes, worded for whoever
/// reads the tool's output.
fn unsupported(route: &crate::route::Route) -> Fault {
    let what = match (&route.signing_name, &route.target_prefix) {
        (Some(name), _) => format!("the service '{name}'"),
        (None, Some(prefix)) => format!("the service behind '{prefix}'"),
        (None, None) => "this request".to_owned(),
    };
    let message =
        format!("ferrocloud doesn't support {what} yet. What it does: /_ferrocloud/services");
    match route.protocol {
        Protocol::Json10 | Protocol::Json11 => {
            Fault::bad_request("UnknownOperationException", message)
        }
        Protocol::Query => Fault::bad_request("InvalidAction", message),
        Protocol::RestXml | Protocol::RestJson => Fault::new(501, "NotImplemented", message),
    }
}

fn respond(reply: Reply, request_id: &str) -> Response {
    let mut response = hyper::Response::builder()
        .status(StatusCode::from_u16(reply.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR))
        .header("server", "ferrocloud")
        .header("x-amzn-requestid", request_id)
        .header("x-amz-request-id", request_id);
    for (name, value) in &reply.headers {
        response = response.header(name, value);
    }
    response.body(Body::full(reply.body)).unwrap_or_else(|_| {
        // A header a service set wasn't valid HTTP.
        let mut broken = hyper::Response::new(Body::full(Bytes::new()));
        *broken.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
        broken
    })
}
