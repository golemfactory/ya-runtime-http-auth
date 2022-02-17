mod command;
mod config;
mod lock;
mod proxy;

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use futures::future::{AbortHandle, Abortable};
use futures::{FutureExt, StreamExt};
use http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_default::DefaultFromSerde;
use structopt::StructOpt;
use tokio::sync::RwLock;

use ya_http_proxy_client::api::ManagementApi;
use ya_http_proxy_client::web::{WebClient, DEFAULT_MANAGEMENT_API_URL};
use ya_http_proxy_client::Error;
use ya_http_proxy_model::{CreateService, GlobalStats, Service, User};
use ya_runtime_sdk::cli::parse_cli;
use ya_runtime_sdk::env::Env;
use ya_runtime_sdk::error::Error as SdkError;
use ya_runtime_sdk::serialize::json;
use ya_runtime_sdk::*;

use crate::command::RuntimeCommand;

type RuntimeCli = <HttpAuthRuntime as RuntimeDef>::Cli;

pub const PROPERTY_PREFIX: &str = "golem.runtime.http-auth.meta";
const COUNTER_NAME: &str = "http-auth.requests";
const COUNTER_PUBLISH_INTERVAL: Duration = Duration::from_secs(2);

const MANAGEMENT_API_URL_ENV_VAR: &str = "MANAGEMENT_API_URL";
const MANAGEMENT_API_MAX_CONCURRENT_REQUESTS: usize = 3;

#[derive(RuntimeDef)]
#[cli(HttpAuthCli)]
#[conf(HttpAuthConf)]
pub struct HttpAuthRuntime {
    http_auth: Rc<RwLock<HttpAuth>>,
}

impl From<ManagementApi> for HttpAuthRuntime {
    fn from(api: ManagementApi) -> Self {
        let http_auth = Rc::new(RwLock::new(HttpAuth {
            api,
            handle: Default::default(),
            service: Default::default(),
            users: Default::default(),
            global_stats: Default::default(),
        }));
        Self { http_auth }
    }
}

pub struct HttpAuth {
    api: ManagementApi,
    handle: Option<AbortHandle>,
    service: Option<Service>,
    users: HashMap<String, User>,
    global_stats: GlobalStats,
}

impl HttpAuth {
    pub async fn count_requests(&self) -> usize {
        let service_name = match self.service {
            Some(ref service) => &service.inner.name,
            None => return 0,
        };

        futures::stream::iter(self.users.keys())
            .map(|username| self.api.get_user_stats(service_name, username))
            .buffer_unordered(MANAGEMENT_API_MAX_CONCURRENT_REQUESTS)
            .filter_map(|r| async move { r.ok() })
            .fold(0, |mut acc, stats| async move {
                acc += stats.requests;
                acc
            })
            .await
    }

    pub async fn delete_users(&self) {
        let service_name = match self.service {
            Some(ref service) => &service.inner.name,
            None => return,
        };

        let total = self.users.len();
        let failed = futures::stream::iter(self.users.keys())
            .map(|username| self.api.delete_user(service_name, username))
            .buffer_unordered(MANAGEMENT_API_MAX_CONCURRENT_REQUESTS)
            .filter_map(|result| async move { result.err() })
            .count()
            .await;

        if failed > 0 {
            log::error!("Failed to remove {} out of {} users", failed, total);
        }
    }
}

#[derive(Default)]
pub struct HttpAuthEnv {
    runtime_name: Option<String>,
}

#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub struct HttpAuthCli {
    name: String,
}

#[derive(Deserialize, Serialize, DefaultFromSerde)]
#[serde(rename_all = "camelCase")]
pub struct HttpAuthConf {
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    #[serde(default = "default_management_api_url")]
    pub management_api_url: String,
    #[serde(default)]
    pub service_lookup_dirs: Vec<PathBuf>,
}

fn default_data_dir() -> PathBuf {
    let crate_name = env!("CARGO_PKG_NAME");
    match dirs::data_dir() {
        Some(dir) => dir,
        None => std::env::temp_dir(),
    }
    .join(crate_name)
}

fn default_management_api_url() -> String {
    std::env::var(MANAGEMENT_API_URL_ENV_VAR)
        .unwrap_or_else(|_| DEFAULT_MANAGEMENT_API_URL.to_string())
}

impl Env<RuntimeCli> for HttpAuthEnv {
    fn runtime_name(&self) -> Option<String> {
        self.runtime_name.clone()
    }

    fn cli(&mut self, project_name: &str, project_version: &str) -> anyhow::Result<RuntimeCli> {
        let cli: RuntimeCli = parse_cli(project_name, project_version, self.args())?;
        self.runtime_name = Some(cli.runtime.name.clone());
        Ok(cli)
    }
}

impl Runtime for HttpAuthRuntime {
    fn deploy<'a>(&mut self, ctx: &mut Context<Self>) -> OutputResponse<'a> {
        if config::lookup(ctx).is_none() {
            return SdkError::response("Config file not found");
        }

        if std::fs::create_dir_all(&ctx.conf.data_dir).is_err() {
            return SdkError::response(format!(
                "Cannot create data directory: {}",
                ctx.conf.data_dir.display()
            ));
        }

        async move { Ok(None) }.boxed_local()
    }

