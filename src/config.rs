use regex::Regex;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{read_dir, File};
use std::io::BufReader;
use std::path::PathBuf;

use ya_http_proxy_model::CreateService;
use ya_runtime_sdk::serialize::{json, toml, yaml};
use ya_runtime_sdk::Context;

use crate::HttpAuthRuntime;

pub const SERVICES_SUBDIRECTORY: &str = "services";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceConf {
    #[serde(flatten)]
    pub inner: CreateService,
    #[serde(default)]
    offer_properties: HashMap<String, json::Value>,
}

impl ServiceConf {
    pub fn offer_properties(&self, prefix: &str) -> anyhow::Result<json::Value> {
        let re = Regex::new(r"[^A-Za-z0-9-_.]+").unwrap();
        let mut map = json::Map::new();

        for (key, value) in self.offer_properties.iter() {
            let value = match sanitize_value(value)? {
                Some(value) => value,
                None => continue,
            };

            let lower = key.to_ascii_lowercase();
            let post_re = re.replace(&lower, "");
            let post_dedup = post_re
                .replace('_', "-")
                .replace("--", "-")
                .replace("..", ".");
            let trimmed = post_dedup.trim_end_matches('.');
            map.insert(format!("{}.{}", prefix, trimmed), value);
        }

        Ok(json::Value::Object(map))
    }
}

fn sanitize_value(value: &json::Value) -> anyhow::Result<Option<json::Value>> {
    let value = match value {
        json::Value::Null => return Ok(None),
        v @ json::Value::Bool(_) | v @ json::Value::Number(_) | v @ json::Value::String(_) => {
            v.clone()
        }
        v @ json::Value::Object(_) => json::Value::String(json::to_string(v)?),
        json::Value::Array(vec) => {
            let mut res = Vec::new();
            for v in vec {
                match sanitize_value(v)? {
                    Some(v) => res.push(v),
                    _ => continue,
                }
            }
            json::Value::Array(res)
        }
    };
    Ok(Some(value))
}

pub fn lookup(ctx: &mut Context<HttpAuthRuntime>) -> Option<ServiceConf> {
    let mut paths: Vec<_> = ctx.conf.service_lookup_dirs.clone();
    let local_paths = vec![dirs::data_local_dir(), dirs::config_dir()];

    paths.extend(local_paths.into_iter().flatten().map(|path| {
        path.join(env!("CARGO_PKG_NAME"))
            .join(SERVICES_SUBDIRECTORY)
    }));

    if let Ok(path) = std::env::current_dir() {
        let path = path.join(SERVICES_SUBDIRECTORY);
        paths.push(path);
    }

    find(paths, ctx)
}

fn find(paths: Vec<PathBuf>, ctx: &mut Context<HttpAuthRuntime>) -> Option<ServiceConf> {
    let runtime_name = ctx.env.runtime_name().unwrap();

    paths
        .into_iter()
        .filter_map(|p| read_dir(p).ok())
        .flatten()
        .filter_map(|r| r.ok().map(|e| e.path()))
        .filter_map(|p| read_file(p).ok())
        .find(|conf: &ServiceConf| conf.inner.name == runtime_name)
}

fn read_file<T: DeserializeOwned>(path: PathBuf) -> anyhow::Result<T> {
    let ext = match path.extension() {
        Some(ext) => ext.to_string_lossy().to_lowercase(),
        _ => anyhow::bail!("missing file extension"),
    };
    let t = match ext.as_str() {
        "json" => {
            let file = File::open(path)?;
            let reader = BufReader::new(file);
            json::from_reader(reader)?
        }
        "toml" => {
            let contents = std::fs::read_to_string(path)?;
            toml::from_str(&contents)?
        }
        "yaml" | "yml" => {
            let file = File::open(path)?;
            let reader = BufReader::new(file);
            yaml::from_reader(reader)?
        }
        _ => anyhow::bail!("unknown format"),
    };
    Ok(t)
}

#[cfg(test)]
mod tests {
    use crate::config::ServiceConf;
    use ya_runtime_sdk::serialize;

    #[test]
    fn service_offer_properties() {
        let json = serialize::json::json!({
            "name": "service_1",
            "bind": "127.0.0.1:443",
            "from": "/",
            "to": "http://127.0.0.1:8444",
            "cert": {
                "path": "/tmp/server.cert",
                "keyPath": "/tmp/server.key"
            },
            "offerProperties": {
                "first": 1,
                "second\\": true,
                "third/=.3": [1, "two", { "three": 3 }],
                " fourth": "4",
                "fifth..": null
            }
        });

        let expected = serialize::json::json!({
            "meta.first": 1,
            "meta.fourth": "4",
            "meta.second": true,
            "meta.third.3": [1, "two", "{\"three\":3}"]
        });

        let service: ServiceConf =
            serialize::json::from_value(json).expect("failed to deserialize service");

        let properties = service
            .offer_properties("meta")
            .expect("failed to build offer properties");

        assert_eq!(properties, expected);
    }
}
