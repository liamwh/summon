//! Optional summon daemon for a warm hot path.

pub mod client;
pub mod protocol;
pub mod server;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::daemon::client::ClientError;
use crate::daemon::protocol::{Request, RequestEnvelope, Status};
use crate::runner::{self, RunOutput};

const LAUNCH_AGENT_LABEL: &str = "dev.liamwh.summond";

/// How the CLI should use the daemon.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DaemonMode {
    /// Try the daemon first and fall back to direct mode.
    #[default]
    Auto,
    /// Skip the daemon and always run direct mode.
    Off,
    /// Require the daemon; do not fall back to direct mode.
    Required,
}

impl DaemonMode {
    /// Resolves the daemon mode from the `SUMMON_DAEMON` environment variable.
    #[must_use]
    pub fn from_env() -> Self {
        match std::env::var("SUMMON_DAEMON")
            .ok()
            .as_deref()
            .map(str::trim)
        {
            Some("off" | "false" | "0" | "direct") => Self::Off,
            Some("required" | "on" | "true" | "1") => Self::Required,
            _ => Self::Auto,
        }
    }
}

/// Errors from daemon control operations.
#[derive(Debug, Error)]
pub enum DaemonError {
    /// The current user home directory could not be resolved.
    #[error("HOME environment variable is not set")]
    NoHome,

    /// The daemon could not be started.
    #[error("Could not start summon daemon: {0}")]
    Start(String),

    /// The daemon could not be stopped.
    #[error("Could not stop summon daemon: {0}")]
    Stop(String),

    /// The daemon did not become ready in time.
    #[error("Summon daemon did not become ready at {0}")]
    StartTimeout(String),

    /// The launch agent plist could not be written.
    #[error("Could not write summon launch agent at {path}: {reason}")]
    LaunchAgentWrite {
        /// The plist path.
        path: String,
        /// The underlying failure.
        reason: String,
    },

    /// A launchctl invocation failed.
    #[error("launchctl {command} failed: {reason}")]
    LaunchCtl {
        /// The launchctl command that failed.
        command: String,
        /// stderr or stdout from launchctl.
        reason: String,
    },

    /// The daemon client failed.
    #[error(transparent)]
    Client(#[from] ClientError),

    /// The daemon server failed.
    #[error(transparent)]
    Server(#[from] server::ServerError),
}

/// Runs a binding through the daemon when configured, otherwise direct mode.
pub fn run_binding_or_direct(name: &str, config_path: &Path, verbose: u8) -> RunOutput {
    let request = RequestEnvelope::new(Request::RunBinding {
        name: name.to_string(),
        config_path: config_path.to_path_buf(),
        verbose,
    });

    match DaemonMode::from_env() {
        DaemonMode::Off => runner::run_binding_from_path(name, config_path, verbose),
        DaemonMode::Required => run_required(request),
        DaemonMode::Auto => run_auto(request, || {
            runner::run_binding_from_path(name, config_path, verbose)
        }),
    }
}

/// Runs an app request through the daemon when configured, otherwise direct mode.
pub fn run_app_or_direct(app: &str, verbose: u8) -> RunOutput {
    let request = RequestEnvelope::new(Request::RunApp {
        app: app.to_string(),
        verbose,
    });

    match DaemonMode::from_env() {
        DaemonMode::Off => runner::run_app(app, verbose),
        DaemonMode::Required => run_required(request),
        DaemonMode::Auto => run_auto(request, || runner::run_app(app, verbose)),
    }
}

/// Starts the daemon and waits until it is ready.
///
/// # Errors
///
/// Returns an error if the daemon could not be started or did not become ready.
pub fn start() -> Result<Status, DaemonError> {
    let socket_path = socket_path()?;

    if let Ok(status) = client::ping(&socket_path) {
        return Ok(status);
    }

    ensure_started()?;
    wait_until_ready(&socket_path, Duration::from_secs(2))
}

/// Returns the current daemon status.
pub fn status() -> Result<Status, DaemonError> {
    let socket_path = socket_path()?;
    client::ping(&socket_path).map_err(Into::into)
}

/// Stops the daemon if it is running.
pub fn stop() -> Result<(), DaemonError> {
    if should_use_launch_agent() {
        stop_launch_agent()?;
        return Ok(());
    }

    let socket_path = socket_path()?;
    client::stop(&socket_path).map_err(Into::into)
}

/// Runs the foreground daemon server loop.
pub fn run_server() -> Result<(), DaemonError> {
    let socket_path = socket_path()?;
    server::serve(&socket_path).map_err(Into::into)
}

/// Resolves the daemon socket path.
pub fn socket_path() -> Result<PathBuf, DaemonError> {
    if let Ok(path) = std::env::var("SUMMOND_SOCKET_PATH") {
        return Ok(PathBuf::from(path));
    }

    if let Ok(path) = std::env::var("XDG_CACHE_HOME") {
        return Ok(PathBuf::from(path).join("summon").join("summond.sock"));
    }

    let home = std::env::var("HOME").map_err(|_| DaemonError::NoHome)?;
    Ok(PathBuf::from(home)
        .join(".cache")
        .join("summon")
        .join("summond.sock"))
}

fn run_auto<F>(request: RequestEnvelope, fallback: F) -> RunOutput
where
    F: FnOnce() -> RunOutput,
{
    let socket_path = match socket_path() {
        Ok(path) => path,
        Err(_) => return fallback(),
    };

    match client::run(&socket_path, request) {
        Ok(output) => output,
        Err(ClientError::Unavailable { .. }) => {
            let _ = ensure_started();
            fallback()
        }
        Err(_) => fallback(),
    }
}

fn run_required(request: RequestEnvelope) -> RunOutput {
    let socket_path = match socket_path() {
        Ok(path) => path,
        Err(err) => return daemon_failure(err.to_string()),
    };

    match client::run(&socket_path, request) {
        Ok(output) => output,
        Err(err) => daemon_failure(err.to_string()),
    }
}

fn ensure_started() -> Result<(), DaemonError> {
    if should_use_launch_agent() {
        install_or_restart_launch_agent()
    } else {
        spawn_transient_process()
    }
}

fn should_use_launch_agent() -> bool {
    std::env::var_os("SUMMOND_SOCKET_PATH").is_none()
}

fn install_or_restart_launch_agent() -> Result<(), DaemonError> {
    let plist_path = launch_agent_path()?;
    let current_exe = std::env::current_exe().map_err(|err| DaemonError::Start(err.to_string()))?;

    if let Some(parent) = plist_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| DaemonError::LaunchAgentWrite {
            path: plist_path.display().to_string(),
            reason: err.to_string(),
        })?;
    }

