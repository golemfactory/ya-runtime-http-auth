use std::net::SocketAddr;
use std::sync::Arc;

use hyper::client::HttpConnector;
use hyper::header::{self, HeaderName, HeaderValue};
use hyper::http::uri::PathAndQuery;
use hyper::http::Uri;
use hyper::{Body, Client, HeaderMap, Request, Response, StatusCode};
use tokio::sync::RwLock;

use crate::proxy::{ProxyState, ProxyStats};

const EMPTY_STR: &str = "";

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

    let f = from_str.len();
    let f = if &req_str[..f] == from_str { f } else { 0 };
    let req_str = &req_str[f..];

    let paq = if let Some(to) = to_paq {
        let merge = [to.as_str(), req_str].concat();
        let len = merge.len();
        if len > 1 && &merge[len - 2..] == "//" {
            PathAndQuery::try_from(&merge[..len - 1])
        } else {
            PathAndQuery::try_from(merge)
        }
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
        None | Some("") | Some("/") => EMPTY_STR,
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

    #[test]
    fn merge_uri_paths() -> anyhow::Result<()> {
        let verify = |from: &Uri, to: &Uri, uri: &str, expected: &str| {
            let mut req_uri = Uri::try_from(uri).unwrap();
            let expected_uri = Uri::try_from(expected).unwrap();

            merge_path_and_query(&mut req_uri, from.clone(), to.clone()).unwrap();

            println!("from {from} to {to} | {req_uri} vs {expected}");
            assert_eq!(req_uri, expected_uri);
            println!("-- ok");
        };

        let from = Uri::from_static("/");
        let to = Uri::try_from("http://127.0.0.1").unwrap();
        verify(&from, &to, "http://1.0.0.1", "http://127.0.0.1/");
        verify(&from, &to, "http://1.0.0.1/", "http://127.0.0.1/");

        let from = Uri::from_static("/");
        let to = Uri::try_from("http://127.0.0.1/to").unwrap();
        verify(&from, &to, "http://1.0.0.1", "http://127.0.0.1/to/");
        verify(&from, &to, "http://1.0.0.1/", "http://127.0.0.1/to/");

        let from = Uri::from_static("/");
        let to = Uri::try_from("http://127.0.0.1/to/").unwrap();
        verify(&from, &to, "http://1.0.0.1", "http://127.0.0.1/to/");
        verify(&from, &to, "http://1.0.0.1/", "http://127.0.0.1/to/");

        let from = Uri::from_static("/sub");
        let to = Uri::try_from("http://127.0.0.1/").unwrap();
        verify(&from, &to, "http://1.0.0.1/sub", "http://127.0.0.1/");
        verify(&from, &to, "http://1.0.0.1/sub/", "http://127.0.0.1/");

        let from = Uri::from_static("/sub/2");
        let to = Uri::try_from("http://127.0.0.1/to").unwrap();
        verify(&from, &to, "http://1.0.0.1/sub/2", "http://127.0.0.1/to");
        verify(&from, &to, "http://1.0.0.1/sub/2/", "http://127.0.0.1/to/");

        let from = Uri::from_static("/");
        let to = Uri::try_from("http://127.0.0.1/to").unwrap();
        verify(
            &from,
            &to,
            "http://1.0.0.1/resource",
            "http://127.0.0.1/to/resource",
        );
        verify(
            &from,
            &to,
            "http://1.0.0.1/resource/",
            "http://127.0.0.1/to/resource/",
        );

        let from = Uri::from_static("/sub/2");
        let to = Uri::try_from("http://127.0.0.1/to").unwrap();
        verify(
            &from,
            &to,
            "http://1.0.0.1/sub/2/resource",
            "http://127.0.0.1/to/resource",
        );
        verify(
            &from,
            &to,
            "http://1.0.0.1/sub/2/resource/",
            "http://127.0.0.1/to/resource/",
        );

        Ok(())
    }
}
