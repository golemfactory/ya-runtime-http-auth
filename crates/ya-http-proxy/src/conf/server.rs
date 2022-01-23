use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_default::DefaultFromSerde;

use crate::conf::common::CommonConf;
use ya_http_proxy_client::deser;

/// Configuration for the HTTP proxy server
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DefaultFromSerde)]
pub struct ServerConf {
    /// Service listening address
    #[serde(default = "default::addr")]
    pub addr: SocketAddr,
    /// Number of CPU (worker) threads to use
    #[serde(default)]
    pub cpu_threads: Option<usize>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(with = "deser::duration::double_opt_ms")]
    #[serde(default = "default::tcp_keepalive")]
    pub tcp_keepalive: Option<Option<Duration>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default = "default::tcp_nodelay")]
    pub tcp_nodelay: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default = "default::tcp_sleep_on_accept_errors")]
    pub tcp_sleep_on_accept_errors: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default = "default::http1_keepalive")]
    pub http1_keepalive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub http1_half_close: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub http1_pipeline_flush: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, with = "deser::duration::opt_ms")]
    pub http1_header_read_timeout: Option<Duration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default = "default::http1_only")]
    pub http1_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub http2_max_concurrent_streams: Option<u32>,

    #[serde(default, flatten)]
    pub server_cert: ServerCertConf,
    #[serde(default, flatten)]
    pub server_common: CommonConf,
}

/// Client CA certificate configuration for the HTTPS client used by a Proxy
#[derive(Default, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerCertConf {
    pub server_cert_store_path: Option<PathBuf>,
    pub server_key_path: Option<PathBuf>,
}

mod default {
    use std::net::SocketAddr;
    use std::time::Duration;

    pub fn addr() -> SocketAddr {
        SocketAddr::from(([0, 0, 0, 0], 443))
    }

    pub const fn tcp_keepalive() -> Option<Option<Duration>> {
        Some(Some(Duration::from_secs(300)))
    }

    pub const fn tcp_nodelay() -> Option<bool> {
        Some(true)
    }

    pub const fn tcp_sleep_on_accept_errors() -> Option<bool> {
        Some(false)
    }

    pub const fn http1_keepalive() -> Option<bool> {
        Some(true)
    }

    pub const fn http1_only() -> Option<bool> {
        Some(false)
    }
}

#[macro_export]
macro_rules! conf_builder_server {
    ($dst:ident, $src:ident) => {{
        crate::conf_builder!(
            $dst,
            $src,
            [
                http1_keepalive,
                http1_half_close,
                http1_pipeline_flush,
                http1_header_read_timeout,
                http1_only,
                http2_max_concurrent_streams
            ]
        );
        let common = &$src.server_common;
        crate::conf_builder_common!($dst, common);
    }};
}
