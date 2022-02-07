mod lock;
mod proxy;

use std::collections::HashMap;
use std::fs::{read_dir, File};
use std::io::BufReader;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use futures::future::{AbortHandle, Abortable};
use futures::{FutureExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_default::DefaultFromSerde;
use structopt::StructOpt;
use tokio::sync::RwLock;

use ya_http_proxy_client::api::ManagementApi;
use ya_http_proxy_client::web::{WebClient, DEFAULT_MANAGEMENT_API_URL};
use ya_http_proxy_model::{CreateService, CreateUser, GlobalStats};
use ya_runtime_sdk::cli::parse_cli;
use ya_runtime_sdk::env::Env;
use ya_runtime_sdk::*;

type RuntimeCli = <BasicAuthRuntime as RuntimeDef>::Cli;

const MANAGEMENT_API_URL_ENV_VAR: &str = "MANAGEMENT_API_URL";
const COUNTER_NAME: &str = "golem.runtime.http-auth.requests.counter";
const INTERVAL: Duration = Duration::from_secs(2);

#[derive(RuntimeDef)]
#[cli(BasicAuthCli)]
#[conf(BasicAuthConf)]
pub struct BasicAuthRuntime {
    basic_auth: Rc<RwLock<BasicAuth>>,
}

impl From<BasicAuth> for BasicAuthRuntime {
    fn from(basic_auth: BasicAuth) -> Self {
        Self {
            basic_auth: Rc::new(RwLock::new(basic_auth)),
        }
    }
}

pub struct BasicAuth {
    api: ManagementApi,
    handle: Option<AbortHandle>,
    users: HashMap<CreateService, CreateUser>,
    global_stats: GlobalStats,
}

impl BasicAuth {
    pub async fn requests(&self) -> usize {
        futures::stream::iter(self.users.iter())
            .map(|(s, u)| {
                self.api
                    .get_user_stats(s.name.as_str(), u.username.as_str())
            })
            .buffered(3)
            .filter_map(|r| async move { r.ok() })
            .fold(0, |mut acc, stats| async move {
                acc += stats.requests;
                acc
            })
            .await
    }
}

#[derive(Default)]
pub struct BasicAuthEnv {
    service_name: Option<String>,
}

#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub struct BasicAuthCli {
    name: String,
}

#[derive(Deserialize, Serialize, DefaultFromSerde)]
#[serde(rename_all = "camelCase")]
pub struct BasicAuthConf {
    #[serde(default = "default_management_api_url")]
    pub management_api_url: String,
}

fn default_management_api_url() -> String {
    std::env::var(MANAGEMENT_API_URL_ENV_VAR)
        .unwrap_or_else(|_| DEFAULT_MANAGEMENT_API_URL.to_string())
}

impl Env<RuntimeCli> for BasicAuthEnv {
    fn cli(&mut self, project_name: &str, project_version: &str) -> anyhow::Result<RuntimeCli> {
        let cli: RuntimeCli = parse_cli(project_name, project_version, self.args())?;
        self.service_name = Some(cli.runtime.name.clone());
        Ok(cli)
    }
}

impl Runtime for BasicAuthRuntime {
    fn deploy<'a>(&mut self, ctx: &mut Context<Self>) -> OutputResponse<'a> {
        let result = match config_lookup(ctx) {
            Some(_) => Ok(None),
            None => Err(ya_runtime_sdk::error::Error::from_string(
                "Config file not found".to_string(),
            )),
        };
        async move { result }.boxed_local()
    }

    fn start<'a>(&mut self, ctx: &mut Context<Self>) -> OutputResponse<'a> {
        let mut emitter = match ctx.emitter.clone() {
            Some(emitter) => emitter,
            None => {
                let err = anyhow::anyhow!("Not running in server mode");
                return futures::future::err(err.into()).boxed_local();
            }
        };

        let basic_auth = self.basic_auth.clone();

        async move {
            let api = {
                let inner = basic_auth.read().await;
                inner.api.clone()
            };

            if let Err(e) = proxy::spawn(api.clone()).await {
                return Err(ya_runtime_sdk::error::Error::from(e));
            }

            let (h, reg) = AbortHandle::new_pair();
            basic_auth.write().await.handle.replace(h);

            tokio::task::spawn_local(Abortable::new(
                async move {
                    loop {
                        let requests = {
                            let inner = basic_auth.read().await;
                            inner.requests().await
                        };

                        if let Ok(stats) = api.get_global_stats().await {
                            basic_auth.write().await.global_stats = stats;
                        }

                        emitter
                            .counter(RuntimeCounter {
                                name: COUNTER_NAME.to_string(),
                                value: requests as f64,
                            })
                            .await;

                        tokio::time::delay_for(INTERVAL).await;
                    }
                },
                reg,
            ));

            Ok(None)
        }
        .boxed_local()
    }

