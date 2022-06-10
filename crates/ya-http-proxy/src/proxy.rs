use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use futures::channel::oneshot;
use futures::FutureExt;
use hyper::service::{make_service_fn, service_fn};
use sha3::{Digest, Sha3_256};
use tokio::sync::RwLock;
use tokio::task::LocalSet;

use crate::conf::ProxyConf;
use crate::error::{Error, ProxyError, ServiceError, UserError};
use crate::proxy::handler::forward_req;
use crate::proxy::stream::HttpStream;
use ya_http_proxy_model as model;
use ya_http_proxy_model::Addresses;

mod client;
mod handler;
mod server;
mod stream;

#[derive(Clone)]
pub struct ProxyManager {
    pub default_conf: Arc<ProxyConf>,
    pub(crate) proxies: Arc<RwLock<HashMap<Addresses, Proxy>>>,
}

impl ProxyManager {
    pub fn new(conf: ProxyConf) -> Self {
        Self {
            default_conf: Arc::new(conf),
            proxies: Default::default(),
        }
    }

    #[inline]
    pub async fn get_or_spawn(&self, create: &mut model::CreateService) -> Result<Proxy, Error> {
        let instances = self.proxies.write().await;
        let addrs = create.addresses();

        match instances.get(&addrs) {
            Some(proxy) => Ok(proxy.clone()),
            None => {
                drop(instances);
                self.spawn(create).await
            }
        }
    }

    async fn spawn(&self, create: &mut model::CreateService) -> Result<Proxy, Error> {
        log::info!("Proxy manager spawn");
        let mut services = self.proxies.write().await;
        let addrs = create.addresses();

        if services.contains_key(&addrs) {
            return Err(ProxyError::AlreadyRunning(addrs).into());
        }

        let conf = self.conf_update(create)?;
        let name = create.name.clone();
        let addrs = conf.server.addresses();
        let proxy_addrs = addrs.clone();
        let cpu_threads = create.cpu_threads;

        let (tx, rx) = oneshot::channel();
        std::thread::spawn(move || {
            let mut rt_builder = tokio::runtime::Builder::new_multi_thread();
            rt_builder.enable_all().thread_name(&name);

            if let Some(n) = cpu_threads {
                rt_builder.worker_threads(n);
            }
            let rt = match rt_builder.build() {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = tx.send(Err(e.into()));
                    return;
                }
            };

            let fut = async move {
                let mut proxy = Proxy::new(conf);
                let finished = proxy.start().await?;
                Ok((proxy, finished))
            }
            .then(|result| async move {
                match result {
                    Ok((proxy, finished)) => {
                        let _ = tx.send(Ok(proxy));

                        log::info!("Proxy '{}' is listening on {}", name, addrs);
                        match finished.await {
                            Ok(_) => log::info!("Proxy '{}' stopped [{}]", name, addrs),
                            Err(e) => log::error!("Proxy '{}' [{}] error: {}", name, addrs, e),
                        }
                    }
                    Err(err) => {
                        let _ = tx.send(Err(err));
                    }
                };
            });

            let task_set = LocalSet::new();
            task_set.block_on(&rt, fut);
        });