    fn start<'a>(&mut self, ctx: &mut Context<Self>) -> OutputResponse<'a> {
        let emitter = match ctx.emitter.clone() {
            Some(emitter) => emitter,
            None => return SdkError::response("Not running in server mode"),
        };
        let service = match config::lookup(ctx) {
            Some(service) => service,
            None => return SdkError::response("Config file not found"),
        };

        let data_dir = ctx.conf.data_dir.clone();
        let http_auth = self.http_auth.clone();
        async move {
            let api = {
                let inner = http_auth.read().await;
                inner.api.clone()
            };

            proxy::spawn(api.clone(), data_dir).await?;
            let service = try_create_service(api.clone(), service.inner.clone()).await?;
            let (h, reg) = AbortHandle::new_pair();
            {
                let mut inner = http_auth.write().await;
                inner.service.replace(service);
                inner.handle.replace(h);
            }

            tokio::task::spawn_local(Abortable::new(
                async move {
                    loop {
                        let total_req = {
                            let inner = http_auth.read().await;
                            inner.count_requests().await
                        };

                        if let Ok(stats) = api.get_global_stats().await {
                            http_auth.write().await.global_stats = stats;
                        }

                        emit_counter(COUNTER_NAME.to_string(), emitter.clone(), total_req as f64)
                            .await;

                        tokio::time::delay_for(COUNTER_PUBLISH_INTERVAL).await;
                    }
                },
                reg,
            ));

            Ok(None)
        }
        .boxed_local()
    }

    fn stop<'a>(&mut self, ctx: &mut Context<Self>) -> EmptyResponse<'a> {
        let emitter = match ctx.emitter.clone() {
            Some(emitter) => emitter,
            None => return SdkError::response("Not running in server mode"),
        };

        let inner = self.http_auth.clone();
        async move {
            let inner = inner.read().await;
            if let Some(handle) = &inner.handle {
                handle.abort();
            };

            let total_req = inner.count_requests().await;
            inner.delete_users().await;
            drop(inner);

            emit_counter(COUNTER_NAME.to_string(), emitter.clone(), total_req as f64).await;
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
        let http_auth = self.http_auth.clone();

        ctx.command(|_| async move {
            let mut http_auth = http_auth.write().await;
            let service_name = http_auth
                .service
                .as_ref()
                .map(|s| s.inner.name.clone())
                .ok_or_else(|| SdkError::from_string("Service not running"))?;

            let cmd = RuntimeCommand::new(cmd.args)?;
            cmd.execute(service_name, &mut http_auth).await
        })
    }

    fn offer<'a>(&mut self, ctx: &mut Context<Self>) -> OutputResponse<'a> {
        let service = match config::lookup(ctx) {
            Some(service) => service,
            None => return SdkError::response("Config file not found"),
        };

        let result = service.offer_properties(PROPERTY_PREFIX);
        let cpu_threads = service.inner.cpu_threads;

        async move {
            use anyhow::Context;

            let mut output = result?;
            let object = output
                .as_object_mut()
                .context("Programming error: offer properties are not a map")?;

            if let Some(cpu_threads) = cpu_threads {
                object.insert(
                    format!("{}.cpu-threads", PROPERTY_PREFIX),
                    json::Value::Number(cpu_threads.into()),
                );
            }

            Ok(Some(output))
        }
        .boxed_local()
    }

    fn test<'a>(&mut self, ctx: &mut Context<Self>) -> EmptyResponse<'a> {
        let offer = self.offer(ctx);
        let inner = self.http_auth.clone();

        async move {
            offer.await?;

            let inner = inner.read().await;
            let api = inner.api.clone();
            proxy::spawn(api, std::env::temp_dir())
                .await
                .map_err(Into::into)
        }
        .boxed_local()
    }
}

fn main() -> anyhow::Result<()> {
    let runtime =
        ya_runtime_sdk::build::<HttpAuthRuntime, _, _, _>(HttpAuthEnv::default(), move |ctx| {
            let api_url = ctx.conf.management_api_url.clone();
            async move {
                let client = WebClient::new(api_url)?;
                let api = ManagementApi::new(client);
                Ok(HttpAuthRuntime::from(api))
            }
        });

    let mut system = actix_rt::System::new("runtime");
    system.block_on(runtime)
}

async fn emit_counter(counter_name: String, mut emitter: EventEmitter, value: f64) {
    emitter
        .counter(RuntimeCounter {
            name: counter_name,
            value,
        })
        .await;
}

async fn try_create_service(
    api: ManagementApi,
    create_service: CreateService,
) -> anyhow::Result<Service> {
    match api.create_service(&create_service).await {
        Err(
            err @ Error::SendRequestError {
                code: StatusCode::CONFLICT,
                ..
            },
        ) => {
            let service = api.get_service(create_service.name.as_str()).await?;
            if service.inner != create_service {
                anyhow::bail!(err);
            }
            Ok(service)
        }
        result => result.map_err(Into::into),
    }
}
