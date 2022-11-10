use http::{Method, Uri};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::rc::Rc;

use crate::{Error, Result};
use ya_http_proxy_model::ErrorResponse;

/// Default management api url.
pub const DEFAULT_MANAGEMENT_API_URL: &str = "http://127.0.0.1:6668";

/// Environment variable to override default management api url.
pub const ENV_MANAGEMENT_API_URL: &str = "MANAGEMENT_API_URL";

const MAX_BODY_SIZE: usize = 8 * 1024 * 1024;

/// REST api client abstraction
#[derive(Clone)]
pub struct WebClient {
    url: Rc<Uri>,
    inner: awc::Client,
}

impl WebClient {
    pub fn try_default() -> Result<Self> {
        Self::new(default_management_api_url().as_ref())
    }

    pub fn new(url: &str) -> Result<Self> {
        Ok(Self {
            url: Rc::new(url.parse()?),
            inner: awc::Client::new(),
        })
    }

    pub async fn get<R, S>(&self, uri: S) -> Result<R>
    where
        R: for<'de> Deserialize<'de>,
        S: AsRef<str>,
    {
        self.request::<(), R, S>(Method::GET, uri, None).await
    }

    pub async fn post<P, R, S>(&self, uri: S, payload: &P) -> Result<R>
    where
        P: Serialize,
        R: for<'de> Deserialize<'de>,
        S: AsRef<str>,
    {
        self.request(Method::POST, uri, Some(payload)).await
    }

    pub async fn delete<S>(&self, uri: S) -> Result<()>
    where
        S: AsRef<str>,
    {
        self.request::<(), (), S>(Method::DELETE, uri, None).await
    }

    async fn request<P, R, S>(&self, method: Method, uri: S, payload: Option<&P>) -> Result<R>
    where
        P: Serialize,
        R: for<'de> Deserialize<'de>,
        S: AsRef<str>,
    {
        let uri = uri.as_ref();
        let url = format!("{}{}", self.url, uri);

        let req = self.inner.request(method.clone(), &url);

        let mut res = match payload {
            Some(payload) => req.send_json(payload),
            None => req.send(),
        }
        .await
        .map_err(|e| Error::from_request(e, method.clone(), url.clone()))?;

        let raw_body = res.body().limit(MAX_BODY_SIZE).await?;
        let body = std::str::from_utf8(&raw_body)?;

        log::debug!(
            "WebRequest: method={} url={}, resp='{}'",
            method,
            url,
            body.split_at(512.min(body.len())).0,
        );

        if res.status().is_success() {
            return Ok(serde_json::from_str(body)?);
        }

        let response: ErrorResponse = serde_json::from_str(body)?;
        Err(Error::SendRequestError {
            code: res.status(),
            url,
            method,
            msg: response.message,
        })
    }
}

fn default_management_api_url() -> Cow<'static, str> {
    std::env::var(ENV_MANAGEMENT_API_URL)
        .map(Cow::Owned)
        .unwrap_or_else(|_| Cow::Borrowed(DEFAULT_MANAGEMENT_API_URL))
}
