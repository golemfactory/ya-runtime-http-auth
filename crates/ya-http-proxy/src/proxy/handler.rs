use std::net::SocketAddr;
use std::sync::Arc;

use hyper::client::HttpConnector;
use hyper::header::{self, HeaderName, HeaderValue};
use hyper::http::uri::PathAndQuery;
use hyper::http::Uri;
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
    let headers = req.headers();
    let state = proxy_state.read().await;

    log::error!("forward error error1: {}", address);
    // Check whether the service is registered
    let service = match state
        .by_endpoint
        .iter()
        .find(|(e, _)| path.starts_with(e.as_str()))
    {
        Some((_, service)) => service,
        None => return response(StatusCode::NOT_FOUND),
    };

    log::error!("forward error auth: {}", address);
    // TODO: consider reading credentials from URL
    // Extract credentials from header
    let auth = match extract_basic_auth(headers) {
        Ok(auth) => auth,
        Err(_) => return response(StatusCode::UNAUTHORIZED),
    };
    // Authorize user
    if !service.access.contains(auth) {
        return response(StatusCode::UNAUTHORIZED);
    }

    let proxy_to = service.created_with.to.clone();
    drop(state);

    log::error!("forward error decode base64: {}", address);
    // Decode credentials
    let decoded_auth = match decode_base64(auth) {
        Ok(decoded_auth) => decoded_auth,
        Err(_) => return response(StatusCode::FORBIDDEN),
    };
    let username = match extract_username(&decoded_auth) {
        Ok(username) => username,
        Err(_) => return response(StatusCode::FORBIDDEN),
    };

    // Domain name
    let host = extract_host(headers);

    log::error!("forward error decode base64: {:?}", host);
    // Update request stats
    {
        let mut stats = proxy_stats.write().await;
        stats.inc(path, username);
    }

    log::info!("[{}] {} -> {}", username, path, proxy_to);

    // Write proxy headers
    let headers = req.headers_mut();

    headers.insert(
        HeaderName::from_static("x-forwarded-for"),
        HeaderValue::try_from(address.ip().to_string()).unwrap(),
    );

    if let Some(host) = host {
        headers.insert(HeaderName::from_static("x-forwarded-host"), host);
    }
    log::error!("forward error decode headers: {:?}", headers);

    if let Err(e) = merge_path_and_query(req.uri_mut(), proxy_to) {
        log::warn!("Forwarded path error: {}", e);
        return response(StatusCode::INTERNAL_SERVER_ERROR);
    }
    log::error!("Sending request: {:?}", req);
    let resp = client.request(req).await;
    log::error!("Got request: {:?}", resp);
    resp

}

#[inline]
fn response(code: StatusCode) -> hyper::Result<hyper::Response<hyper::Body>> {
    let mut builder = Response::builder().status(code);

    if code == StatusCode::UNAUTHORIZED {
        builder = builder.header(header::WWW_AUTHENTICATE, "Basic realm=\"Service access\"");
    }
    Ok(builder.body(hyper::Body::empty()).unwrap())
}

#[inline]
fn merge_path_and_query(req_uri: &mut Uri, proxy_to: Uri) -> Result<(), String> {
    let mut to_parts = proxy_to.into_parts();

    let to_paq = to_parts.path_and_query.as_ref();
    let req_paq = req_uri.path_and_query();

    match (to_paq, req_paq) {
        (Some(_), Some(req)) if req == "/" => (),
        (None, Some(req)) => {
            to_parts.path_and_query.replace(req.clone());
        }
        (Some(to), Some(req)) if to == "/" => {
            to_parts.path_and_query.replace(req.clone());
        }
        (Some(to), Some(req)) => {
            let paq =
                PathAndQuery::try_from(format!("{}{}", to, req)).map_err(|e| e.to_string())?;
            to_parts.path_and_query.replace(paq);
        }
        _ => (),
    }

    *req_uri = Uri::from_parts(to_parts).map_err(|e| e.to_string())?;
    Ok(())
}

#[inline]
fn decode_base64(string: &str) -> Result<String, ()> {
    let decoded = base64::decode(string).map_err(|_| ())?;
    String::from_utf8(decoded).map_err(|_| ())
}

#[inline]
fn extract_host(headers: &HeaderMap) -> Option<HeaderValue> {
    headers.get(header::HOST).cloned()
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

#[cfg(test)]
mod tests {
    use super::merge_path_and_query;
    use hyper::http::Uri;

    #[test]
    fn merge_uri_paths() -> anyhow::Result<()> {
        let verify = |against: Uri, uri: &str, expected: &str| {
            let mut req_uri = Uri::try_from(uri).unwrap();
            merge_path_and_query(&mut req_uri, against).unwrap();
            assert_eq!(&req_uri.to_string(), expected);
        };

        let proxy_to = Uri::try_from("http://127.0.0.1").unwrap();
        verify(proxy_to.clone(), "http://1.0.0.1", "http://127.0.0.1/");
        verify(proxy_to.clone(), "http://1.0.0.1/", "http://127.0.0.1/");

        let proxy_to = Uri::try_from("http://127.0.0.1/").unwrap();
        verify(proxy_to.clone(), "http://1.0.0.1", "http://127.0.0.1/");
        verify(proxy_to.clone(), "http://1.0.0.1/", "http://127.0.0.1/");

        let proxy_to = Uri::try_from("http://127.0.0.1/to").unwrap();
        verify(proxy_to.clone(), "http://1.0.0.1", "http://127.0.0.1/to");
        verify(proxy_to.clone(), "http://1.0.0.1/", "http://127.0.0.1/to");

        let proxy_to = Uri::try_from("http://127.0.0.1/to").unwrap();
        verify(
            proxy_to.clone(),
            "http://1.0.0.1/resource",
            "http://127.0.0.1/to/resource",
        );
        verify(
            proxy_to.clone(),
            "http://1.0.0.1/resource/",
            "http://127.0.0.1/to/resource/",
        );

        Ok(())
    }
}
