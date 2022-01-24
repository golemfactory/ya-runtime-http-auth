use crate::model::CreateService;
use futures::FutureExt;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use structopt::StructOpt;
use ya_runtime_sdk::cli::parse_cli;
use ya_runtime_sdk::env::Env;
use ya_runtime_sdk::*;

mod model;

type RuntimeCli = <BasicAuthRuntime as RuntimeDef>::Cli;

#[derive(Default, RuntimeDef)]
#[cli(BasicAuthCli)]
pub struct BasicAuthRuntime;

#[derive(Default)]
pub struct BasicAuthEnv {
    runtime_name: Option<String>,
}

#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub struct BasicAuthCli {
    name: Option<String>,
}

impl Env<RuntimeCli> for BasicAuthEnv {
    fn runtime_name(&self) -> Option<String> {
        self.runtime_name.clone()
    }

    fn cli(&mut self, project_name: &str, project_version: &str) -> anyhow::Result<RuntimeCli> {
        let cli: RuntimeCli = parse_cli(project_name, project_version, self.args())?;

        if cli.runtime.name.is_some() {
            // set runtime name from a positional argument
            self.runtime_name = cli.runtime.name.clone();
        }

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
        //
        // let cs = path1
        //     .read_dir()
        //     .expect("Reading directory failed")
        //     .map(|p| read_config_from_file(p.unwrap().path()).expect("Parsing config file failed"))
        //     .find(|cs| cs.name.clone().eq(&ctx.env.runtime_name().unwrap()));
    }

    fn start<'a>(&mut self, _ctx: &mut Context<Self>) -> OutputResponse<'a> {
        async move { Ok(None) }.boxed_local()
    }

    fn stop<'a>(&mut self, _ctx: &mut Context<Self>) -> EmptyResponse<'a> {
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ya_runtime_sdk::run_with::<BasicAuthRuntime, _>(BasicAuthEnv::default()).await
}

fn read_config_from_file(path: PathBuf) -> anyhow::Result<CreateService> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let cs = serde_json::from_reader(reader)?;

    Ok(cs)
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
        if let Ok(cs) = check_paths(paths, ctx) {
            return cs
        }
    }

    None
}

fn check_paths(paths: Vec<PathBuf>, ctx: &mut Context<BasicAuthRuntime>) -> anyhow::Result<Option<CreateService>> {
    let mut dir_paths: Vec<PathBuf> = paths
        .iter()
        .map(|p| fs::read_dir(p).unwrap()
            .map(|d| d.unwrap().path()).collect())
        .collect();

    dir_paths = dir_paths
        .into_iter()
        .filter(|p: &PathBuf| match p.extension() {
            Some(ext) => ext == "json",
            None => false,
        })
        .collect();

    let cs = dir_paths
        .into_iter()
        .map(|p| read_config_from_file(p).unwrap())
        .find(|cs| cs.name == ctx.env.runtime_name().unwrap());

    Ok(cs)
}
