mod lock;

use futures::future::{AbortHandle, Abortable};
use futures::FutureExt;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::{Duration, Instant};
use structopt::StructOpt;
use tokio::process::Command;

use crate::lock::{with_lock_ext, LockFile};
use ya_http_proxy_client::api::ManagementApi;
use ya_http_proxy_client::model::{CreateService, CreateUser, GlobalStats, Requests};
use ya_http_proxy_client::web::WebClient;
use ya_runtime_sdk::cli::parse_cli;
use ya_runtime_sdk::env::Env;
use ya_runtime_sdk::*;

type RuntimeCli = <BasicAuthRuntime as RuntimeDef>::Cli;

const COUNTER_NAME: &str = "golem.runtime.http-auth.requests.counter";
const INTERVAL: Duration = Duration::from_secs(2);

#[derive(Default, RuntimeDef)]
#[cli(BasicAuthCli)]
pub struct BasicAuthRuntime {
    client: WebClient,
    handle: Option<AbortHandle>,
    users: RwLock<HashMap<CreateService, CreateUser>>,
    global_stats: Option<GlobalStats>,
}

#[derive(Default)]
pub struct BasicAuthEnv {
    runtime_name: Option<String>,
}

#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub struct BasicAuthCli {
    name: String,
}

impl Env<RuntimeCli> for BasicAuthEnv {
    fn runtime_name(&self) -> Option<String> {
        self.runtime_name.clone()
    }

    fn cli(&mut self, project_name: &str, project_version: &str) -> anyhow::Result<RuntimeCli> {
        let cli: RuntimeCli = parse_cli(project_name, project_version, self.args())?;

        // set runtime name from a positional argument
        self.runtime_name = Some(cli.runtime.name.clone());

        Ok(cli)
    }
}

impl Runtime for BasicAuthRuntime {
    fn deploy<'a>(&mut self, ctx: &mut Context<Self>) -> OutputResponse<'a> {
        match config_lookup(ctx) {
            Some(_cs) => async move { Ok(None) }.boxed_local(),
            None => async move {
                Err(ya_runtime_sdk::error::Error::from_string(
                    "Config file not found".to_string(),
                ))
            }
            .boxed_local(),
        }
    }

    fn start<'a>(&mut self, ctx: &mut Context<Self>) -> OutputResponse<'a> {
        let api = ManagementApi::new(&self.client.clone());

        let mut emitter = match ctx.emitter.clone() {
            Some(emitter) => emitter,
            None => {
                let err = anyhow::anyhow!("Not running in server mode");
                return futures::future::err(err.into()).boxed_local();
            }
        };

        async move {
            let (handle, reg) = AbortHandle::new_pair();
            tokio::task::spawn_local(Abortable::new(
                async {
                    let api_cloned = api.clone();
                    loop {
                        let mut total_req = 0_usize;
                        let mut total_users = 0_usize;
                        let mut total_services = 0_usize;

                        if let Ok(services) = api_cloned.get_services().await {
                            total_services = services.iter().count()
                        }

                        if let Ok(users) = self.users.read() {
                            for (s, u) in users.iter() {
                                let us = api_cloned
                                    .get_user_stats(s.name.as_str(), u.username.as_str())
                                    .await;
                                if let Ok(us) = us {
                                    total_users += 1;
                                    total_req += us.requests
                                }
                            }
                        }

                        tokio::time::delay_for(INTERVAL).await;

                        self.global_stats = Some(GlobalStats {
                            users: total_users,
                            services: total_services,
                            requests: Requests { total: total_req },
                        });

                        emitter
                            .counter(RuntimeCounter {
                                name: COUNTER_NAME.to_string(),
                                value: total_req as f64,
                            })
                            .await;
                    }
                },
                reg,
            ));
            self.handle = Some(handle);

            match ya_http_proxy(api).await {
                Ok(()) => Ok(None),
                Err(e) => Err(ya_runtime_sdk::error::Error::from(e)),
            }
        }
        .boxed_local()
    }

    fn stop<'a>(&mut self, ctx: &mut Context<Self>) -> EmptyResponse<'a> {
        let mut total_req = 0_usize;
        if let Some(gs) = self.global_stats.take() {
            total_req = gs.requests.total
        }
        log::info! {"Total requests: {}", total_req}
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
        async move { Ok(()) }.boxed_local()
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
        let api = ManagementApi::new(&self.client.clone());

        async move {
            match ya_http_proxy(api).await {
                Ok(()) => Ok(()),
                Err(e) => Err(ya_runtime_sdk::error::Error::from(e)),
            }
        }
        .boxed_local()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ya_runtime_sdk::run_with::<BasicAuthRuntime, _>(BasicAuthEnv::default()).await
}

fn config_lookup(ctx: &mut Context<BasicAuthRuntime>) -> Option<CreateService> {
    let mut paths = vec![];

    if let Some(path) = dirs::config_dir() {
        paths.push(path.join(env!("CARGO_PKG_NAME")))
    }

    if let Ok(path) = std::env::current_dir() {
        paths.push(path)
    }

    if !paths.is_empty() {
        if let Ok(cs) = find_config(paths, ctx) {
            return cs;
        }
    }

    None
}

fn find_config(
    paths: Vec<PathBuf>,
    ctx: &mut Context<BasicAuthRuntime>,
) -> anyhow::Result<Option<CreateService>> {
    let mut dir_paths = vec![];

    for path in paths {
        let dirs = fs::read_dir(path)?;
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

async fn ya_http_proxy(api: ManagementApi) -> anyhow::Result<()> {
    let mut lock = LockFile::new(with_lock_ext(std::env::current_dir().unwrap_or_default()));
    let timestamp = Instant::now();

    loop {
        match api.get_services().await {
            Ok(_) => {
                break;
            }
            Err(_) => {
                if lock.is_locked() {
                    if Instant::now() - timestamp >= Duration::from_secs(10) {
                        anyhow::bail!("timeout")
                    }
                    tokio::time::delay_for(Duration::from_millis(500)).await;
                    continue;
                }

                lock.lock()?;
                Command::new("ya-http-proxy").kill_on_drop(false);
                lock.unlock()?;

                if let Err(e) = api.get_services().await {
                    anyhow::bail!(e.to_string());
                }
                break;
            }
        }
    }

    Ok(())
}
