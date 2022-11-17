use std::net;
use std::net::{IpAddr, Ipv4Addr};

use anyhow::Result;

use clap::{Parser, Subcommand};
use ya_http_proxy_model::{Addresses, CreateService, CreateUser, Service};

fn print_service(service: &Service) {
    eprintln!("name:     {:20}", service.inner.name);
    eprintln!("from:     {:20}", service.inner.from);
    eprintln!("to:       {:20}", service.inner.to);
    eprintln!("[servers: {:?}]", service.inner.server_name);
    eprintln!("[http: {:?}]", service.inner.bind_http);
    eprintln!("[https: {:?}]", service.inner.bind_https);
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub command: Commands,
}

impl Args {
    async fn run(&self) -> Result<()> {
        self.command.run().await
    }
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// does testing things
    Service {
        #[command(subcommand)]
        command: ServiceCommands,
    },
    User {
        service: String,
        #[command(subcommand)]
        command: UserCommands,
    },
}

impl Commands {
    async fn run(&self) -> Result<()> {
        match self {
            Self::Service { command } => command.run().await?,
            Self::User { service, command } => command.run(service).await?,
        }
        Ok(())
    }
}

#[derive(Subcommand, Debug)]
pub enum ServiceCommands {
    /// does testing things
    List {},
    Add {
        name: String,
        port: u16,
        from: String,
        to: String,
    },
    Delete {
        name: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum UserCommands {
    /// does testing things
    List {},
    Add {
        user: String,
        pass: String,
    },
    Delete {
        name: String,
    },
}

impl ServiceCommands {
    async fn run(&self) -> Result<()> {
        let api = ya_http_proxy_client::ManagementApi::try_default()?;
        match self {
            Self::List {} => {
                eprintln!("{:?}", api.get_services().await?);
            }
            Self::Add {
                name,
                from,
                to,
                port,
            } => {
                let s = api
                    .create_service(&CreateService {
                        name: name.clone(),
                        server_name: vec![format!("box.local:{port}")],
                        bind_https: None,
                        bind_http: Some(Addresses::new([
                            (std::net::Ipv4Addr::UNSPECIFIED, *port).into()
                        ])),
                        cert: None,
                        auth: None,
                        from: from.parse()?,
                        to: to.parse()?,
                        timeouts: None,
                        cpu_threads: None,
                        user: None,
                    })
                    .await?;
                print_service(&s);
            }
            Self::Delete { name } => api.delete_service(name).await?,
        }
        Ok(())
    }
}

impl UserCommands {
    async fn run(&self, service: &str) -> Result<()> {
        let api = ya_http_proxy_client::ManagementApi::try_default()?;
        match self {
            Self::Delete { name } => api.delete_user(service, name).await?,
            Self::Add { user, pass } => {
                let user = api
                    .create_user(
                        service,
                        &CreateUser {
                            username: user.to_string(),
                            password: pass.to_string(),
                        },
                    )
                    .await?;
                eprintln!("{user:?}");
            }
            Self::List {} => {
                let users = api.get_users(service).await?;
                for user in users {
                    eprintln!("{:?}", user);
                }
            }
        }
        Ok(())
    }
}

#[actix_rt::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    args.run().await
}
