//! Integration-style ACP policy-selection tests for protocol sessions.

use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
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
        self.bytes.lock().expect("recording mutex").clone()
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
            .expect("recording mutex")
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

fn blocked_request_frame(id: i64) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "terminal/create",
        "params": {},
    }))
    .expect("blocked request should serialize");
    bytes.push(b'\n');
    bytes
}

fn drive_policy_session(policy: CapabilityPolicy) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let runtime = tokio::runtime::Runtime::new().expect("runtime should build");
    let initialize = super::initialize_frame("\n");
    let host_stdin = runtime
        .block_on(super::build_host_stdin(&initialize))
        .expect("host stdin should build");
    let host_stdout = RecordingOutput::default();
    let host_stdout_handle = host_stdout.clone();
    let host_stderr = RecordingOutput::default();
    let host_stderr_handle = host_stderr.clone();
    let container_input = super::RecordingInputWriter::new();
    let container_stdin = container_input.bytes.clone();
    let output = stream::iter([Ok(LogOutput::StdOut {
        message: blocked_request_frame(7).into(),
    })]);
    let stdio = ProtocolProxyIo::new(host_stdin, host_stdout, host_stderr)
        .with_options(ProtocolSessionOptions::new().with_capability_policy(policy));

    runtime
        .block_on(run_protocol_session_with_io_async(
            &protocol_request().expect("protocol request should build"),
            Box::pin(output),
            Box::pin(container_input),
            stdio,
        ))
        .expect("policy session should complete");

    (
        container_stdin.lock().expect("stdin mutex").clone(),
        host_stdout_handle.snapshot(),
        host_stderr_handle.snapshot(),
    )
}

fn parse_json_line(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes.strip_suffix(b"\n").unwrap_or(bytes))
        .expect("line should contain JSON")
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

fn synthesized_response_for_method(bytes: &[u8], method: &str) -> bool {
    let response = parse_json_line(bytes);
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
        drive_policy_session(CapabilityPolicy::MaskAndDeny);
    let lines = split_lines(&container_stdin);
    let expected_initialize = super::mask_acp_initialize_frame(&super::initialize_frame("\n"));

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
        drive_policy_session(CapabilityPolicy::Disabled);
    let blocked = blocked_request_frame(7);

    assert_eq!(
        container_stdin,
        super::initialize_frame("\n"),
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
        drive_policy_session(CapabilityPolicy::MaskOnly);
    let blocked = blocked_request_frame(7);

    assert_eq!(
        container_stdin,
        super::mask_acp_initialize_frame(&super::initialize_frame("\n")),
        "MaskOnly should apply the initialize masking path",
    );
    assert_eq!(
        host_stdout, blocked,
        "MaskOnly should not enforce the outbound denylist",
    );
}
