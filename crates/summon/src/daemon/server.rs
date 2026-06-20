//! Daemon server implementation.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use thiserror::Error;

use crate::controller;
use crate::daemon::protocol::{
    PROTOCOL_VERSION, Request, RequestEnvelope, Response, ResponseEnvelope, Status,
};
use crate::runner::{self, RunOutput};

/// Errors from serving the summon daemon.
#[derive(Debug, Error)]
pub enum ServerError {
    /// Parent directories or socket files could not be prepared.
    #[error("Could not prepare summon daemon socket at {path}: {reason}")]
    SocketSetup {
        /// Socket path.
        path: String,
        /// Underlying failure.
        reason: String,
    },

    /// The daemon is already running on the requested socket.
    #[error("Summon daemon is already running at {0}")]
    AlreadyRunning(String),

    /// The socket listener failed.
    #[error("Summon daemon socket error at {path}: {reason}")]
    SocketIo {
        /// Socket path.
        path: String,
        /// Underlying failure.
        reason: String,
    },
}

/// Runs the daemon server loop in the current process.
///
/// # Errors
///
/// Returns an error if the socket cannot be prepared or served.
pub fn serve(socket_path: &Path) -> Result<(), ServerError> {
    let listener = bind_listener(socket_path)?;
    let _guard = SocketGuard {
        path: socket_path.to_path_buf(),
    };
    let mut configs = ConfigCache::default();

    for stream in listener.incoming() {
        let mut stream = stream.map_err(|err| ServerError::SocketIo {
            path: socket_path.display().to_string(),
            reason: err.to_string(),
        })?;

        let should_stop = handle_stream(&mut stream, socket_path, &mut configs).map_err(|err| {
            ServerError::SocketIo {
                path: socket_path.display().to_string(),
                reason: err,
            }
        })?;

        if should_stop {
            break;
        }
    }

    Ok(())
}

fn bind_listener(socket_path: &Path) -> Result<UnixListener, ServerError> {
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent).map_err(|err| ServerError::SocketSetup {
            path: socket_path.display().to_string(),
            reason: err.to_string(),
        })?;
    }

    match UnixListener::bind(socket_path) {
        Ok(listener) => Ok(listener),
        Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
            if UnixStream::connect(socket_path).is_ok() {
                return Err(ServerError::AlreadyRunning(
                    socket_path.display().to_string(),
                ));
            }

            fs::remove_file(socket_path).map_err(|remove_err| ServerError::SocketSetup {
                path: socket_path.display().to_string(),
                reason: remove_err.to_string(),
            })?;

            UnixListener::bind(socket_path).map_err(|bind_err| ServerError::SocketSetup {
                path: socket_path.display().to_string(),
                reason: bind_err.to_string(),
            })
        }
        Err(err) => Err(ServerError::SocketSetup {
            path: socket_path.display().to_string(),
            reason: err.to_string(),
        }),
    }
}

fn handle_stream(
    stream: &mut UnixStream,
    socket_path: &Path,
    configs: &mut ConfigCache,
) -> Result<bool, String> {
    let request: RequestEnvelope = read_json(stream)?;
    if request.version != PROTOCOL_VERSION {
        return Err(format!(
            "protocol mismatch: expected v{}, got v{}",
            PROTOCOL_VERSION, request.version
        ));
    }

    let started = Instant::now();
    let summary = request_summary(&request.request);
    eprintln!("{} request start {}", log_timestamp(), summary);

    let (response, should_stop) = controller::with_autorelease_pool(|| match request.request {
        Request::Ping => (
            Response::Pong(Status {
                pid: std::process::id(),
                socket_path: socket_path.to_path_buf(),
                protocol_version: PROTOCOL_VERSION,
            }),
            false,
        ),
        Request::Stop => (Response::Stopped, true),
        Request::RunApp { app, verbose } => (Response::Run(runner::run_app(&app, verbose)), false),
        Request::RunBinding {
            name,
            config_path,
            verbose,
        } => {
            let output = configs.run_binding(&config_path, &name, verbose);
            (Response::Run(output), false)
        }
    });

    if let Response::Run(output) = &response {
        let stderr = output.stderr.trim();
        if stderr.is_empty() {
            eprintln!(
                "{} request end {} success={} elapsed_ms={}",
                log_timestamp(),
                summary,
                output.success,
                started.elapsed().as_millis()
            );
        } else {
            eprintln!(
                "{} request end {} success={} elapsed_ms={} stderr={}",
                log_timestamp(),
                summary,
                output.success,
                started.elapsed().as_millis(),
                stderr
            );
        }
    } else {
        eprintln!(
            "{} request end {} elapsed_ms={}",
            log_timestamp(),
            summary,
            started.elapsed().as_millis()
        );
    }

    write_json(stream, &ResponseEnvelope::new(response))?;
    Ok(should_stop)
}