    std::fs::write(&plist_path, launch_agent_plist(&current_exe)).map_err(|err| {
        DaemonError::LaunchAgentWrite {
            path: plist_path.display().to_string(),
            reason: err.to_string(),
        }
    })?;

    let domain = launchctl_domain();
    let plist = plist_path.as_os_str();
    let _ = launchctl(
        [OsStr::new("bootout"), OsStr::new(&domain), plist],
        "bootout",
        true,
    );
    launchctl(
        [OsStr::new("bootstrap"), OsStr::new(&domain), plist],
        "bootstrap",
        false,
    )?;
    launchctl(
        [
            OsStr::new("kickstart"),
            OsStr::new("-k"),
            OsStr::new(&format!("{domain}/{LAUNCH_AGENT_LABEL}")),
        ],
        "kickstart",
        false,
    )?;

    Ok(())
}

fn stop_launch_agent() -> Result<(), DaemonError> {
    let domain = launchctl_domain();
    let plist_path = launch_agent_path()?;
    let _ = launchctl(
        [
            OsStr::new("bootout"),
            OsStr::new(&domain),
            plist_path.as_os_str(),
        ],
        "bootout",
        true,
    );

    let socket_path = socket_path()?;
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match client::ping(&socket_path) {
            Err(ClientError::Unavailable { .. }) => {
                let _ = std::fs::remove_file(&socket_path);
                return Ok(());
            }
            Err(_) => return Ok(()),
            Ok(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            Ok(_) => {
                return Err(DaemonError::Stop(format!(
                    "daemon at {} did not stop after launchctl bootout",
                    socket_path.display()
                )));
            }
        }
    }
}

fn launch_agent_path() -> Result<PathBuf, DaemonError> {
    let home = std::env::var("HOME").map_err(|_| DaemonError::NoHome)?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{LAUNCH_AGENT_LABEL}.plist")))
}

fn launch_agent_plist(executable: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{exe}</string>
    <string>daemon</string>
    <string>run</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>ProcessType</key>
  <string>Interactive</string>
  <key>LimitLoadToSessionType</key>
  <array>
    <string>Aqua</string>
  </array>
</dict>
</plist>
"#,
        label = LAUNCH_AGENT_LABEL,
        exe = executable.display()
    )
}

fn launchctl_domain() -> String {
    format!("gui/{}", unsafe { libc::geteuid() })
}

fn launchctl<I, S>(args: I, command: &str, allow_failure: bool) -> Result<(), DaemonError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = std::process::Command::new("launchctl")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|err| DaemonError::LaunchCtl {
            command: command.to_string(),
            reason: err.to_string(),
        })?;

    if allow_failure || output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let reason = if !stderr.is_empty() { stderr } else { stdout };

    Err(DaemonError::LaunchCtl {
        command: command.to_string(),
        reason,
    })
}

fn spawn_transient_process() -> Result<(), DaemonError> {
    let socket_path = socket_path()?;
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| DaemonError::Start(err.to_string()))?;
    }

    let current_exe = std::env::current_exe().map_err(|err| DaemonError::Start(err.to_string()))?;
    std::process::Command::new(current_exe)
        .args(["daemon", "run"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|err| DaemonError::Start(err.to_string()))
}

fn wait_until_ready(socket_path: &Path, timeout: Duration) -> Result<Status, DaemonError> {
    let deadline = Instant::now() + timeout;
    loop {
        match client::ping(socket_path) {
            Ok(status) => return Ok(status),
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            Err(_) => {
                return Err(DaemonError::StartTimeout(socket_path.display().to_string()));
            }
        }
    }
}

fn daemon_failure(message: String) -> RunOutput {
    RunOutput {
        success: false,
        stdout: String::new(),
        stderr: format!("Daemon error: {message}\n"),
    }
}
