use std::collections::{HashMap, HashSet};
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
    #[allow(missing_docs)]
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

/// Public service information
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PubService {
    /// Service name.
    pub name: String,
    /// Host name. (for virtual name server).
    pub server_name: Vec<String>,
    /// Time when service was created.
    pub created_at: DateTime<Utc>,
    /// Set of ports for `https` connections.
    pub port_https: HashSet<u16>,
    /// Set of ports for `http` connections.
    pub port_http: HashSet<u16>,
    /// SSL certificate hash.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cert_hash: Option<String>,
    /// Service timeout rules.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeouts: Option<Timeouts>,
    /// How many cpu threads should be started for given service.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_threads: Option<usize>,
}

impl From<Service> for PubService {
    fn from(service: Service) -> Self {
        let port_https = service.inner.https_ports();
        let port_http = service.inner.http_ports();

        Self {
            name: service.inner.name,
            server_name: service.inner.server_name,
            created_at: service.created_at,
            port_https,
            port_http,
            cert_hash: service.inner.cert.as_ref().map(|c| c.hash.clone()),
            timeouts: service.inner.timeouts,
            cpu_threads: service.inner.cpu_threads,
        }
    }
}

/// New service descriptor
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateService {
    /// Unique Service name
    #[serde(default = "next_service_name")]
    pub name: String,
    /// Domain names or public IP addresses
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
    #[serde(with = "deser::uri")]
    pub from: Uri,
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
    /// Collection of all service listen addresses for `https` & `http`.
    pub fn addresses(&self) -> Addresses {
        self.bind_https.clone().unwrap_or_default() + self.bind_http.clone().unwrap_or_default()
    }

    /// Collection of listen port for `https` addresses.
    pub fn https_ports(&self) -> HashSet<u16> {
        Self::ports(&self.bind_https)
    }

    /// Collection of listen port for `http` addresses.
    pub fn http_ports(&self) -> HashSet<u16> {
        Self::ports(&self.bind_http)
    }

    fn ports(bind: &Option<Addresses>) -> HashSet<u16> {
        match bind {
            Some(addrs) => addrs.ports(),
            None => Default::default(),
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
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateServiceCert {
    /// Hash Sha3_256 of ssl certificate.
    #[serde(default)]
    pub hash: String,
    /// Certificate path on disk.
    pub path: PathBuf,
    /// certificate key.
    pub key_path: PathBuf,
}

impl PartialEq for CreateServiceCert {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path && self.key_path == other.key_path
    }
}

impl Eq for CreateServiceCert {}

/// New user descriptor
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateUser {
    /// Http auth user name.
    pub username: String,
    /// Password for new user.
    pub password: String,
}

/// User descriptor
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    /// User name.
    pub username: String,
    /// Time when user was created.
    pub created_at: DateTime<Utc>,
}

/// Aggregated user statistics
#[derive(Clone, Default, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserStats {
    /// Number of user requests.
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
    /// Timeout for requests.
    #[serde(with = "deser::duration::opt_ms")]
    pub request_timeout: Option<Duration>,
    /// Max wait time for response.
    #[serde(with = "deser::duration::opt_ms")]
    pub response_timeout: Option<Duration>,
}

/// Error response
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    /// Human readable error message.
    pub message: String,
}

/// Global statistics
/// FIXME: introduce per-runtime instead of global statistics
#[derive(Clone, Default, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalStats {
    /// Number of registered users.
    pub users: usize,
    /// Number of created services.
    pub services: usize,
    #[doc(hidden)]
    pub requests: UserStats,
}

fn next_service_name() -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static ATOMIC_ID: AtomicUsize = AtomicUsize::new(0);
    let id = ATOMIC_ID.fetch_add(1, Ordering::SeqCst);
    format!("service-{}", id)
}
