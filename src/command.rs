use derive_more::From;
use futures::TryFutureExt;
use serde::Serialize;
use structopt::StructOpt;
use strum::VariantNames;

use ya_http_proxy_model::{AuthMethod, CreateUser, PubService, User, UserEndpointStats};
use ya_runtime_sdk::error::Error as SdkError;

use crate::HttpAuth;

#[derive(Clone, Debug, Eq, PartialEq, StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub enum RuntimeCommand {
    User(UserCommand),
    #[structopt(alias = "svc")]
    Service(ServiceCommand),
}

impl RuntimeCommand {
    pub fn new(args: Vec<String>) -> Result<Self, SdkError> {
        let args = std::iter::once("run".to_string()).chain(args.into_iter());
        Self::from_iter_safe(args).map_err(SdkError::from_string)
    }

    pub async fn execute(
        self,
        service_name: String,
        rt: &mut HttpAuth,
    ) -> Result<RuntimeCommandOutput, SdkError> {
        match self {
            Self::User(cmd) => cmd.execute(service_name, rt).await.map(Into::into),
            Self::Service(cmd) => cmd.execute(service_name, rt).await.map(Into::into),
        }
    }
}

#[derive(Clone, Debug, Serialize, From)]
#[serde(untagged)]
pub enum RuntimeCommandOutput {
    User(UserCommandOutput),
    Service(ServiceCommandOutput),
}

#[derive(Clone, Debug, Eq, PartialEq, StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub enum UserCommand {
    Add {
        username: String,
        password: String,
        #[structopt(
            long, short,
            possible_values = AuthMethod::VARIANTS,
            default_value = AuthMethod::Basic.into(),
        )]
        auth: AuthMethod,
    },
    Remove {
        username: String,
        #[structopt(
            long, short,
            possible_values = AuthMethod::VARIANTS,
            default_value = AuthMethod::Basic.into(),
        )]
        auth: AuthMethod,
    },
    List,
    Stats {
        username: String,
    },
}

#[derive(Clone, Debug, Serialize, From)]
#[serde(untagged)]
pub enum UserCommandOutput {
    None,
    User(User),
    Users(Vec<User>),
    Stats(UserEndpointStats),
}

impl UserCommand {
    pub async fn execute(
        self,
        service_name: String,
        rt: &mut HttpAuth,
    ) -> Result<UserCommandOutput, SdkError> {
        match self {
            Self::Add {
                username,
                password,
                auth: _,
            } => {
                let user = rt
                    .api
                    .create_user(&service_name, &CreateUser { username, password })
                    .map_err(SdkError::from_string)
                    .await?;
                rt.users.insert(user.username.clone(), user.clone());

                Ok(user.into())
            }
            Self::Remove { username, auth: _ } => {
                rt.api
                    .delete_user(&service_name, &username)
                    .map_err(SdkError::from_string)
                    .await?;
                rt.users.remove(&username);

                Ok(().into())
            }
            Self::List => {
                let users = rt
                    .api
                    .get_users(&service_name)
                    .map_err(SdkError::from_string)
                    .await?;

                Ok(users.into())
            }
            Self::Stats { username } => {
                let stats = rt
                    .api
                    .get_endpoint_user_stats(&service_name, &username)
                    .map_err(SdkError::from_string)
                    .await?;

                Ok(stats.into())
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub enum ServiceCommand {
    Info {},
}

#[derive(Clone, Debug, Serialize, From)]
#[serde(untagged)]
pub enum ServiceCommandOutput {
    None,
    Service(PubService),
}

impl ServiceCommand {
    pub async fn execute(
        self,
        service_name: String,
        rt: &mut HttpAuth,
    ) -> Result<ServiceCommandOutput, SdkError> {
        match self {
            Self::Info {} => {
                let service: PubService = rt
                    .api
                    .get_service(&service_name)
                    .map_err(SdkError::from_string)
                    .await?
                    .into();

                Ok(service.into())
            }
        }
    }
}
