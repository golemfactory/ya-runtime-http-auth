use std::net::SocketAddr;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Authorization configuration
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Auth {
    /// Authorization method
    pub method: AuthMethod,
}

/// Authorization method
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthMethod {
    /// HTTP basic auth
    Basic,
}

impl Default for AuthMethod {
    fn default() -> Self {
        Self::Basic
    }
}

/// Service descriptor
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    /// Creation date
    pub created_at: DateTime<Utc>,
    #[serde(flatten)]
    pub inner: CreateService,
}

/// New service descriptor
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateService {
    /// Service name (non-unique)
    pub name: String,
    pub command: String,
    /// Listening address
    pub bind: Option<SocketAddr>,
    /// Certificate configuration
    pub cert: CreateServiceCert,
    /// Authorization options
    pub auth: Option<Auth>,
    /// Source endpoint (e.g. `/resource`)
    pub from: String,
    /// Destination URL (e.g. `http://127.0.0.1:8080`)
    pub to: String,
    /// Timeout configuration
    #[serde(flatten)]
    pub timeouts: Option<Timeouts>,
    /// Number of CPU (worker) threads to use
    pub cpu_threads: Option<usize>,
    /// Forwarding options
    pub user: Option<CreateServiceUser>,
}

/// Forwarding options
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateServiceUser {
    /// Authorization options
    pub auth: Option<Auth>,
    /// Timeout configuration
    #[serde(flatten)]
    pub timeouts: Option<Timeouts>,
}

/// Service certificate configuration
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateServiceCert {
    pub path: PathBuf,
    pub key_path: PathBuf,
}

/// Timeout configuration
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Timeouts {
    pub request_timeout: Option<usize>,
    pub response_timeout: Option<usize>,
}
