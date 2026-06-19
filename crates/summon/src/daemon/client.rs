//! Daemon client for socket-based summon requests.

use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use serde::de::DeserializeOwned;
use thiserror::Error;

use crate::daemon::protocol::{
    PROTOCOL_VERSION, RequestEnvelope, Response, ResponseEnvelope, Status,
};
use crate::runner::RunOutput;

/// Errors from daemon client operations.
#[derive(Debug, Error)]
pub enum ClientError {
    /// The daemon socket could not be reached.
    #[error("Summon daemon is unavailable at {socket}: {reason}")]
    Unavailable {
        /// Socket path that failed.
        socket: String,
        /// Connection failure.
        reason: String,
    },

    /// The daemon returned an unexpected protocol version.
    #[error("Summon daemon protocol mismatch at {socket}: expected v{expected}, got v{actual}")]
    ProtocolMismatch {
        /// Socket path that replied.
        socket: String,
        /// Expected version.
        expected: u32,
        /// Actual version.
        actual: u32,
    },

    /// The daemon response could not be parsed.
    #[error("Could not decode summon daemon response from {socket}: {reason}")]
    Decode {
        /// Socket path that replied.
        socket: String,
        /// Decode error.
        reason: String,
    },

    /// The daemon returned an unexpected response type.
    #[error("Unexpected summon daemon response from {socket}")]
    UnexpectedResponse {
        /// Socket path that replied.
        socket: String,
    },
}

/// Sends a daemon request and returns the raw response.
pub fn send(socket_path: &Path, request: RequestEnvelope) -> Result<ResponseEnvelope, ClientError> {
    let socket_label = socket_path.display().to_string();
    let mut stream = UnixStream::connect(socket_path).map_err(|err| ClientError::Unavailable {
        socket: socket_label.clone(),
        reason: err.to_string(),
    })?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));

    write_json(&mut stream, &request).map_err(|err| ClientError::Unavailable {
        socket: socket_label.clone(),
        reason: err.to_string(),
    })?;
    let _ = stream.shutdown(Shutdown::Write);

    let response: ResponseEnvelope = read_json(&mut stream).map_err(|err| ClientError::Decode {
        socket: socket_label.clone(),
        reason: err,
    })?;

    if response.version != PROTOCOL_VERSION {
        return Err(ClientError::ProtocolMismatch {
            socket: socket_label,
            expected: PROTOCOL_VERSION,
            actual: response.version,
        });
    }

    Ok(response)
}

/// Sends a daemon request that should return a run result.
pub fn run(socket_path: &Path, request: RequestEnvelope) -> Result<RunOutput, ClientError> {
    let response = send(socket_path, request)?;
    match response.response {
        Response::Run(output) => Ok(output),
        _ => Err(ClientError::UnexpectedResponse {
            socket: socket_path.display().to_string(),
        }),
    }
}

/// Pings the daemon and returns its status.
pub fn ping(socket_path: &Path) -> Result<Status, ClientError> {
    let response = send(
        socket_path,
        RequestEnvelope::new(crate::daemon::protocol::Request::Ping),
    )?;
    match response.response {
        Response::Pong(status) => Ok(status),
        _ => Err(ClientError::UnexpectedResponse {
            socket: socket_path.display().to_string(),
        }),
    }
}

/// Requests a graceful daemon shutdown.
pub fn stop(socket_path: &Path) -> Result<(), ClientError> {
    let response = send(
        socket_path,
        RequestEnvelope::new(crate::daemon::protocol::Request::Stop),
    )?;
    match response.response {
        Response::Stopped => Ok(()),
        _ => Err(ClientError::UnexpectedResponse {
            socket: socket_path.display().to_string(),
        }),
    }
}

fn write_json<T: serde::Serialize>(stream: &mut UnixStream, value: &T) -> std::io::Result<()> {
    serde_json::to_writer(&mut *stream, value)?;
    stream.write_all(b"\n")
}

fn read_json<T: DeserializeOwned>(stream: &mut UnixStream) -> Result<T, String> {
    let mut buffer = Vec::new();
    stream
        .read_to_end(&mut buffer)
        .map_err(|err| err.to_string())?;
    serde_json::from_slice(&buffer).map_err(|err| err.to_string())
}