fn request_summary(request: &Request) -> String {
    match request {
        Request::Ping => "ping".to_string(),
        Request::Stop => "stop".to_string(),
        Request::RunApp { app, verbose } => format!("run-app app={app} verbose={verbose}"),
        Request::RunBinding {
            name,
            config_path,
            verbose,
        } => format!(
            "run-binding name={name} config={} verbose={verbose}",
            config_path.display()
        ),
    }
}

fn log_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("ts={}s", now.as_secs())
}

#[derive(Default)]
struct ConfigCache {
    entries: HashMap<PathBuf, CachedConfig>,
}

impl ConfigCache {
    fn run_binding(&mut self, config_path: &Path, name: &str, verbose: u8) -> RunOutput {
        let entry = self.entries.entry(config_path.to_path_buf()).or_default();
        match entry.config(config_path) {
            Ok(config) => runner::run_binding_with_config(name, config_path, config, verbose),
            Err(stderr) => RunOutput {
                success: false,
                should_fallback_direct: false,
                stdout: String::new(),
                stderr,
            },
        }
    }
}

#[derive(Default)]
struct CachedConfig {
    modified_at: Option<SystemTime>,
    loaded_once: bool,
    state: CachedConfigState,
}

#[derive(Default)]
enum CachedConfigState {
    Loaded(crate::config::Config),
    #[default]
    Missing,
    Failed(String),
}

impl CachedConfig {
    fn config(&mut self, path: &Path) -> Result<&crate::config::Config, String> {
        self.refresh(path);
        match &self.state {
            CachedConfigState::Loaded(config) => Ok(config),
            CachedConfigState::Missing => Err(format!(
                "Config error in {}:\n  config file is not loaded\n",
                path.display()
            )),
            CachedConfigState::Failed(stderr) => Err(stderr.clone()),
        }
    }

    fn refresh(&mut self, path: &Path) {
        let modified_at = fs::metadata(path).and_then(|meta| meta.modified()).ok();
        if self.loaded_once && self.modified_at == modified_at {
            return;
        }

        self.loaded_once = true;
        self.modified_at = modified_at;
        self.state = match crate::config::load_from(path) {
            Ok(config) => CachedConfigState::Loaded(config),
            Err(err) => {
                CachedConfigState::Failed(format!("Config error in {}:\n  {err}\n", path.display()))
            }
        };
    }
}

struct SocketGuard {
    path: PathBuf,
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn read_json<T: serde::de::DeserializeOwned>(stream: &mut UnixStream) -> Result<T, String> {
    let mut buffer = Vec::new();
    stream
        .read_to_end(&mut buffer)
        .map_err(|err| err.to_string())?;
    serde_json::from_slice(&buffer).map_err(|err| err.to_string())
}

fn write_json<T: serde::Serialize>(stream: &mut UnixStream, value: &T) -> Result<(), String> {
    serde_json::to_writer(&mut *stream, value).map_err(|err| err.to_string())?;
    stream.write_all(b"\n").map_err(|err| err.to_string())
}
