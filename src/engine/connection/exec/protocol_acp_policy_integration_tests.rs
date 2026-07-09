//! Integration-style ACP policy-selection tests for protocol sessions.

use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll};

use bollard::container::LogOutput;
use futures_util::stream;
use tokio::io::AsyncWrite;

use super::super::{ProtocolProxyIo, ProtocolSessionOptions, run_protocol_session_with_io_async};
use crate::engine::connection::exec::session::CapabilityPolicy;
use crate::engine::connection::exec::{ExecMode, ExecRequest};
use crate::error::PodbotError;

#[derive(Clone, Default)]
struct RecordingOutput {
    bytes: Arc<Mutex<Vec<u8>>>,
}

impl RecordingOutput {
    fn snapshot(&self) -> Vec<u8> {
        self.bytes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl AsyncWrite for RecordingOutput {
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
        Poll::Ready(Ok(()))
    }
}

fn protocol_request() -> Result<ExecRequest, PodbotError> {
    ExecRequest::new(
        "policy-selection-sandbox",
        vec![String::from("codex"), String::from("app-server")],
        ExecMode::Protocol,
    )
}

fn blocked_request_frame(id: i64) -> Result<Vec<u8>, serde_json::Error> {
    let mut bytes = serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "terminal/create",
        "params": {},
    }))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn drive_policy_session(policy: CapabilityPolicy) -> io::Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    let runtime = tokio::runtime::Runtime::new()?;
    let initialize = super::initialize_frame("\n").map_err(io::Error::other)?;
    let host_stdin = runtime.block_on(super::build_host_stdin(&initialize))?;
    let host_stdout = RecordingOutput::default();
    let host_stdout_handle = host_stdout.clone();
    let host_stderr = RecordingOutput::default();
    let host_stderr_handle = host_stderr.clone();
    let container_input = super::RecordingInputWriter::new();
    let container_stdin = container_input.bytes.clone();
    let blocked = blocked_request_frame(7).map_err(io::Error::other)?;
    let output = stream::iter([Ok(LogOutput::StdOut {
        message: blocked.into(),
    })]);
    let request = protocol_request().map_err(io::Error::other)?;
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

    let recorded_container_stdin = container_stdin
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    Ok((
        recorded_container_stdin,
        host_stdout_handle.snapshot(),
        host_stderr_handle.snapshot(),
    ))
}

fn parse_json_line(bytes: &[u8]) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::from_slice(bytes.strip_suffix(b"\n").unwrap_or(bytes))
}

fn split_lines(bytes: &[u8]) -> Vec<&[u8]> {
    bytes
        .split_inclusive(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .collect()
}

fn line_matches(bytes: &[u8], expected: &[u8]) -> bool {
    bytes == expected
}

/// Pure query: returns `true` when `bytes` parses as the synthesized denial
/// response for the blocked request with id 7 and the given `method`.
fn synthesized_response_for_method(bytes: &[u8], method: &str) -> bool {
    let Ok(response) = parse_json_line(bytes) else {
        return false;
    };
    response.get("id") == Some(&serde_json::json!(7))
        && response
            .get("error")
            .and_then(|error| error.get("data"))
            .and_then(|data| data.get("method"))
            == Some(&serde_json::json!(method))
}

#[test]
fn mask_and_deny_masks_initialize_and_synthesizes_blocked_response() {
    let (container_stdin, host_stdout, _host_stderr) =
        drive_policy_session(CapabilityPolicy::MaskAndDeny).expect("policy session should run");
    let lines = split_lines(&container_stdin);
    let initialize = super::initialize_frame("\n").expect("initialize frame should serialize");
    let expected_initialize = super::mask_acp_initialize_frame(&initialize);

    assert!(
        host_stdout.is_empty(),
        "blocked outbound request must not reach host stdout",
    );
    assert!(
        lines
            .iter()
            .any(|line| line_matches(line, &expected_initialize)),
        "initialize should be masked before it reaches container stdin",
    );
    assert!(
        lines
            .iter()
            .any(|line| synthesized_response_for_method(line, "terminal/create")),
        "blocked request should produce a synthesized response",
    );
}

#[test]
fn disabled_policy_preserves_plain_streaming_proxy_behaviour() {
    let (container_stdin, host_stdout, _host_stderr) =
        drive_policy_session(CapabilityPolicy::Disabled).expect("policy session should run");
    let initialize = super::initialize_frame("\n").expect("initialize frame should serialize");
    let blocked = blocked_request_frame(7).expect("blocked request should serialize");

    assert_eq!(
        container_stdin, initialize,
        "Disabled should forward host stdin without ACP masking",
    );
    assert_eq!(
        host_stdout, blocked,
        "Disabled should not enforce the outbound denylist",
    );
}

#[test]
fn mask_only_masks_initialize_without_runtime_denylist_enforcement() {
    let (container_stdin, host_stdout, _host_stderr) =
        drive_policy_session(CapabilityPolicy::MaskOnly).expect("policy session should run");
    let initialize = super::initialize_frame("\n").expect("initialize frame should serialize");
    let blocked = blocked_request_frame(7).expect("blocked request should serialize");

    assert_eq!(
        container_stdin,
        super::mask_acp_initialize_frame(&initialize),
        "MaskOnly should apply the initialize masking path",
    );
    assert_eq!(
        host_stdout, blocked,
        "MaskOnly should not enforce the outbound denylist",
    );
}
