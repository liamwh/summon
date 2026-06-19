//! Versioned protocol for summon daemon requests.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::runner::RunOutput;

/// Protocol version for daemon requests and responses.
pub const PROTOCOL_VERSION: u32 = 1;

/// Request envelope sent from the CLI to the daemon.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct RequestEnvelope {
    /// Protocol version.
    pub version: u32,
    /// Requested operation.
    pub request: Request,
}

impl RequestEnvelope {
    /// Wraps a request in the current protocol version.
    pub fn new(request: Request) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            request,
        }
    }
}

/// Supported daemon requests.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum Request {
    /// Health check for the daemon.
    Ping,
    /// Gracefully stop the daemon.
    Stop,
    /// Run a binding from a specific config file path.
    RunBinding {
        /// Binding name to execute.
        name: String,
        /// Client-resolved config path.
        config_path: PathBuf,
        /// Verbosity level from the CLI.
        verbose: u8,
    },
    /// Run a direct app target.
    RunApp {
        /// App target to execute.
        app: String,
        /// Verbosity level from the CLI.
        verbose: u8,
    },
}

/// Response envelope sent from the daemon to the CLI.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ResponseEnvelope {
    /// Protocol version.
    pub version: u32,
    /// Response payload.
    pub response: Response,
}

impl ResponseEnvelope {
    /// Wraps a response in the current protocol version.
    pub fn new(response: Response) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            response,
        }
    }
}

/// Supported daemon responses.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum Response {
    /// Daemon status response.
    Pong(Status),
    /// Result of running a command.
    Run(RunOutput),
    /// Acknowledgement that the daemon is shutting down.
    Stopped,
}

/// Lightweight daemon status.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Status {
    /// Server PID.
    pub pid: u32,
    /// Active socket path.
    pub socket_path: PathBuf,
    /// Protocol version.
    pub protocol_version: u32,
}
