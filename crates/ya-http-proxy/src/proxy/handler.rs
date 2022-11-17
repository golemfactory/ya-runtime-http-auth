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
) -> hyper::Result<Response<Body>> {
    let path = req.uri().path();
    let headers = req.headers();
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

    let proxy_from = service.created_with.from.clone();
    let proxy_to = service.created_with.to.clone();
    drop(state);

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

    // Update request stats
    {
        let mut stats = proxy_stats.write().await;
        stats.inc(path, username);
    }

    log::debug!("[{}] {} -> {}", username, path, proxy_to);

    // Write proxy headers
    let headers = req.headers_mut();

    headers.insert(
        HeaderName::from_static("x-forwarded-for"),
        HeaderValue::try_from(address.ip().to_string()).unwrap(),
    );

    if let Some(host) = host {
        headers.insert(HeaderName::from_static("x-forwarded-host"), host);
    }

    if let Err(e) = merge_path_and_query(req.uri_mut(), proxy_from, proxy_to) {
        log::warn!("Forwarded path error: {}", e);
        return response(StatusCode::INTERNAL_SERVER_ERROR);
    }
    client.request(req).await
}

#[inline]
fn response(code: StatusCode) -> hyper::Result<Response<Body>> {
    let mut builder = Response::builder().status(code);

    if code == StatusCode::UNAUTHORIZED {
        builder = builder.header(header::WWW_AUTHENTICATE, "Basic realm=\"Service access\"");
    }
    Ok(builder.body(Body::empty()).unwrap())
}

#[inline]
fn merge_path_and_query(req_uri: &mut Uri, proxy_from: Uri, proxy_to: Uri) -> Result<(), String> {
    let from_parts = proxy_from.into_parts();
    let mut to_parts = proxy_to.into_parts();

    let req_paq = req_uri.path_and_query();
    let from_paq = from_parts.path_and_query.as_ref();
    let to_paq = to_parts.path_and_query.as_ref();

    let req_str = extract_req(&req_paq);
    let from_str = extract_from(&from_paq);

    fn strip_or_stay<'a>(s: &'a str, p: &str) -> &'a str {
        s.strip_prefix(p).unwrap_or(s)
    }

    let (is_root, req_str) = {
        let r = strip_or_stay(req_str, from_str);
        (r == "/", strip_or_stay(r, "/"))
    };

    let paq = if let Some(to) = to_paq {
        let to_str = to.as_str();
        let merge = if req_str.is_empty() {
            if is_root && !to_str.ends_with('/') {
                [to.as_str(), "/"].concat()
            } else {
                to.to_string()
            }
        } else if to_str.ends_with('/') {
            [to.as_str(), req_str].concat()
        } else if is_root {
            [to_str, "/"].concat()
        } else {
            [to_str, "/", req_str].concat()
        };
        PathAndQuery::try_from(merge)
    } else {
        PathAndQuery::try_from(req_str)
    }
    .map_err(|e| e.to_string())?;

    to_parts.path_and_query.replace(paq);
    *req_uri = Uri::from_parts(to_parts).map_err(|e| e.to_string())?;

    Ok(())
}

#[inline]
fn extract_req<'a>(paq: &'a Option<&PathAndQuery>) -> &'a str {
    match paq.map(|p| p.as_str()) {
        None | Some("") => "/",
        Some(s) => s,
    }
}

#[inline]
fn extract_from<'a>(paq: &'a Option<&PathAndQuery>) -> &'a str {
    match paq.map(|p| p.as_str()) {
        None | Some("") | Some("/") => "/",
        Some(s) => {
            let e = s.len() - 1;
            if &s[e..] == "/" {
                &s[..e]
            } else {
                s
            }
        }
    }
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
    use serde::de::StdError;

    #[test]
    fn merge_uri_paths() -> anyhow::Result<()> {
        fn verify<T1, T2>(from: T1, to: T2, request: &str, expect: &str) -> anyhow::Result<()>
        where
            T1: TryInto<Uri>,
            T2: TryInto<Uri>,
            <T1 as TryInto<Uri>>::Error: StdError + Send + Sync + 'static,
            <T2 as TryInto<Uri>>::Error: StdError + Send + Sync + 'static,
        {
            let from_uri = from.try_into()?;
            let to_uri = to.try_into()?;
            let mut req_uri = request.parse()?;
            let expect_uri: Uri = expect.parse()?;
            merge_path_and_query(&mut req_uri, from_uri, to_uri).map_err(anyhow::Error::msg)?;

            assert_eq!(req_uri, expect_uri);
            Ok(())
        }

        verify(
            "/",
            "http://127.0.0.1:5050/",
            "/eth/v1/node/syncing",
            "http://127.0.0.1:5050/eth/v1/node/syncing",
        )?;
        verify(
            "/",
            "http://127.0.0.1",
            "http://1.0.0.1",
            "http://127.0.0.1/",
        )?;
        verify(
            "/",
            "http://127.0.0.1/to",
            "http://1.0.0.1/",
            "http://127.0.0.1/to",
        )?;
        verify(
            "/",
            "http://127.0.0.1/to/",
            "http://1.0.0.1",
            "http://127.0.0.1/to/",
        )?;
        verify(
            "/",
            "http://127.0.0.1/to/",
            "http://1.0.0.1/",
            "http://127.0.0.1/to/",
        )?;

        verify(
            "/sub",
            "http://127.0.0.1/",
            "http://1.0.0.1/sub",
            "http://127.0.0.1/",
        )?;
        verify(
            "/sub",
            "http://127.0.0.1/",
            "http://1.0.0.1/sub/",
            "http://127.0.0.1/",
        )?;

        verify(
            "/sub/2",
            "http://127.0.0.1/to",
            "http://1.0.0.1/sub/2",
            "http://127.0.0.1/to",
        )?;

        verify(
            "/sub/2",
            "http://127.0.0.1/to",
            "http://1.0.0.1/sub/2/test",
            "http://127.0.0.1/to/test",
        )?;
        verify(
            "/sub/2",
            "http://127.0.0.1/to",
            "http://1.0.0.1/sub/2/",
            "http://127.0.0.1/to/",
        )?;

        verify(
            "/",
            "http://127.0.0.1/to",
            "http://1.0.0.1/resource",
            "http://127.0.0.1/to/resource",
        )?;

        verify(
            "/",
            "http://127.0.0.1/to",
            "http://1.0.0.1/resource/",
            "http://127.0.0.1/to/resource/",
        )?;

        verify(
            "/sub/2",
            "http://127.0.0.1/to",
            "http://1.0.0.1/sub/2/resource",
            "http://127.0.0.1/to/resource",
        )?;
        verify(
            "/sub/2",
            "http://127.0.0.1/to",
            "http://1.0.0.1/sub/2/resource/",
            "http://127.0.0.1/to/resource/",
        )?;

        Ok(())
    }
}
