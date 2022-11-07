use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

use crate::conf::common::CommonConf;
use ya_http_proxy_model::deser;

/// Configuration for the HTTP client used by a Proxy
#[derive(Default, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientConf {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, with = "deser::duration::double_opt_ms")]
    pub pool_idle_timeout: Option<Option<Duration>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool_max_idle_per_host: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_canceled_requests: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_host: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub http09_responses: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http1_read_buf_exact_size: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http1_allow_spaces_after_header_name_in_responses: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http2_keep_alive_while_idle: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http2_max_concurrent_reset_streams: Option<usize>,

    #[serde(default, flatten)]
    pub client_cert: ClientCertConf,
    #[serde(default, flatten)]
    pub client_common: CommonConf,
}

/// Client CA certificate configuration for the HTTPS client used by a Proxy
#[derive(Default, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientCertConf {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_ca_cert_store_path: Option<PathBuf>,
}

#[macro_export]
macro_rules! conf_builder_client {
    ($dst:ident, $src:ident) => {{
        $crate::conf_builder!(
            $dst,
            $src,
            [
                pool_idle_timeout,
                pool_max_idle_per_host,
                retry_canceled_requests,
                set_host,
                http09_responses,
                http1_read_buf_exact_size,
                http1_allow_spaces_after_header_name_in_responses,
                http2_keep_alive_while_idle,
                http2_max_concurrent_reset_streams
            ]
        );

        let common = &$src.client_common;
        $crate::conf_builder_common!($dst, common);
    }};
}