        match rx.await {
            Ok(result) => {
                if let Ok(ref proxy) = result {
                    services.insert(proxy_addrs, proxy.clone());
                }
                result
            }
            Err(_) => Err(ProxyError::rt("Proxy canceled").into()),
        }
    }

    fn conf_update(&self, create: &mut model::CreateService) -> Result<ProxyConf, ProxyError> {
        let mut conf = (*self.default_conf).clone();

        match create.bind_https {
            Some(ref addrs) => {
                conf.server.bind_https.replace(addrs.clone());
            }
            None => create.bind_https = conf.server.bind_https.clone(),
        }
        match create.bind_http {
            Some(ref addrs) => {
                conf.server.bind_http.replace(addrs.clone());
            }
            None => create.bind_http = conf.server.bind_http.clone(),
        }

        if create.server_name.is_empty() {
            if conf.server.server_name.is_empty() {
                return Err(ProxyError::Conf(
                    "Missing public address information".to_string(),
                ));
            }

            create.server_name = conf.server.server_name.clone();
        } else {
            conf.server.server_name = create.server_name.clone();
        }

        create.cpu_threads = create
            .cpu_threads
            .take()
            .or(conf.server.cpu_threads)
            .map(|n| 1.max(n));

        match create.cert {
            Some(ref mut cert) => {
                conf.server.server_cert.server_cert_store_path = Some(cert.path.clone());
                conf.server.server_cert.server_key_path = Some(cert.key_path.clone());
                cert.hash = cert_hash(&cert.path)?;
            }
            None => {
                let path = match conf.server.server_cert.server_cert_store_path {
                    Some(ref path) => path.clone(),
                    None => return Ok(conf),
                };
                let key_path = match conf.server.server_cert.server_key_path {
                    Some(ref path) => path.clone(),
                    None => return Ok(conf),
                };
                let hash = cert_hash(&path)?;

                create.cert = Some(model::CreateServiceCert {
                    hash,
                    path,
                    key_path,
                });
            }
        }

        Ok(conf)
    }

    pub(crate) fn proxies(&self) -> Arc<RwLock<HashMap<Addresses, Proxy>>> {
        self.proxies.clone()
    }

    pub(crate) async fn proxy(&self, service_name: &str) -> Result<Proxy, Error> {
        let proxies = self.proxies.read().await;
        for proxy in proxies.values() {
            if proxy.contains(service_name).await {
                return Ok(proxy.clone());
            }
        }
        Err(ServiceError::NotFound(service_name.to_string()).into())
    }

    pub(crate) async fn stop(&self) {
        let mut proxies = { std::mem::take(&mut *self.proxies.write().await) };
        proxies.values_mut().for_each(|p| p.stop());
        std::process::exit(0);
    }
}

/// Proxy instance
#[derive(Clone)]
pub struct Proxy {
    pub conf: Arc<ProxyConf>,
    pub(crate) state: Arc<RwLock<ProxyState>>,
    pub(crate) stats: Arc<RwLock<ProxyStats>>,
    stop_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
}

impl Proxy {
    pub fn new(conf: ProxyConf) -> Self {
        Self {
            conf: Arc::new(conf),
            state: Default::default(),
            stats: Default::default(),
            stop_tx: Default::default(),
        }
    }

    pub async fn start(
        &mut self,
    ) -> Result<impl Future<Output = hyper::Result<()>> + 'static, Error> {
        if self.conf.server.bind_https.is_none() && self.conf.server.bind_http.is_none() {
            return Err(ProxyError::Conf("No listening addresses specified".to_string()).into());
        }

        {
            let stop_tx = self.stop_tx.lock().unwrap();
            if stop_tx.is_some() {
                return Err(ProxyError::AlreadyRunning(self.conf.server.addresses()).into());
            }
        }

        let client = client::build(&self.conf.client);
        let (tx, rx) = oneshot::channel();
        let rx = rx.shared();

        let handler = || {
            let client = client.clone();
            let state = self.state.clone();
            let stats = self.stats.clone();

            move |stream: &HttpStream| {
                let client = client.clone();
                let state = state.clone();
                let stats = stats.clone();
                let address = stream.remote_addr();

                async move {
                    Ok::<_, Error>(service_fn(move |req| {
                        forward_req(req, state.clone(), stats.clone(), client.clone(), address)
                    }))
                }
            }
        };

        let rx_ = rx.clone();
        let https = server::listen_https(&self.conf.server)
            .await?
            .map(|builder| {
                builder
                    .serve(make_service_fn(handler()))
                    .with_graceful_shutdown(rx_.map(|_| ()))
                    .boxed()
            });

        let rx_ = rx;
        let http = server::listen_http(&self.conf.server)
            .await?
            .map(|builder| {
                builder
                    .serve(make_service_fn(handler()))
                    .with_graceful_shutdown(rx_.map(|_| ()))
                    .boxed()
            });

        {
            let mut stop_tx = self.stop_tx.lock().unwrap();
            stop_tx.replace(tx);
        }

        Ok(async move {
            match (http, https) {
                (Some(http), Some(https)) => {
                    futures::future::try_join(http, https).await?;
                    Ok(())
                }
                (http, https) => {
                    http.or(https)
                        .unwrap_or_else(|| futures::future::ok(()).boxed())
                        .await
                }
            }
        })
    }

    pub fn stop(&mut self) {
        std::mem::take(&mut *self.stop_tx.lock().unwrap())
            .into_iter()
            .for_each(|tx| {
                let _ = tx.send(());
            });
    }
}

