#![allow(unused)]

use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Result;
use awc::Connector;
use http::{Method, Uri};
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
use serde::{Deserialize, Serialize};

use ya_http_proxy::{Management, ProxyConf, ProxyManager};
use ya_http_proxy_model as model;

struct WebClient {
    url: Uri,
    inner: awc::Client,
    credentials: Option<(String, String)>,
}

impl WebClient {
    pub fn new(url: String) -> Result<Self> {
        Ok(Self {
            url: url.parse()?,
            inner: awc::Client::new(),
            credentials: None,
        })
    }

    pub fn new_permissive_tls(url: String, username: &str, password: &str) -> Result<Self> {
        let mut builder = SslConnector::builder(SslMethod::tls_client())?;
        builder.set_verify(SslVerifyMode::NONE);
        let connector = Connector::new().openssl(builder.build());
        let inner = awc::Client::builder().connector(connector).finish();

        Ok(Self {
            url: url.parse()?,
            inner,
            credentials: Some((username.to_string(), password.to_string())),
        })
    }

    pub async fn get<R, S>(&self, uri: S) -> Result<R>
    where
        R: for<'de> Deserialize<'de>,
        S: AsRef<str>,
    {
        self.request::<(), R, S>(Method::GET, uri, None).await
    }

    pub async fn post<P, R, S>(&self, uri: S, payload: &P) -> Result<R>
    where
        P: Serialize,
        R: for<'de> Deserialize<'de>,
        S: AsRef<str>,
    {
        self.request(Method::POST, uri, Some(payload)).await
    }

    pub async fn delete<S>(&self, uri: S) -> Result<()>
    where
        S: AsRef<str>,
    {
        self.request::<(), (), S>(Method::DELETE, uri, None).await
    }

    async fn request<P, R, S>(&self, method: Method, uri: S, payload: Option<&P>) -> Result<R>
    where
        P: Serialize,
        R: for<'de> Deserialize<'de>,
        S: AsRef<str>,
    {
        let uri = uri.as_ref();
        let url = format!("{}{}", self.url, uri);

        let mut req = self.inner.request(method, &url);
        if let Some((username, password)) = self.credentials.as_ref() {
            req = req.basic_auth(username, password);
        }

        let mut res = match payload {
            Some(payload) => req.send_json(payload),
            None => req.send(),
        }
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        if !res.status().is_success() {
            anyhow::bail!("{} {}", url, res.status().as_str());
        }
        Ok(res.json().await?)
    }
}

fn default_proxy_conf() -> Result<ProxyConf> {
    let cert_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?)
        .join("tests")
        .join("resources");
    let cert_store_path = cert_dir.join("server.cert");
    let cert_key_path = cert_dir.join("server.key");

    let mut conf = ProxyConf::default();
    conf.server.addr = SocketAddr::from(([127, 0, 0, 1], 8080));
    conf.server.server_cert.server_cert_store_path = Some(cert_store_path);
    conf.server.server_cert.server_key_path = Some(cert_key_path);

    Ok(conf)
}

