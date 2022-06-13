use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use is_executable::IsExecutable;

use ya_http_proxy_client::api::ManagementApi;
use ya_http_proxy_client::Error;

use crate::lock::{with_lock_ext, LockFile};

const TIMEOUT: Duration = Duration::from_secs(3);
const SLEEP: Duration = Duration::from_millis(500);

pub async fn spawn(api: ManagementApi, data_dir: PathBuf) -> anyhow::Result<()> {
    let started = Instant::now();
    let lock_path = with_lock_ext("/tmp/proxy.lock");
    let mut lock = LockFile::new(&lock_path);
    let mut state = ProxyState::Unknown;

    loop {
        if Instant::now() - started >= TIMEOUT {
            anyhow::bail!("proxy timed out after {}s", TIMEOUT.as_secs_f32());
        }

        state = match std::mem::replace(&mut state, ProxyState::Poisoned) {
            ProxyState::Unknown => match api.get_services().await {
                Ok(_) => ProxyState::Running,
                Err(err) => match err {
                    Error::SendRequestError { .. } => lock
                        .is_locked()
                        .then(|| ProxyState::AwaitLock)
                        .unwrap_or(ProxyState::Lock),
                    err => anyhow::bail!(err),
                },
            },
            ProxyState::Lock => lock
                .lock()
                .is_ok()
                .then(|| ProxyState::Start)
                .unwrap_or(ProxyState::AwaitLock),
            ProxyState::AwaitLock => {
                if lock.is_locked() {
                    tokio::time::delay_for(SLEEP).await;
                    ProxyState::AwaitLock
                } else {
                    ProxyState::Unknown
                }
            }
            ProxyState::Start => {
                let exe_path = std::env::current_exe()?;
                let exe_dir = exe_path
                    .parent()
                    .ok_or_else(|| anyhow::anyhow!("unable to retrieve executable directory"))?;

                let path = if cfg!(windows) {
                    exe_dir.join("ya-http-proxy.exe")
                } else {
                    exe_dir.join("ya-http-proxy")
                };

                if !path.is_file() {
                    anyhow::bail!("unable to find proxy binary");
                } else if !path.is_executable() {
                    anyhow::bail!("unable to execute proxy");
                }

                let mut command = Command::new(path);
                command
                    .arg("--log-dir")
                    .arg(&data_dir.to_string_lossy().to_string())
                    .current_dir(std::env::current_dir()?)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());

                spawn_detached_command(command)?;
                ProxyState::AwaitStart
            }
            ProxyState::AwaitStart => match api.get_services().await {
                Ok(_) => ProxyState::Running,
                Err(err) => match err {
                    Error::SendRequestError { .. } => {
                        tokio::time::delay_for(SLEEP).await;
                        ProxyState::AwaitStart
                    }
                    err => anyhow::bail!(err),
                },
            },
            ProxyState::Running => break,
            ProxyState::Poisoned => panic!("programming error"),
        };
    }

    Ok(())
}

fn spawn_detached_command(mut command: Command) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;

        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
        let child = command.spawn()?;
    }
    #[cfg(unix)]
    {
        use nix::sys::wait::waitpid;
        use nix::unistd::{fork, setsid, ForkResult};
        use std::process::exit;

        match unsafe { fork().expect("failed to fork the process") } {
            ForkResult::Parent { child } => {
                let _ = waitpid(Some(child), None);
            }
            ForkResult::Child => {
                if setsid().is_err() {
                    exit(166);
                }
                let result = command.spawn();
                exit(if result.is_ok() { 0 } else { 167 });
            }
        }
    }
    Ok(())
}

enum ProxyState {
    Unknown,
    Lock,
    AwaitLock,
    Start,
    AwaitStart,
    Running,
    Poisoned,
}