impl Proxy {
    pub async fn contains(&self, service_name: &str) -> bool {
        let state = self.state.read().await;
        state.get_service(service_name).is_ok()
    }

    pub async fn get<S>(&self, service_name: &str) -> Result<S, Error>
    where
        S: From<(model::CreateService, DateTime<Utc>)> + 'static,
    {
        let state_lock = self.state.clone();
        let state = state_lock.read().await;
        let service = state.get_service(service_name)?;
        Ok(S::from((service.created_with.clone(), service.created_at)))
    }

    pub async fn add<S>(&self, mut create: model::CreateService) -> Result<S, Error>
    where
        S: From<(model::CreateService, DateTime<Utc>)>,
    {
        if create.from.trim().is_empty() {
            create.from = "/".to_string()
        }

        let mut state = self.state.write().await;
        let service = state.add_service(create)?;
        let model = S::from((service.created_with.clone(), service.created_at));
        let endpoint = service.created_with.from.clone();
        drop(state);

        let mut stats = self.stats.write().await;
        stats.reset_endpoint(&endpoint);
        Ok(model)
    }

    pub async fn remove(&self, service_name: &str) -> Result<(), Error> {
        let mut state = self.state.write().await;
        Ok(state.remove_service(service_name)?)
    }

    pub async fn get_users(&self, service_name: &str) -> Result<Vec<ProxyUser>, Error> {
        let state = self.state.read().await;
        let service = state.get_service(service_name)?;
        Ok(service.get_users())
    }

    pub async fn get_user(&self, service_name: &str, username: &str) -> Result<ProxyUser, Error> {
        let state = self.state.read().await;
        let service = state.get_service(service_name)?;
        Ok(service.get_user(username)?)
    }

    pub async fn add_user(
        &self,
        service_name: &str,
        username: impl ToString,
        password: impl ToString,
    ) -> Result<ProxyUser, Error> {
        let mut state = self.state.write().await;
        let service = state.get_service_mut(service_name)?;
        let user = service.add_user(username, password)?;
        drop(state);

        let mut stats = self.stats.write().await;
        stats.reset_user(&user.username);
        Ok(user)
    }

    pub async fn remove_user(&self, service_name: &str, username: &str) -> Result<(), Error> {
        let mut state = self.state.write().await;
        let service = state.get_service_mut(service_name)?;
        Ok(service.remove_user(username)?)
    }
}

/// Proxy service state
#[derive(Default)]
pub struct ProxyState {
    pub(crate) by_endpoint: HashMap<String, ProxyService>,
    pub(crate) by_name: HashMap<String, String>,
}

impl ProxyState {
    fn get_service(&self, service_name: &str) -> Result<&ProxyService, ServiceError> {
        self.by_name
            .get(service_name)
            .and_then(|s| self.by_endpoint.get(s))
            .ok_or_else(|| ServiceError::NotFound(service_name.to_string()))
    }

    fn get_service_mut(&mut self, service_name: &str) -> Result<&mut ProxyService, ServiceError> {
        self.by_name
            .get(service_name)
            .and_then(|s| self.by_endpoint.get_mut(s))
            .ok_or_else(|| ServiceError::NotFound(service_name.to_string()))
    }

    fn add_service(
        &mut self,
        create: model::CreateService,
    ) -> Result<&mut ProxyService, ServiceError> {
        let name = create.name.clone();
        let mut endpoint = create.from.trim().to_string();

        if !endpoint.starts_with('/') {
            endpoint = format!("/{}", endpoint);
        }

        if self.by_name.contains_key(&name) {
            return Err(ServiceError::AlreadyExists { name, endpoint });
        }

        for by_endpoint in self.by_endpoint.keys() {
            if by_endpoint.starts_with(&endpoint) || endpoint.starts_with(by_endpoint) {
                return Err(ServiceError::AlreadyExists { name, endpoint });
            }
        }

        let service = ProxyService::new(create);
        self.by_name.insert(name, endpoint.clone());
        self.by_endpoint.insert(endpoint.clone(), service);

        Ok(self.by_endpoint.get_mut(&endpoint).unwrap())
    }

    fn remove_service(&mut self, service_name: &str) -> Result<(), ServiceError> {
        match self.by_name.remove(service_name) {
            Some(endpoint) => {
                self.by_endpoint.remove(&endpoint);
                Ok(())
            }
            None => Err(ServiceError::NotFound(service_name.to_string())),
        }
    }
}

