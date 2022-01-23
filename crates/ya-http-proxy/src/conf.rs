use std::net::SocketAddr;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub use crate::conf::client::ClientConf;
pub use crate::conf::common::CommonConf;
pub use crate::conf::server::ServerConf;
use crate::ProxyError;

mod client;
mod common;
mod server;

/// Management API configuration
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagementConf {
    pub addr: SocketAddr,
}

/// Proxy instance configuration
#[derive(Default, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyConf {
    #[serde(default)]
    pub client: ClientConf,
    #[serde(default)]
    pub server: ServerConf,
}

impl ProxyConf {
    pub fn from_env() -> Result<Self, ProxyError> {
        envy::from_env().map_err(|e| ProxyError::Conf(e.to_string()))
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ProxyError> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path).map_err(|e| ProxyError::conf(path, e))?;
        let format = path
            .extension()
            .ok_or_else(|| ProxyError::conf(path, "file extension missing"))?
            .to_string_lossy()
            .to_lowercase();

        let conf: Self = match format.as_str() {
            "json" => serde_json::from_str(&contents).map_err(|e| ProxyError::conf(path, e))?,
            "toml" => toml::de::from_str(&contents).map_err(|e| ProxyError::conf(path, e))?,
            "yaml" | "yml" => {
                serde_yaml::from_str(&contents).map_err(|e| ProxyError::conf(path, e))?
            }
            _ => return Err(ProxyError::conf(path, "unknown file extension")),
        };

        Ok(conf)
    }
}
