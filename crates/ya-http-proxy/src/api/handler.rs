use futures::{stream, StreamExt};
use hyper::{Body, Request, Response, StatusCode};
use routerify::prelude::RequestExt;

use crate::api::ApiErrorKind;
use crate::proxy::ProxyManager;
use crate::UserError;
use ya_http_proxy_model as model;

type HandlerResult = Result<Response<Body>, ApiErrorKind>;

/// Lists services
pub async fn get_services(req: Request<Body>) -> HandlerResult {
    log::debug!("get_services");
    let manager: &ProxyManager = req.data().unwrap();
    let proxies = manager.proxies();

    let vec: Vec<model::Service> = Default::default();
    let vec = stream::iter(proxies.read().await.values())
        .fold(vec, |mut vec, proxy| async move {
            let state = proxy.state.read().await;
            vec.extend(state.by_endpoint.values().map(model::Service::from));
            vec
        })
        .await;

    log::debug!("Get services returned: {:?}", vec);
    Response::object(&vec)
}

/// Creates a new service
pub async fn post_services(req: Request<Body>) -> HandlerResult {
    log::debug!("post_services 1");
    let (parts, body) = req.into_parts();
    log::debug!("post_services 2");
    let manager: &ProxyManager = parts.data().unwrap();
    log::debug!("post_services 3");
    let body = hyper::body::to_bytes(body).await?;

    log::debug!("post_services 4");
    let mut create: model::CreateService = serde_json::from_slice(body.as_ref()).map_err(
        |e| {
            log::error!("Failed to parse request body: {}", e);
            e
        },
    )?;
    log::debug!("post_services 5");
    let proxy = manager.get_or_spawn(&mut create).await.map_err(
        |e| {
            log::error!("Failed to create service: {}", e);
            e
        },
    )?;
    log::debug!("post_services 6");
    let service: model::Service = proxy.add(create).await?;

    log::debug!("post_services returned {:?}", service);
    Response::object(&service)
}

/// Retrieves a single service
pub async fn get_service(req: Request<Body>) -> HandlerResult {
    let service_name = req.param("service").unwrap();
    let manager: &ProxyManager = req.data().unwrap();

    let proxy = manager.proxy(service_name).await?;
    let service = proxy.get::<model::Service>(service_name).await?;

    Response::object(&service)
}

/// Removes a service
pub async fn delete_service(req: Request<Body>) -> HandlerResult {
    let service_name = req.param("service").unwrap();
    let manager: &ProxyManager = req.data().unwrap();

    let proxy = manager.proxy(service_name).await?;
    proxy.remove(service_name).await?;

    Response::object(&())
}

/// Lists service users
pub async fn get_users(req: Request<Body>) -> HandlerResult {
    let service_name = req.param("service").unwrap();
    let manager: &ProxyManager = req.data().unwrap();

    let proxy = manager.proxy(service_name).await?;
    let vec = proxy
        .get_users(service_name)
        .await?
        .into_iter()
        .map(|u| model::User {
            username: u.username,
            created_at: u.created_at,
        })
        .collect::<Vec<_>>();

    Response::object(&vec)
}

/// Creates a new service user
pub async fn post_users(req: Request<Body>) -> HandlerResult {
    let (parts, body) = req.into_parts();
    let manager: &ProxyManager = parts.data().unwrap();
    let body = hyper::body::to_bytes(body).await?;

    let service_name = parts.param("service").unwrap();
    let create: model::CreateUser = serde_json::from_slice(body.as_ref())?;

    let proxy = manager.proxy(service_name).await?;
    let user = proxy
        .add_user(service_name, create.username, create.password)
        .await?;

    Response::object(&model::User {
        username: user.username,
        created_at: user.created_at,
    })
}

/// Retrieves a single service user
pub async fn get_user(req: Request<Body>) -> HandlerResult {
    let service_name = req.param("service").unwrap();
    let username = req.param("user").unwrap();
    let manager: &ProxyManager = req.data().unwrap();

    let proxy = manager.proxy(service_name).await?;
    let user = proxy.get_user(service_name, username).await?;

    Response::object(&model::User {
        username: user.username,
        created_at: user.created_at,
    })
}

/// Removes a service user
pub async fn delete_user(req: Request<Body>) -> HandlerResult {
    let service_name = req.param("service").unwrap();
    let username = req.param("user").unwrap();
    let manager: &ProxyManager = req.data().unwrap();

    let proxy = manager.proxy(service_name).await?;
    proxy.remove_user(service_name, username).await?;

    Response::object(&())
}

/// Retrieves service user stats
pub async fn get_user_stats(req: Request<Body>) -> HandlerResult {
    let service_name = req.param("service").unwrap();
    let username = req.param("user").unwrap();
    let manager: &ProxyManager = req.data().unwrap();

    let proxy = manager.proxy(service_name).await?;
    let stats = proxy.stats.read().await;
    let requests = stats
        .user
        .get(username)
        .copied()
        .ok_or_else(|| UserError::NotFound(username.to_string()))?;

    Response::object(&model::UserStats { requests })
}

/// Retrieves service user stats per endpoint called
pub async fn get_user_endpoint_stats(req: Request<Body>) -> HandlerResult {
    let service_name = req.param("service").unwrap();
    let username = req.param("user").unwrap();
    let manager: &ProxyManager = req.data().unwrap();

    let proxy = manager.proxy(service_name).await?;
    let stats = proxy.stats.read().await;
    let endpoint_requests = stats
        .user_endpoint
        .get(username)
        .ok_or_else(|| UserError::NotFound(username.to_string()))?;

    Response::object(&model::UserEndpointStats(endpoint_requests.clone()))
}

/// Shuts down the proxy
pub async fn post_shutdown(req: Request<Body>) -> HandlerResult {
    let manager: &ProxyManager = req.data().unwrap();
    manager.stop().await;

    Response::object(&())
}

trait ResponseExt<B, E> {
    fn object<T>(t: &T) -> Result<Response<B>, E>
    where
        T: serde::Serialize;
}

impl<B, E> ResponseExt<B, E> for Response<B>
where
    B: From<String>,
    E: From<ApiErrorKind> + From<hyper::http::Error>,
{
    fn object<T>(t: &T) -> Result<Response<B>, E>
    where
        T: serde::Serialize,
    {
        let ser = serde_json::to_string(&t)
            .map_err(|e| ApiErrorKind::InternalServerError(e.to_string()))?;
        let res = Response::builder()
            .header("Content-Type", "application/json")
            .status(StatusCode::OK)
            .body(B::from(ser))?;
        Ok(res)
    }
}