/// Proxy service instance
#[derive(Debug)]
pub struct ProxyService {
    pub created_at: DateTime<Utc>,
    pub created_with: model::CreateService,
    pub(crate) access: HashSet<String>,
    pub(crate) users: HashMap<String, ProxyUser>,
}

impl ProxyService {
    pub fn new(create: model::CreateService) -> Self {
        Self {
            created_at: Utc::now(),
            created_with: create,
            access: Default::default(),
            users: Default::default(),
        }
    }

    fn get_users(&self) -> Vec<ProxyUser> {
        self.users.values().cloned().collect()
    }

    fn get_user(&self, username: &str) -> Result<ProxyUser, UserError> {
        self.users
            .get(username)
            .cloned()
            .ok_or_else(|| UserError::NotFound(username.to_string()))
    }

    fn add_user(
        &mut self,
        username: impl ToString,
        password: impl ToString,
    ) -> Result<ProxyUser, UserError> {
        let username = username.to_string();
        let password = password.to_string();

        if self.users.contains_key(&username) {
            return Err(UserError::AlreadyExists(username));
        }

        let credentials = base64::encode(format!("{}:{}", username, password));
        let user = ProxyUser {
            created_at: Utc::now(),
            username: username.clone(),
            credentials: credentials.clone(),
        };

        self.access.insert(credentials);
        self.users.insert(username, user.clone());

        Ok(user)
    }

    fn remove_user(&mut self, username: &str) -> Result<(), UserError> {
        match self.users.remove(username) {
            Some(user) => {
                self.access.remove(&user.credentials);
                Ok(())
            }
            None => Err(UserError::NotFound(username.to_string())),
        }
    }
}

impl<'a> From<&'a ProxyService> for model::Service {
    fn from(s: &'a ProxyService) -> Self {
        model::Service {
            created_at: s.created_at,
            inner: s.created_with.clone(),
        }
    }
}

/// Proxy service user
#[derive(Clone, Debug)]
pub struct ProxyUser {
    pub created_at: DateTime<Utc>,
    pub username: String,
    credentials: String,
}

/// Proxy server stats
#[derive(Default)]
pub struct ProxyStats {
    pub(crate) total: usize,
    pub(crate) endpoint: HashMap<String, usize>,
    pub(crate) user: HashMap<String, usize>,
    pub(crate) user_endpoint: HashMap<String, HashMap<String, usize>>,
}

impl ProxyStats {
    pub fn reset_endpoint(&mut self, endpoint: &str) {
        self.endpoint.insert(endpoint.to_string(), 0);
    }

    pub fn reset_user(&mut self, username: &str) {
        let username = username.to_string();
        self.user.insert(username.clone(), 0);
        self.user_endpoint.insert(username, Default::default());
    }

    pub fn inc(&mut self, endpoint: &str, username: &str) {
        self.total += 1;

        // `HashMap::raw_entry_mut` is unstable;
        // use lookups before converting the key

        if let Some(counter) = self.endpoint.get_mut(endpoint) {
            *counter += 1;
        } else {
            self.endpoint.insert(endpoint.to_string(), 1);
        }

        if let Some(stats) = self.user.get_mut(username) {
            *stats += 1;
        } else {
            self.user.insert(username.to_string(), 1);
        }

        let user_stats = if let Some(stats) = self.user_endpoint.get_mut(username) {
            stats
        } else {
            self.user_endpoint.entry(username.to_string()).or_default()
        };

        if let Some(stats) = user_stats.get_mut(endpoint) {
            *stats += 1;
        } else {
            user_stats.insert(endpoint.to_string(), 1);
        };
    }
}

pub(crate) fn cert_hash(path: impl AsRef<Path>) -> Result<String, ProxyError> {
    match std::fs::read(&path) {
        Ok(vec) => {
            let mut digest = Sha3_256::default();
            digest.update(&vec);

            let digest_str = format!("{:x}", digest.finalize());
            let prefix = if digest_str.len() % 2 == 1 { "0" } else { "" };

            Ok(format!("sha3:{}{}", prefix, digest_str))
        }
        Err(err) => Err(ProxyError::Conf(format!(
            "Unable to read the certificate file '{}': {}",
            path.as_ref().display(),
            err
        ))),
    }
}
