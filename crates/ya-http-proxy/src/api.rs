use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

use hyper::http::response::Builder;
use hyper::server::conn::AddrIncoming;
use hyper::{Body, Request, Response, Server, StatusCode};
use routerify::prelude::*;
use routerify::{Middleware, RouteError, Router, RouterService};

use crate::error::{Error, ProxyError, ServiceError, UserError};
use crate::proxy::ProxyManager;
use crate::ManagementError;
use ya_http_proxy_model as model;

mod handler;

pub type HandlerError = ApiErrorKind;
pub type ApiServer = Server<AddrIncoming, RouterService<Body, HandlerError>>;

pub struct Management {
    server: Option<ApiServer>,
    pub(self) manager: ProxyManager,
}

impl Management {
    pub fn new(manager: ProxyManager) -> Self {
        Self {
            server: None,
            manager,
        }
    }

    pub fn bind(&mut self, address: SocketAddr) -> Result<(), Error> {
        let router = router(self.manager.clone()).map_err(ProxyError::rt)?;
        let service = RouterService::new(router).unwrap();

        let server = Server::try_bind(&address)
            .map_err(|e| ManagementError::Bind {
                address,
                message: e.to_string(),
            })?
            .serve(service);
        self.server.replace(server);

        Ok(())
    }

    pub fn local_addr(&self) -> Result<SocketAddr, Error> {
        self.server
            .as_ref()
            .map(|s| s.local_addr())
            .ok_or_else(|| ManagementError::NotRunning.into())
    }
}

impl Future for Management {
    type Output = Result<(), Error>;

    #[inline]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.server.as_mut() {
            Some(server) => Pin::new(server).poll(cx).map_err(Error::other),
            None => Poll::Ready(Err(ManagementError::NotRunning.into())),
        }
    }
}

fn router(manager: ProxyManager) -> routerify::Result<Router<Body, HandlerError>> {
    use handler::*;

    let mut builder = Router::builder()
        .data(manager)
        .middleware(Middleware::pre(middleware_logger));

    builder = builder
        .get("/services", get_services)
        .post("/services", post_services)
        .get("/services/:service", get_service)
        .delete("/services/:service", delete_service)
        .get("/services/:service/users", get_users)
        .post("/services/:service/users", post_users)
        .get("/services/:service/users/:user", get_user)
        .delete("/services/:service/users/:user", delete_user)
        .get("/services/:service/users/:user/stats", get_user_stats)
        .get(
            "/services/:service/users/:user/endpoints/stats",
            get_user_endpoint_stats,
        );

    builder.err_handler(err_handler).build()
}

async fn middleware_logger(req: Request<Body>) -> Result<Request<Body>, HandlerError> {
    log::debug!(
        "{} {} {}",
        req.remote_addr(),
        req.method(),
        req.uri().path()
    );
    Ok(req)
}

async fn err_handler(err: RouteError) -> Response<Body> {
    let builder = Response::builder();

    match err.downcast::<ApiErrorKind>() {
        Ok(err) => match *err {
            ApiErrorKind::BadRequest(err) => err_response(builder, StatusCode::BAD_REQUEST, err),
            ApiErrorKind::Conflict(err) => err_response(builder, StatusCode::CONFLICT, err),
            ApiErrorKind::InternalServerError(err) => {
                err_response(builder, StatusCode::INTERNAL_SERVER_ERROR, err)
            }
        },
        Err(err) => err_response(builder, StatusCode::INTERNAL_SERVER_ERROR, err),
    }
}

fn err_response(builder: Builder, code: StatusCode, msg: impl ToString) -> Response<Body> {
    match serde_json::to_string(&model::ErrorResponse {
        message: msg.to_string(),
    }) {
        Ok(ser) => builder.status(code).body(Body::from(ser)),
        Err(err) => builder
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(err.to_string())),
    }
    .unwrap()
}

#[derive(thiserror::Error, Debug)]
pub enum ApiErrorKind {
    #[error("Bad request: {}", .0.to_string())]
    BadRequest(Error),
    #[error("Conflict: {}", .0.to_string())]
    Conflict(Error),
    #[error("Internal server error {0}")]
    InternalServerError(String),
}

impl<T> From<T> for ApiErrorKind
where
    Error: From<T>,
{
    fn from(e: T) -> Self {
        match Error::from(e) {
            e @ Error::Proxy(ProxyError::AlreadyExists(_)) => Self::Conflict(e),
            e @ Error::Service(ServiceError::AlreadyExists { .. }) => Self::Conflict(e),
            e @ Error::User(UserError::AlreadyExists(_)) => Self::Conflict(e),
            e => Self::BadRequest(e),
        }
    }
}

impl From<hyper::Error> for ApiErrorKind {
    fn from(e: hyper::Error) -> Self {
        Self::InternalServerError(e.to_string())
    }
}

impl From<hyper::http::Error> for ApiErrorKind {
    fn from(e: hyper::http::Error) -> Self {
        Self::InternalServerError(e.to_string())
    }
}

impl From<serde_json::Error> for ApiErrorKind {
    fn from(e: serde_json::Error) -> Self {
        Self::BadRequest(Error::other(e))
    }
}

impl From<Box<serde_json::Error>> for ApiErrorKind {
    fn from(e: Box<serde_json::Error>) -> Self {
        Self::BadRequest(Error::other(e))
    }
}
