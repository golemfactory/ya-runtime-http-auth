use futures::{FutureExt, TryFutureExt};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::fs::OpenOptions;
use tokio::process::{Child, Command};
use ya_runtime_sdk::*;

#[derive(Deserialize, Serialize)]
pub struct BasicAuthConf {
    service_prefix: String,
    public_addr: String,
    passwd_tool_path: String,
    passwd_file_path: String,
    password_default_length: usize,
}

impl Default for BasicAuthConf {
    fn default() -> Self {
        BasicAuthConf {
            service_prefix: "yagna_service".to_string(),
            public_addr: "http://yagna.service:8080".to_string(),
            passwd_tool_path: "htpasswd".to_string(),
            passwd_file_path: "/etc/nginx/htpasswd".to_string(),
            password_default_length: 15,
        }
    }
}

#[derive(Default, RuntimeDef)]
#[conf(BasicAuthConf)]
pub struct BasicAuthRuntime {
    username: Option<String>,
    password: Option<String>,
}

impl Runtime for BasicAuthRuntime {
    fn deploy<'a>(&mut self, ctx: &mut Context<Self>) -> OutputResponse<'a> {
        let path = PathBuf::from(&ctx.conf.passwd_file_path);

        async move {
            let _ = touch(&path)
                .map_err(|_| {
                    error::Error::from_string(
                        "Wrong path to passwd_file_path: ".to_owned() + path.to_str().unwrap(),
                    )
                })
                .await?;
            Ok(None)
        }
        .boxed_local()
    }

    fn start<'a>(&mut self, ctx: &mut Context<Self>) -> OutputResponse<'a> {
        // Generate user & password entry with passwd tool
        let username = format!("{}_{}", ctx.conf.service_prefix, std::process::id());

        let rng = thread_rng();
        let password: String = rng
            .sample_iter(Alphanumeric)
            .map(char::from)
            .take(ctx.conf.password_default_length)
            .collect();
        self.username = Some(username.clone());
        self.password = Some(password.clone());

        let passwd_tool_path = ctx.conf.passwd_tool_path.clone();
        let passwd_file_path = ctx.conf.passwd_file_path.clone();

        let auth_data = serialize::json::json!(
            {
                "service": &ctx.conf.service_prefix,
                "url": &ctx.conf.public_addr,
                "auth": {
                    "user": &self.username,
                    "password": &self.password
                }
            }
        );

        async move {
            add_user_to_pass_file(&passwd_tool_path, &passwd_file_path, &username, &password)
                .map_err(|_| error::Error::from_string("Unable to add entry in passwd file"))?
                .await?;

            Ok(Some(auth_data))
        }
        .boxed_local()
    }

    fn stop<'a>(&mut self, ctx: &mut Context<Self>) -> EmptyResponse<'a> {
        let passwd_tool_path = ctx.conf.passwd_tool_path.clone();
        let passwd_file_path = ctx.conf.passwd_file_path.clone();
        let username = self.username.as_ref().unwrap().clone();
        async move {
            remove_user_from_pass_file(&passwd_tool_path, &passwd_file_path, &username)
                .map_err(|_| error::Error::from_string("Unable to remove entry from passwd file"))?
                .await?;
            Ok(())
        }
        .boxed_local()
    }

    fn run_command<'a>(
        &mut self,
        _cmd: RunProcess,
        _mode: RuntimeMode,
        ctx: &mut Context<Self>,
    ) -> ProcessIdResponse<'a> {
        let auth_data = serialize::json::json!(
            {
                "service": &ctx.conf.service_prefix,
                "url": &ctx.conf.public_addr,
                "auth": {
                    "user": &self.username,
                    "password": &self.password
                }
            }
        );

        ctx.command(|mut run_ctx| async move {
            run_ctx.stdout(format!("{}", auth_data)).await;
            Ok(())
        })
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ya_runtime_sdk::run::<BasicAuthRuntime>().await
}

fn add_user_to_pass_file(
    passwd_bin: &str,
    passwd_file: &str,
    username: &str,
    password: &str,
) -> std::io::Result<Child> {
    Command::new(passwd_bin)
        .arg("-b")
        .arg(passwd_file)
        .arg(username)
        .arg(password)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

fn remove_user_from_pass_file(
    passwd_bin: &str,
    passwd_file: &str,
    username: &str,
) -> std::io::Result<Child> {
    Command::new(passwd_bin)
        .arg("-D")
        .arg(passwd_file)
        .arg(username)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

// A simple implementation of `touch path` (ignores existing files)
async fn touch(path: &Path) -> io::Result<()> {
    OpenOptions::new()
        .create(true)
        .write(true)
        .open(path)
        .await
        .map(|_| ())
}
