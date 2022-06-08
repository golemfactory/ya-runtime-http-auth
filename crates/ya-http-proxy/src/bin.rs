use chrono::{DateTime, Local};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::{env, fs, io};

use flexi_logger::*;
use futures::future::{select, Either};
use structopt::StructOpt;
use tokio::runtime;
use tokio::signal::ctrl_c;
use tokio::task;

use ya_http_proxy::{Management, ProxyConf, ProxyManager};

#[derive(StructOpt, Debug)]
struct Cli {
    /// Path to a custom configuration file
    #[structopt(long, short)]
    pub config: Option<PathBuf>,
    /// Path to write logs to
    #[structopt(long, short)]
    pub log_dir: Option<PathBuf>,
    /// Management API address
    #[structopt(long, short, default_value = "127.0.0.1:6668")]
    pub management_addr: SocketAddr,
    /// Default proxy address
    #[structopt(long, short)]
    pub default_addr: Option<SocketAddr>,
    /// Default proxy certificate path
    #[structopt(long)]
    pub default_cert: Option<PathBuf>,
    /// Default proxy certificate key path
    #[structopt(long)]
    pub default_key: Option<PathBuf>,
}

impl Cli {
    fn update_conf(&self, conf: &mut ProxyConf) {
        if let Some(addr) = self.default_addr {
            conf.server.bind_https = Some(addr.into());
        }
        if let Some(ref path) = self.default_cert {
            conf.server.server_cert.server_cert_store_path = Some(path.clone());
        }
        if let Some(ref path) = self.default_key {
            conf.server.server_cert.server_key_path = Some(path.clone());
        }
    }
}

async fn run(addr: SocketAddr, conf: ProxyConf) -> anyhow::Result<()> {
    let mut server = Management::new(ProxyManager::new(conf));

    server.bind(addr)?;
    log::info!("Management API server is listening on {}", addr);

    let ctrl_c = ctrl_c();
    futures::pin_mut!(ctrl_c);
    futures::pin_mut!(server);

    match select(ctrl_c, server).await {
        Either::Left(_) => log::info!("C-c received, terminating ..."),
        Either::Right(_) => log::info!("Management API server has terminated"),
    }

    log::info!("Server stopped");
    Ok(())
}

fn setup_logging(log_dir: Option<impl AsRef<Path>>) -> anyhow::Result<()> {
    let log_level = env::var("PROXY_LOG").unwrap_or_else(|_| "info".into());
    env::set_var("PROXY_LOG", &log_level);

    let mut logger = Logger::try_with_str(&log_level)?;

    if let Some(log_dir) = log_dir {
        let log_dir = log_dir.as_ref();

        match fs::create_dir_all(log_dir) {
            Ok(_) => (),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
            Err(e) => anyhow::bail!(format!("invalid log path: {}", e)),
        }

        logger = logger
            .log_to_file(FileSpec::default().directory(log_dir))
            .duplicate_to_stderr(Duplicate::All)
            .rotate(
                Criterion::Size(2 * 1024 * 1024),
                Naming::Timestamps,
                Cleanup::KeepLogFiles(7),
            )
    }

    logger
        .format_for_stderr(log_format)
        .format_for_files(log_format)
        .print_message()
        .start()?;

    Ok(())
}

fn log_format(
    w: &mut dyn std::io::Write,
    now: &mut DeferredNow,
    record: &Record,
) -> Result<(), std::io::Error> {
    use std::time::{Duration, UNIX_EPOCH};
    const DATE_FORMAT_STR: &str = "%Y-%m-%d %H:%M:%S%.3f %z";

    let timestamp = now.now().unix_timestamp_nanos() as u64;
    let date = UNIX_EPOCH + Duration::from_nanos(timestamp);
    let local_date = DateTime::<Local>::from(date);

    write!(
        w,
        "[{} {:5} {}] {}",
        local_date.format(DATE_FORMAT_STR),
        record.level(),
        record.module_path().unwrap_or("<unnamed>"),
        record.args()
    )
}

fn main() -> anyhow::Result<()> {
    let _ = dotenv::dotenv();
    let cli: Cli = Cli::from_args();

    setup_logging(cli.log_dir.as_ref())?;

    let mut conf = match cli.config {
        Some(ref path) => ProxyConf::from_path(path)?,
        None => ProxyConf::from_env()?,
    };

    cli.update_conf(&mut conf);

    if !cli.management_addr.ip().is_loopback() {
        log::warn!("!!! Management API server will NOT be bound to a loopback address !!!");
        log::warn!("This is a dangerous action and should be taken with care");
    }

    let rt = runtime::Builder::new_current_thread()
        .enable_all()
        .thread_name("main")
        .worker_threads(1)
        .on_thread_start(|| {
            log::debug!("main thread started");
        })
        .on_thread_stop(|| {
            log::debug!("main thread stopped");
        })
        .build()?;

    let task_set = task::LocalSet::new();
    task_set.block_on(&rt, run(cli.management_addr, conf))?;

    Ok(())
}
