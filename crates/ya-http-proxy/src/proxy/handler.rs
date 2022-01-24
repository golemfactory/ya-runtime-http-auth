use std::net::SocketAddr;
use std::sync::Arc;

use hyper::client::HttpConnector;
use hyper::header::{self, HeaderName, HeaderValue};
use hyper::{Body, Client, HeaderMap, Request, Response, StatusCode};
use tokio::sync::RwLock;

use crate::proxy::{ProxyState, ProxyStats};

#[inline(always)]
pub async fn forward_req(
    mut req: Request<Body>,
    proxy_state: Arc<RwLock<ProxyState>>,
    proxy_stats: Arc<RwLock<ProxyStats>>,
    client: Client<HttpConnector>,
    address: SocketAddr,
) -> hyper::Result<hyper::Response<hyper::Body>> {
    let path = req.uri().path();
    let state = proxy_state.read().await;

    // Check whether the service is registered
    let service = match state
        .by_endpoint
        .iter()
        .find(|(e, _)| path.starts_with(e.as_str()))
    {
        Some((_, service)) => service,
        None => return response(StatusCode::NOT_FOUND),
    };
    // Extract credentials from header
    let auth = match extract_basic_auth(req.headers()) {
        Ok(auth) => auth,
        Err(_) => return response(StatusCode::UNAUTHORIZED),
    };
    // Authorize user
    if !service.access.contains(auth) {
        return response(StatusCode::UNAUTHORIZED);
    }

    let proxy_to = service.created_with.to.clone();
    drop(state);

    // Decode credentials
    let decoded_auth = match decode_base64(auth) {
        Ok(cred) => cred,
        Err(_) => return response(StatusCode::UNAUTHORIZED),
    };
    let username = match extract_username(&decoded_auth) {
        Ok(cred) => cred,
        Err(_) => return response(StatusCode::UNAUTHORIZED),
    };
    // Update request stats
    {
        let mut stats = proxy_stats.write().await;
        stats.inc(path, username);
    }

    log::debug!("{} -> {}", path, proxy_to);

    // Write proxy headers
    let headers = req.headers_mut();
    headers.insert(
        HeaderName::from_static("x-forwarded-for"),
        HeaderValue::try_from(address.ip().to_string()).unwrap(),
    );

    *req.uri_mut() = proxy_to;
    client.request(req).await
}

#[inline]
fn response(code: StatusCode) -> hyper::Result<hyper::Response<hyper::Body>> {
    Ok(Response::builder()
        .status(code)
        .body(hyper::Body::empty())
        .unwrap())
}

#[inline]
fn decode_base64(string: &str) -> Result<String, ()> {
    let decoded = base64::decode(string).map_err(|_| ())?;
    String::from_utf8(decoded).map_err(|_| ())
}

#[inline]
fn extract_basic_auth(headers: &HeaderMap) -> Result<&str, ()> {
    if let Some(Ok(auth)) = headers.get(header::AUTHORIZATION).map(|v| v.to_str()) {
        if let Some(idx) = auth.find(' ') {
            if auth[..idx].eq_ignore_ascii_case("basic") {
                return Ok(auth[(idx + 1).min(auth.len())..].trim());
            }
        }
    }
    Err(())
}

#[inline]
fn extract_username(decoded_auth: &str) -> Result<&str, ()> {
    let mut split = decoded_auth.splitn(2, ':');
    split.next().ok_or(())
}
