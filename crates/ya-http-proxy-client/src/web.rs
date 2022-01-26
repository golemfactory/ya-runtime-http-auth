use awc::http::header;
use heck::MixedCase;
use http::{Method, StatusCode, Uri};
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

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

        // allow empty body and no content (204) to pass smoothly
        if StatusCode::NO_CONTENT == res.status()
            || Some("0")
                == res
                    .headers()
                    .get(header::CONTENT_LENGTH)
                    .and_then(|h| h.to_str().ok())
        {
            return Ok(serde_json::from_value(serde_json::json!(()))?);
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

/// Builder for the query part of the URLs.
pub struct QueryParamsBuilder<'a> {
    serializer: form_urlencoded::Serializer<'a, String>,
}

impl<'a> QueryParamsBuilder<'a> {
    pub fn new() -> Self {
        let serializer = form_urlencoded::Serializer::new("".into());
        QueryParamsBuilder { serializer }
    }

    pub fn put<N: ToString, V: ToString>(mut self, name: N, value: Option<V>) -> Self {
        if let Some(v) = value {
            self.serializer
                .append_pair(&name.to_string().to_mixed_case(), &v.to_string());
        };
        self
    }

    pub fn build(mut self) -> String {
        self.serializer.finish()
    }
}

impl<'a> Default for QueryParamsBuilder<'a> {
    fn default() -> Self {
        Self::new()
    }
}

/// Macro to facilitate URL formatting for REST API async bindings
macro_rules! url_format {
    {
        $path:expr $(,$var:ident)* $(,#[query] $varq:ident)* $(,)?
    } => {{
        let mut url = format!( $path $(, $var=$var)* );
        let query = crate::web::QueryParamsBuilder::new()
            $( .put( stringify!($varq), $varq ) )*
            .build();
        if query.len() > 1 {
            url = format!("{}?{}", url, query)
        }
        url
    }};
}
