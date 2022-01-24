use std::net::SocketAddr;
use std::path::Path;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid URL: {0}")]
    Url(#[from] hyper::http::uri::InvalidUri),
    #[error(transparent)]
    Tls(#[from] TlsError),
    #[error(transparent)]
    Management(#[from] ManagementError),
    #[error(transparent)]
    Proxy(#[from] ProxyError),
    #[error(transparent)]
    Service(#[from] ServiceError),
    #[error(transparent)]
    User(#[from] UserError),
    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn other(msg: impl ToString) -> Self {
        Self::Other(msg.to_string())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum TlsError {
    #[error("Client CA certificate error: {0}")]
    ClientCertStore(String),
    #[error("Server certificate error: {0}")]
    ServerCertStore(String),
    #[error("Server key error: {0}")]
    ServerCertKey(String),
    #[error("TLS error: {0}")]
    Other(String),
}

#[derive(thiserror::Error, Debug)]
pub enum ManagementError {
    #[error("Management API server is not running")]
    NotRunning,
    #[error("Management API server cannot bind to {address}: {message}")]
    Bind {
        address: SocketAddr,
        message: String,
    },
}

#[derive(thiserror::Error, Debug)]
pub enum ProxyError {
    #[error("Proxy is already running on {}", .0.to_string())]
    AlreadyExists(std::net::SocketAddr),
    #[error("Proxy runtime error: {0}")]
    Runtime(String),
    #[error("Proxy configuration error: {0}")]
    Conf(String),
}

#[derive(thiserror::Error, Debug)]
pub enum ServiceError {
    #[error("Service '{name}' is already bound to '{endpoint}'")]
    AlreadyExists { name: String, endpoint: String },
    #[error("Service '{0}' not found")]
    NotFound(String),
}

#[derive(thiserror::Error, Debug)]
pub enum UserError {
    #[error("User already '{0}' exists")]
    AlreadyExists(String),
    #[error("User '{0}' not found")]
    NotFound(String),
}

impl ProxyError {
    pub fn conf(path: impl AsRef<Path>, e: impl ToString) -> Self {
        Self::Conf(format!(
            "error reading '{}': {}",
            path.as_ref().display(),
            e.to_string()
        ))
    }

    pub fn rt(m: impl ToString) -> Self {
        Self::Runtime(m.to_string())
    }
}

impl From<hyper::Error> for ProxyError {
    fn from(e: hyper::Error) -> Self {
        Self::Runtime(format!("server error: {}", e))
    }
}

impl From<routerify::Error> for ProxyError {
    fn from(e: routerify::Error) -> Self {
        Self::Runtime(format!("route error: {}", e))
    }
}
