use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use http::uri::Uri;
use serde::{Deserialize, Serialize};
use strum::{EnumString, EnumVariantNames, IntoStaticStr};

use crate::{deser, Addresses};

/// Authorization configuration
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Auth {
    /// Authorization method
    pub method: AuthMethod,
}

/// Authorization method
#[non_exhaustive]
#[derive(
    Clone, Debug, Eq, PartialEq, Deserialize, Serialize, EnumString, EnumVariantNames, IntoStaticStr,
)]
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
    #[serde(flatten)]
    pub inner: CreateService,
    /// Creation date
    pub created_at: DateTime<Utc>,
}

impl From<(CreateService, DateTime<Utc>)> for Service {
    fn from((inner, created_at): (CreateService, DateTime<Utc>)) -> Self {
        Self { inner, created_at }
    }
}

/// New service descriptor
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateService {
    /// Unique Service name
    #[serde(default = "next_service_name")]
    pub name: String,
    #[serde(default)]
    pub server_name: Vec<String>,
    /// HTTPS listening addresses
    #[serde(alias = "bind")]
    pub bind_https: Option<Addresses>,
    /// HTTP listening addresses
    pub bind_http: Option<Addresses>,
    /// Certificate configuration
    pub cert: Option<CreateServiceCert>,
    /// Authorization options
    pub auth: Option<Auth>,
    /// Source endpoint (e.g. `/resource`)
    pub from: String,
    /// Destination URL (e.g. `http://127.0.0.1:8080`)
    #[serde(with = "deser::uri")]
    pub to: Uri,
    /// Timeout configuration
    #[serde(flatten)]
    pub timeouts: Option<Timeouts>,
    /// Number of CPU (worker) threads to use
    pub cpu_threads: Option<usize>,
    /// Forwarding options
    pub user: Option<CreateServiceUser>,
}

impl CreateService {
    pub fn addresses(&self) -> Addresses {
        let addrs = self.bind_https.clone().unwrap_or_default();
        match self.bind_http.clone() {
            Some(a) => addrs + a,
            _ => addrs,
        }
    }
}

/// HTTP request forward options
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

/// New user descriptor
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateUser {
    pub username: String,
    pub password: String,
}

/// User descriptor
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub username: String,
    pub created_at: DateTime<Utc>,
}

/// Aggregated user statistics
#[derive(Clone, Default, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserStats {
    pub requests: usize,
}

/// User statistics per endpoint
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserEndpointStats(pub HashMap<String, usize>);

/// Timeout configuration
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Timeouts {
    #[serde(with = "deser::duration::opt_ms")]
    pub request_timeout: Option<Duration>,
    #[serde(with = "deser::duration::opt_ms")]
    pub response_timeout: Option<Duration>,
}

/// Error response
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    pub message: String,
}

/// Global statistics
/// FIXME: introduce per-runtime instead of global statistics
#[derive(Clone, Default, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalStats {
    pub users: usize,
    pub services: usize,
    pub requests: UserStats,
}

fn next_service_name() -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static ATOMIC_ID: AtomicUsize = AtomicUsize::new(0);
    let id = ATOMIC_ID.fetch_add(1, Ordering::SeqCst);
    format!("service-{}", id)
}