async fn e2e() -> anyhow::Result<()> {
    let log_level = env::var("RUST_LOG").unwrap_or_else(|_| "info".into());
    env::set_var("RUST_LOG", &log_level);
    env_logger::init();

    let service_name = "test-service".to_string();
    let service_addr: SocketAddr = "127.0.0.1:8443".parse()?;
    let service_endpoint = "/test".to_string();
    let service_url = format!(
        "https://localhost:{}{}",
        service_addr.port(),
        service_endpoint
    );

    let fwd_service_addr = service::spawn("127.0.0.1:0".to_string()).await?;
    let fwd_service_url = format!("http://{}/resource", fwd_service_addr);

    let proxy_conf = default_proxy_conf()?;
    let mut management = Management::new(ProxyManager::new(proxy_conf.clone()));
    management.bind("127.0.0.1:9090".parse()?)?;
    let management_url = format!("http://{}", management.local_addr()?);

    tokio::task::spawn(async move {
        if let Err(e) = management.await {
            panic!("Management API server error: {}", e);
        }
        println!("Management API server stopped");
    });

    let user_name = "user1".to_string();
    let password = "password123".to_string();

    let create_service = model::CreateService {
        name: service_name.clone(),
        bind: service_addr,
        cert: Default::default(),
        auth: Some(model::Auth {
            method: model::AuthMethod::Basic,
        }),
        from: service_endpoint,
        to: fwd_service_url.parse()?,
        timeouts: None,
        user: None,
        cpu_threads: Some(2),
    };
    let create_user = model::CreateUser {
        username: user_name.clone(),
        password: password.clone(),
    };

    let client = WebClient::new(management_url)?;

    let services_get: Vec<model::Service> = client.get("services").await?;
    assert_eq!(0, services_get.len());

    println!("[s] Creating a new service");
    let service_post: model::Service = client.post("services", &create_service).await?;
    println!("[s] Created service: {:?}", service_post);
    let service_get: model::Service = client.get(format!("services/{}", service_name)).await?;
    println!("[s] Retrieved service: {:?}", service_get);

    let services_get: Vec<model::Service> = client.get("services").await?;
    assert_eq!(1, services_get.len());

    println!();

    let users_get: Vec<model::User> = client
        .get(format!("services/{}/users", service_name))
        .await?;
    assert_eq!(0, users_get.len());

    println!("[s] [u] Creating a new user");
    let user_post: model::User = client
        .post(format!("services/{}/users", service_name), &create_user)
        .await?;
    println!("[s] [u] Created user: {:?}", user_post);
    let user_get: model::User = client
        .get(format!("services/{}/users/{}", service_name, user_name))
        .await?;
    println!("[s] [u] Retrieved user: {:?}", user_get);

    let users_get: Vec<model::User> = client
        .get(format!("services/{}/users", service_name))
        .await?;
    assert_eq!(1, users_get.len());

    println!();
    println!("[-] Requesting {}", service_url);

    let service_client = WebClient::new_permissive_tls(service_url, &user_name, &password)?;
    let response: String = service_client.get("").await?;

    println!("[-] Response: {:?}", response);
    println!();

    let stats_get: model::UserStats = client
        .get(format!(
            "services/{}/users/{}/stats",
            service_name, user_name
        ))
        .await?;
    println!("[s] [u] User stats: {:?}", stats_get);

    let ep_stats_get: model::UserEndpointStats = client
        .get(format!(
            "services/{}/users/{}/endpoints/stats",
            service_name, user_name
        ))
        .await?;
    println!("[s] [u] User endpoint stats: {:?}", ep_stats_get);

    println!("[s] [u] Removing the user");
    client
        .delete(format!("services/{}/users/{}", service_name, user_name))
        .await?;

    let users_get: Vec<model::User> = client
        .get(format!("services/{}/users", service_name))
        .await?;
    assert_eq!(0, users_get.len());

    println!("[s] Removing the service");
    client.delete(format!("services/{}", service_name)).await?;

    let services_get: Vec<model::Service> = client.get("services").await?;
    assert_eq!(0, services_get.len());

    Ok(())
}

#[cfg(feature = "tests-e2e")]
#[test]
fn test_e2e() -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .thread_name("service")
        .worker_threads(1)
        .build()
        .unwrap();

    let task_set = tokio::task::LocalSet::new();
    match task_set.block_on(&rt, e2e()) {
        Ok(v) => Ok(v),
        Err(e) => {
            eprintln!("Error: {}", e);
            Err(e)
        }
    }
}

mod service {
    use std::net::SocketAddr;

    use actix_web::{middleware, web, App, HttpResponse, HttpServer};
    use futures::channel::oneshot;

    async fn resource() -> Result<HttpResponse, actix_web::Error> {
        Ok(HttpResponse::Ok().json("OK"))
    }

    pub async fn spawn(address: String) -> anyhow::Result<SocketAddr> {
        let (tx, rx) = oneshot::channel();

        std::thread::spawn(move || {
            println!("[t] Starting target service ...");

            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .worker_threads(1)
                .build()
                .unwrap();

            println!("[t] Target service started");

            let task_set = tokio::task::LocalSet::new();
            task_set.block_on(&rt, async move {
                let server = HttpServer::new(move || {
                    App::new()
                        .wrap(middleware::Logger::default())
                        .service(web::resource("/resource").route(web::get().to(resource)))
                })
                .workers(1)
                .bind(address)
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;

                let address = server.addrs().into_iter().next().unwrap();
                let server = server.run();

                tx.send(address)
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                server.await.map_err(|e| anyhow::anyhow!(e.to_string()))?;

                println!("[t] Target service stopped");

                Ok::<_, anyhow::Error>(())
            })?;

            Ok::<_, anyhow::Error>(())
        });

        Ok(rx.await?)
    }
}
