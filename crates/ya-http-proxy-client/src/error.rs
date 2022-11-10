use awc::error::{PayloadError, SendRequestError};
use http::uri::InvalidUri;
use http::{Method, StatusCode};

/// Specialized Error for proxy Management API
#[derive(thiserror::Error, Debug)]
#[allow(missing_docs)]
pub enum Error {
    #[error("HTTP error requesting {method} {url}: {code}; msg: '{msg}'")]
    SendRequestError {
        code: StatusCode,
        msg: String,
        method: Method,
        url: String,
    },
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::error::Error),
    #[error("Invalid UTF8 string: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),
    #[error("AWC payload error: {0}")]
    PayloadError(String),
    #[error("Invalid URI string: {0}")]
    InvalidUriError(#[from] InvalidUri),
}

impl From<PayloadError> for Error {
    fn from(e: PayloadError) -> Self {
        Self::PayloadError(e.to_string())
    }
}

impl Error {
    pub(crate) fn from_request(err: SendRequestError, method: Method, url: String) -> Self {
        let msg = err.to_string();
        let code = StatusCode::INTERNAL_SERVER_ERROR;
        Error::SendRequestError {
            code,
            msg,
            method,
            url,
        }
    }
}