    fn stop<'a>(&mut self, ctx: &mut Context<Self>) -> EmptyResponse<'a> {
        let mut emitter = match ctx.emitter.clone() {
            Some(emitter) => emitter,
            None => {
                let err = anyhow::anyhow!("Not running in server mode");
                return futures::future::err(err.into()).boxed_local();
            }
        };

        let inner = self.basic_auth.clone();
        async move {
            let inner = inner.read().await;
            if let Some(handle) = &inner.handle {
                handle.abort();
            };

            let requests = inner.requests().await;
            drop(inner);

            emitter
                .counter(RuntimeCounter {
                    name: COUNTER_NAME.to_string(),
                    value: requests as f64,
                })
                .await;

            Ok(())
        }
        .boxed_local()
    }

    fn run_command<'a>(
        &mut self,
        cmd: RunProcess,
        _mode: RuntimeMode,
        ctx: &mut Context<Self>,
    ) -> ProcessIdResponse<'a> {
        ctx.command(|mut run_ctx| async move {
            run_ctx.stdout(format!("{:?}", cmd)).await;
            Ok(())
        })
    }

    fn offer<'a>(&mut self, _ctx: &mut Context<Self>) -> OutputResponse<'a> {
        todo!()
    }

    fn test<'a>(&mut self, ctx: &mut Context<Self>) -> EmptyResponse<'a> {
        if config_lookup(ctx).is_none() {
            return async move {
                Err(ya_runtime_sdk::error::Error::from_string(
                    "Config file not found".to_string(),
                ))
            }
            .boxed_local();
        };

        let inner = self.basic_auth.clone();
        async move {
            let inner = inner.read().await;
            let api = inner.api.clone();

            match proxy::spawn(api).await {
                Ok(()) => Ok(()),
                Err(e) => Err(ya_runtime_sdk::error::Error::from(e)),
            }
        }
        .boxed_local()
    }
}

fn main() -> anyhow::Result<()> {
    let runtime =
        ya_runtime_sdk::build::<BasicAuthRuntime, _, _, _>(BasicAuthEnv::default(), move |ctx| {
            let api_url = ctx.conf.management_api_url.clone();

            async move {
                let client = WebClient::new(api_url)?;
                let api = ManagementApi::new(client);

                Ok(BasicAuthRuntime::from(BasicAuth {
                    api,
                    handle: None,
                    users: Default::default(),
                    global_stats: Default::default(),
                }))
            }
        });

    let mut system = actix_rt::System::new("runtime");
    system.block_on(runtime)
}

fn config_lookup(ctx: &mut Context<BasicAuthRuntime>) -> Option<CreateService> {
    let mut paths = vec![];

    if let Some(path) = dirs::config_dir() {
        paths.push(path.join(env!("CARGO_PKG_NAME")))
    }

    if let Ok(path) = std::env::current_dir() {
        paths.push(path)
    }

    find_config(paths, ctx).ok().flatten()
}

fn find_config(
    paths: Vec<PathBuf>,
    ctx: &mut Context<BasicAuthRuntime>,
) -> anyhow::Result<Option<CreateService>> {
    let mut dir_paths = vec![];

    for path in paths {
        let dirs = read_dir(path)?;
        dir_paths.push(dirs.filter_map(|d| d.ok()).map(|d| d.path()).collect());
    }

    dir_paths = dir_paths
        .into_iter()
        .filter(|p: &PathBuf| match p.extension() {
            Some(ext) => ext == "json",
            None => false,
        })
        .collect();

    for dir_path in dir_paths {
        let cs = read_config(dir_path)?;
        if cs.name == ctx.env.runtime_name().unwrap() {
            return Ok(Some(cs));
        }
    }

    Ok(None)
}

fn read_config(path: PathBuf) -> anyhow::Result<CreateService> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let cs = serde_json::from_reader(reader)?;

    Ok(cs)
}
