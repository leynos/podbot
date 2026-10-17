//! Shared helpers for exec request validation and Bollard option building.

use std::future::Future;
use std::io;
use std::pin::Pin;

use bollard::exec::{CreateExecOptions, StartExecOptions};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::task::JoinHandle;

use super::{ExecRequest, exec_failed};
use crate::error::{ConfigError, PodbotError};

/// Translate an exec request into daemon create options without changing argv.
/// For example, protocol mode attaches all streams while leaving TTY disabled.
pub(super) fn build_create_exec_options(request: &ExecRequest) -> CreateExecOptions<String> {
    let attached = request.mode().is_attached();
    CreateExecOptions::<String> {
        attach_stdin: Some(attached),
        attach_stdout: Some(attached),
        attach_stderr: Some(attached),
        tty: Some(attached && request.tty()),
        env: request.env().map(<[String]>::to_vec),
        cmd: Some(request.command().to_vec()),
        ..CreateExecOptions::default()
    }
}

/// Choose attachment and output buffering for the requested exec mode.
/// For example, protocol mode uses the bounded protocol output capacity.
pub(super) const fn build_start_exec_options(request: &ExecRequest) -> StartExecOptions {
    let output_capacity = match request.mode() {
        super::ExecMode::Protocol => Some(super::PROTOCOL_OUTPUT_CAPACITY),
        super::ExecMode::Attached | super::ExecMode::Detached => None,
    };

    StartExecOptions {
        detach: !request.mode().is_attached(),
        tty: request.mode().is_attached() && request.tty(),
        output_capacity,
    }
}

/// Reject missing argv or a blank executable while preserving valid arguments.
/// For example, `[String::from("git"), String::from("status")]` is returned
/// unchanged.
pub(super) fn validate_command(command: Vec<String>) -> Result<Vec<String>, PodbotError> {
    if command.is_empty() {
        return Err(PodbotError::from(ConfigError::MissingRequired {
            field: String::from("command"),
        }));
    }

    let executable = command.first().map(String::as_str).unwrap_or_default();
    if executable.trim().is_empty() {
        return Err(PodbotError::from(ConfigError::InvalidValue {
            field: String::from("command"),
            reason: String::from("command executable must not be empty"),
        }));
    }

    Ok(command)
}

/// Trim a required value and identify its configuration field on failure.
/// For example, `validate_required_field("container", " c ")` yields `Ok("c")`.
pub(super) fn validate_required_field<'a>(
    field: &str,
    value: &'a str,
) -> Result<&'a str, PodbotError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(PodbotError::from(ConfigError::MissingRequired {
            field: String::from(field),
        }));
    }

    Ok(trimmed)
}

/// Attach container context to a daemon create-exec failure.
/// For example, an error for `"c"` names that container and the create
/// operation.
pub(super) fn map_create_exec_error(
    container_id: &str,
    error: impl std::fmt::Display,
) -> PodbotError {
    exec_failed(container_id, format!("create exec failed: {error}"))
}

/// Attach container context to a daemon start-exec failure.
/// For example, an error for `"c"` names that container and the start
/// operation.
pub(super) fn map_start_exec_error(
    container_id: &str,
    error: impl std::fmt::Display,
) -> PodbotError {
    exec_failed(container_id, format!("start exec failed: {error}"))
}

/// Give the forwarding future ownership of both stdin and the exec writer.
/// For example, the returned task handle lets the session coordinator stop
/// input.
pub(super) fn spawn_stdin_forwarding_task<HostStdin, Forward, ForwardFuture>(
    host_stdin: HostStdin,
    input: Pin<Box<dyn AsyncWrite + Send>>,
    forward: Forward,
) -> JoinHandle<io::Result<()>>
where
    HostStdin: AsyncRead + Send + Unpin + 'static,
    Forward: FnOnce(HostStdin, Pin<Box<dyn AsyncWrite + Send>>) -> ForwardFuture + Send + 'static,
    ForwardFuture: Future<Output = io::Result<()>> + Send + 'static,
{
    tokio::spawn(async move { forward(host_stdin, input).await })
}

#[cfg(test)]
#[path = "helpers_tests.rs"]
mod tests;
