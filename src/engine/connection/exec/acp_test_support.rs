//! Shared test doubles and frame builders for the ACP test modules.
//!
//! Consolidates the recording writer used to capture host or container
//! output in tests, together with the newline-terminated JSON-RPC frame
//! builder, so the individual ACP test modules do not duplicate them.

use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll};

use bollard::container::LogOutput;
use futures_util::stream;
use serde_json::Value;
use tokio::io::{AsyncWrite, AsyncWriteExt, DuplexStream};

use super::protocol::{
    ProtocolProxyIo, ProtocolSessionOptions, run_protocol_session_with_io_async,
};
use super::session::CapabilityPolicy;
use super::{ExecMode, ExecRequest};

/// Recording writer that captures every byte written to it and tracks
/// whether `poll_shutdown` was observed.
///
/// Clones share the same underlying buffers, so a test can clone the
/// writer before moving it into the code under test and query the clone
/// afterwards.
#[derive(Clone, Default)]
pub(super) struct RecordingWriter {
    bytes: Arc<Mutex<Vec<u8>>>,
    shutdown_called: Arc<Mutex<bool>>,
}

impl RecordingWriter {
    /// Create a fresh recorder with empty buffers.
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Return a copy of the bytes captured so far.
    pub(super) fn snapshot(&self) -> Vec<u8> {
        self.bytes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Return `true` when `poll_shutdown` has been called on any clone.
    pub(super) fn shutdown_observed(&self) -> bool {
        *self
            .shutdown_called
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl AsyncWrite for RecordingWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.bytes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        *self
            .shutdown_called
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = true;
        Poll::Ready(Ok(()))
    }
}

/// Build a serialized JSON-RPC 2.0 frame terminated by `line_ending`.
///
/// Pass `id = Some(…)` for requests and `id = None` for notifications.
pub(super) fn jsonrpc_frame(
    id: Option<&Value>,
    method: &str,
    line_ending: &[u8],
) -> Result<Vec<u8>, serde_json::Error> {
    let payload = id.map_or_else(
        || {
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": method,
                "params": {},
            })
        },
        |request_id| {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "method": method,
                "params": {},
            })
        },
    );
    let mut bytes = serde_json::to_vec(&payload)?;
    bytes.extend_from_slice(line_ending);
    Ok(bytes)
}

/// Build a host-stdin duplex stream pre-loaded with `bytes` and closed at EOF.
pub(super) async fn build_host_stdin(bytes: &[u8]) -> io::Result<DuplexStream> {
    let capacity = bytes.len().max(1);
    let (mut writer, reader) = tokio::io::duplex(capacity);
    writer.write_all(bytes).await?;
    drop(writer);
    Ok(reader)
}

/// Byte streams captured while driving a protocol session under a policy.
pub(super) struct CapturedSessionIo {
    pub(super) host_stdout: Vec<u8>,
    pub(super) container_stdin: Vec<u8>,
}

/// Drives one protocol session under `policy`, feeding `host_stdin_bytes` from
/// the host and a single `output_frame` chunk from the daemon.
///
/// Host stdout and container stdin are captured for assertions; host stderr is
/// wired to a discarding recorder because no scenario inspects it. Set-up and
/// session failures surface as `io::Error`; the helper never panics.
pub(super) fn run_policy_session(
    container_id: &str,
    policy: CapabilityPolicy,
    host_stdin_bytes: &[u8],
    output_frame: &[u8],
) -> io::Result<CapturedSessionIo> {
    let runtime = tokio::runtime::Runtime::new()?;
    let request = ExecRequest::new(
        container_id,
        vec![String::from("codex"), String::from("app-server")],
        ExecMode::Protocol,
    )
    .map_err(io::Error::other)?;
    let host_stdin = runtime.block_on(build_host_stdin(host_stdin_bytes))?;

    let host_stdout = RecordingWriter::new();
    let host_stdout_recorder = host_stdout.clone();
    let host_stderr = RecordingWriter::new();
    let container_input = RecordingWriter::new();
    let container_stdin_recorder = container_input.clone();

    let output = stream::iter([Ok(LogOutput::StdOut {
        message: output_frame.to_vec().into(),
    })]);
    let stdio = ProtocolProxyIo::new(host_stdin, host_stdout, host_stderr)
        .with_options(ProtocolSessionOptions::new().with_capability_policy(policy));

    runtime
        .block_on(run_protocol_session_with_io_async(
            &request,
            Box::pin(output),
            Box::pin(container_input),
            stdio,
        ))
        .map_err(io::Error::other)?;

    Ok(CapturedSessionIo {
        host_stdout: host_stdout_recorder.snapshot(),
        container_stdin: container_stdin_recorder.snapshot(),
    })
}
