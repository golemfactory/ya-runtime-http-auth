mod lock;
mod proxy;

use std::collections::HashMap;
use std::fs::{read_dir, File};
use std::io::BufReader;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use futures::future::{AbortHandle, Abortable};
use futures::FutureExt;
use structopt::StructOpt;
use tokio::sync::RwLock;

use http::StatusCode;
use ya_http_proxy_client::api::ManagementApi;
use ya_http_proxy_client::web::WebClient;
use ya_http_proxy_client::Error;
use ya_http_proxy_model::{CreateService, CreateUser, GlobalStats, UserStats};
use ya_runtime_sdk::cli::parse_cli;
use ya_runtime_sdk::env::Env;
use ya_runtime_sdk::*;

type RuntimeCli = <BasicAuthRuntime as RuntimeDef>::Cli;

const COUNTER_NAME: &str = "golem.runtime.http-auth.requests.counter";
const INTERVAL: Duration = Duration::from_secs(2);

#[derive(Default, RuntimeDef)]
#[cli(BasicAuthCli)]
pub struct BasicAuthRuntime {
    basic_auth: Rc<RwLock<BasicAuth>>,
}

#[derive(Default)]
pub struct BasicAuth {
    client: WebClient,
    handle: Option<AbortHandle>,
    users: HashMap<CreateService, CreateUser>,
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
        let emitter = match ctx.emitter.clone() {
            Some(emitter) => emitter,
            None => {
                let err = anyhow::anyhow!("Not running in server mode");
                return futures::future::err(err.into()).boxed_local();
            }
        };

        let service = match config_lookup(ctx) {
            Some(service) => service,
            None => {
                let err = anyhow::anyhow!("Config file not found");
                return futures::future::err(err.into()).boxed_local();
            }
        };

        let p1 = self.basic_auth.clone();
        let p2 = self.basic_auth.clone();

        async move {
            let (h, reg) = AbortHandle::new_pair();
            let api = ManagementApi::new(&p1.read().await.client);
            tokio::task::spawn_local(Abortable::new(
                async move {
                    loop {
                        let mut total_req = 0_usize;
                        let mut total_users = 0_usize;
                        let mut total_services = 0_usize;

                        if let Ok(services) = api.get_services().await {
                            total_services = services.iter().len()
                        }

                        for (s, u) in p1.read().await.users.iter() {
                            let us = api
                                .get_user_stats(s.name.as_str(), u.username.as_str())
                                .await;
                            if let Ok(us) = us {
                                total_users += 1;
                                total_req += us.requests
                            }
                        }

                        tokio::time::delay_for(INTERVAL).await;

                        p1.write().await.global_stats.replace(GlobalStats {
                            users: total_users,
                            services: total_services,
                            requests: UserStats {
                                requests: total_req,
                            },
                        });

                        emit_counter(COUNTER_NAME.to_string(), emitter.clone(), total_req as f64)
                            .await;
                    }
                },
                reg,
            ));
            p2.write().await.handle.replace(h);

            let api = ManagementApi::new(&p2.read().await.client);

            match proxy::spawn(api).await {
                Ok(()) => {
                    let api = ManagementApi::new(&p2.read().await.client);
                    match try_create_service(api, service).await {
                        Ok(()) => Ok(None),
                        Err(err) => Err(ya_runtime_sdk::error::Error::from(err)),
                    }
                }
                Err(err) => Err(ya_runtime_sdk::error::Error::from(err)),
            }
        }
        .boxed_local()
    }

    fn stop<'a>(&mut self, ctx: &mut Context<Self>) -> EmptyResponse<'a> {
        let emitter = match ctx.emitter.clone() {
            Some(emitter) => emitter,
            None => {
                let err = anyhow::anyhow!("Not running in server mode");
                return futures::future::err(err.into()).boxed_local();
            }
        };

        let p = self.basic_auth.clone();

        async move {
            let mut total_req = 0_usize;
            let api = ManagementApi::new(&p.read().await.client);

            for (s, u) in p.read().await.users.iter() {
                let service_name = s.name.as_str();
                let username = u.username.as_str();
                let us = api.get_user_stats(service_name, username).await;
                if let Ok(us) = us {
                    total_req += us.requests;
                    api.delete_user(service_name, username).await.ok();
                }
            }

            emit_counter(COUNTER_NAME.to_string(), emitter.clone(), total_req as f64).await;

            if let Some(handle) = &p.read().await.handle {
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

    fn offer<'a>(&mut self, ctx: &mut Context<Self>) -> OutputResponse<'a> {
        let service = match config_lookup(ctx) {
            Some(service) => service,
            None => {
                let err = anyhow::anyhow!("Config file not found");
                return futures::future::err(err.into()).boxed_local();
            }
        };

        async move {
            Ok(Some(crate::serialize::json::json!({
            "golem.runtime.http-auth.meta": "",
            "golem.runtime.http-auth.service.cpu-threads": service.cpu_threads
            })))
        }
        .boxed_local()
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
        let p = self.basic_auth.clone();

        async move {
            let api = ManagementApi::new(&p.read().await.client);
            match proxy::spawn(api).await {
                Ok(()) => Ok(()),
                Err(e) => Err(ya_runtime_sdk::error::Error::from(e)),
            }
        }
        .boxed_local()
    }
}

#[actix_rt::main]
async fn main() -> anyhow::Result<()> {
    ya_runtime_sdk::run_local_with::<BasicAuthRuntime, _>(BasicAuthEnv::default()).await
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

async fn emit_counter(counter_name: String, mut emitter: EventEmitter, value: f64) {
    emitter
        .counter(RuntimeCounter {
            name: counter_name,
            value,
        })
        .await;
}

async fn try_create_service(api: ManagementApi, service: CreateService) -> anyhow::Result<()> {
    if let Err(err) = api.create_service(&service).await {
        match err {
            Error::SendRequestError {
                code: StatusCode::CONFLICT,
                ..
            } => {
                let s = api.get_service(service.name.as_str()).await?;
                if s.inner != service {
                    anyhow::bail!(err);
                }
            }
            err => anyhow::bail!(err),
        }
    }
    Ok(())
}
