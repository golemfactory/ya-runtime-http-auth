mod lock;

use futures::future::{AbortHandle, Abortable};
use futures::FutureExt;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};
use structopt::StructOpt;
use tokio::process::Command;
use tokio::sync::RwLock;

use crate::lock::{with_lock_ext, LockFile};
use ya_http_proxy_client::api::ManagementApi;
use ya_http_proxy_client::model::{CreateService, CreateUser, GlobalStats, UserStats};
use ya_http_proxy_client::web::WebClient;
use ya_runtime_sdk::cli::parse_cli;
use ya_runtime_sdk::env::Env;
use ya_runtime_sdk::*;

type RuntimeCli = <BasicAuthRuntime as RuntimeDef>::Cli;

const COUNTER_NAME: &str = "golem.runtime.http-auth.requests.counter";
const INTERVAL: Duration = Duration::from_secs(2);
const TIMEOUT: Duration = Duration::from_secs(10);
const SLEEP: Duration = Duration::from_millis(500);

#[derive(Default, RuntimeDef)]
#[cli(BasicAuthCli)]
pub struct BasicAuthRuntime {
    client: Rc<RwLock<WebClient>>,
    handle: Rc<RwLock<Option<AbortHandle>>>,
    users: Rc<RwLock<HashMap<CreateService, CreateUser>>>,
    global_stats: Rc<RwLock<Option<GlobalStats>>>,
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
        let mut emitter = match ctx.emitter.clone() {
            Some(emitter) => emitter,
            None => {
                let err = anyhow::anyhow!("Not running in server mode");
                return futures::future::err(err.into()).boxed_local();
            }
        };

        let users = self.users.clone();
        let global_stats = self.global_stats.clone();
        let handle = self.handle.clone();
        let c1 = self.client.clone();
        let c2 = self.client.clone();

        async move {
            let (h, reg) = AbortHandle::new_pair();
            tokio::task::spawn_local(Abortable::new(
                async move {
                    let client = c1.read().await;
                    let api = ManagementApi::new(&client);
                    loop {
                        let mut total_req = 0_usize;
                        let mut total_users = 0_usize;
                        let mut total_services = 0_usize;

                        if let Ok(services) = api.get_services().await {
                            total_services = services.iter().len()
                        }

                        for (s, u) in users.read().await.iter() {
                            let us = api
                                .get_user_stats(s.name.as_str(), u.username.as_str())
                                .await;
                            if let Ok(us) = us {
                                total_users += 1;
                                total_req += us.requests
                            }
                        }

                        tokio::time::delay_for(INTERVAL).await;

                        global_stats.write().await.replace(GlobalStats {
                            users: total_users,
                            services: total_services,
                            requests: UserStats {
                                requests: total_req,
                            },
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
            handle.write().await.replace(h);

            let client = c2.read().await;
            let api2 = ManagementApi::new(&client);

            match ya_http_proxy(api2).await {
                Ok(()) => Ok(None),
                Err(e) => Err(ya_runtime_sdk::error::Error::from(e)),
            }
        }
        .boxed_local()
    }

    fn stop<'a>(&mut self, _ctx: &mut Context<Self>) -> EmptyResponse<'a> {
        let handle = self.handle.clone();

        async move {
            //raz jeszcze emitter
            if let Some(handle) = handle.read().await.clone() {
                handle.abort();
            }

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
        let c = self.client.clone();
        async move {
            let client = c.read().await;
            let api = ManagementApi::new(&client);
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
                    if Instant::now() - timestamp >= TIMEOUT {
                        anyhow::bail!("timeout")
                    }
                    tokio::time::delay_for(SLEEP).await;
                    continue;
                }

                lock.lock()?;
                let _child = Command::new("ya-http-proxy").kill_on_drop(false).spawn()?;
                lock.unlock()?;

                if let Err(e) = api.get_services().await {
                    // let pid = child.id();
                    // if pid == ... {
                    //     //ponowic sprawdzenie
                    //     if Instant::now() - timestamp >= Duration::from_secs(10) {
                    //         anyhow::bail!("timeout")
                    //     }
                    //     tokio::time::delay_for(Duration::from_millis(500)).await;
                    // }
                    anyhow::bail!(e.to_string());
                }
                break;
            }
        }
    }

    Ok(())
}
