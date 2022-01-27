use http::{Method, Uri};
use serde::{Deserialize, Serialize};

use crate::model::ErrorResponse;
use crate::{Error, Result};

/// Body size limit: 8 MiB
const MAX_BODY_SIZE: usize = 8 * 1024 * 1024;

#[derive(Clone)]
pub struct WebClient {
    url: Uri,
    inner: awc::Client,
}

impl WebClient {
    pub fn new(url: String) -> Result<Self> {
        Ok(Self {
            url: url.parse()?,
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

        if !res.status().is_success() {
            let e = ErrorResponse {
                message: res.status().to_string(),
            };
            return Ok(serde_json::from_value(serde_json::json!(e))?);
        }

        let raw_body = res.body().limit(MAX_BODY_SIZE).await?;
        let body = std::str::from_utf8(&raw_body)?;
        log::debug!(
            "WebRequest: method={} url={}, resp='{}'",
            method,
            url,
            body.split_at(512.min(body.len())).0,
        );
        Ok(serde_json::from_str(body)?)
    }
}