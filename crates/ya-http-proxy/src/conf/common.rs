use serde::{Deserialize, Serialize};
use std::time::Duration;

use ya_http_proxy_model::deser;

/// Configuration options common to both client and server
#[derive(Default, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommonConf {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http1_writev: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http1_max_buf_size: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http1_title_case_headers: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http1_preserve_header_case: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub http2_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, with = "deser::double_opt")]
    pub http2_initial_stream_window_size: Option<Option<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, with = "deser::double_opt")]
    pub http2_initial_connection_window_size: Option<Option<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http2_adaptive_window: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, with = "deser::double_opt")]
    pub http2_max_frame_size: Option<Option<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, with = "deser::duration::double_opt_ms")]
    pub http2_keep_alive_interval: Option<Option<Duration>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, with = "deser::duration::opt_ms")]
    pub http2_keep_alive_timeout: Option<Duration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http2_max_send_buf_size: Option<usize>,
}

#[macro_export]
macro_rules! conf_builder {
    ($dst:ident, $src:ident, [ $($prop:ident),* ] ) => {{
        #![allow(unused)]
        {
            $(if let Some(ref v) = $src.$prop {
                $dst = $dst.$prop(*v);
            })*
        }
    }}
}

#[macro_export]
macro_rules! conf_builder_common {
    ($dst:ident, $src:ident) => {{
        crate::conf_builder!(
            $dst,
            $src,
            [
                http1_writev,
                http1_max_buf_size,
                http1_title_case_headers,
                http1_preserve_header_case,
                http2_only,
                http2_initial_stream_window_size,
                http2_initial_connection_window_size,
                http2_adaptive_window,
                http2_max_frame_size,
                http2_keep_alive_interval,
                http2_keep_alive_timeout,
                http2_max_send_buf_size
            ]
        );
    }};
}
