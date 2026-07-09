//! Integration-style ACP policy-selection tests for protocol sessions.

use std::io;

use bollard::container::LogOutput;
use futures_util::stream;
use rstest::rstest;

use super::super::{ProtocolProxyIo, ProtocolSessionOptions, run_protocol_session_with_io_async};
use crate::engine::connection::exec::acp_test_support::{RecordingWriter, jsonrpc_frame};
use crate::engine::connection::exec::session::CapabilityPolicy;
use crate::engine::connection::exec::{ExecMode, ExecRequest};
use crate::error::PodbotError;

fn protocol_request() -> Result<ExecRequest, PodbotError> {
    ExecRequest::new(
        "policy-selection-sandbox",
        vec![String::from("codex"), String::from("app-server")],
        ExecMode::Protocol,
    )
}

fn blocked_request_frame(id: i64) -> Result<Vec<u8>, serde_json::Error> {
    jsonrpc_frame(Some(&serde_json::json!(id)), "terminal/create", b"\n")
}

fn drive_policy_session(policy: CapabilityPolicy) -> io::Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    let runtime = tokio::runtime::Runtime::new()?;
    let initialize = super::initialize_frame("\n").map_err(io::Error::other)?;
    let host_stdin = runtime.block_on(super::build_host_stdin(&initialize))?;
    let host_stdout = RecordingWriter::default();
    let host_stdout_handle = host_stdout.clone();
    let host_stderr = RecordingWriter::default();
    let host_stderr_handle = host_stderr.clone();
    let container_input = RecordingWriter::new();
    let container_recorder = container_input.clone();
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

    Ok((
        container_recorder.snapshot(),
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

#[rstest]
#[case::disabled(CapabilityPolicy::Disabled, false)]
#[case::mask_only(CapabilityPolicy::MaskOnly, true)]
fn non_enforcing_policies_stream_blocked_frames_to_host_stdout(
    #[case] policy: CapabilityPolicy,
    #[case] masks_initialize: bool,
) {
    let (container_stdin, host_stdout, _host_stderr) =
        drive_policy_session(policy).expect("policy session should run");
    let initialize = super::initialize_frame("\n").expect("initialize frame should serialize");
    let blocked = blocked_request_frame(7).expect("blocked request should serialize");
    let expected_stdin = if masks_initialize {
        super::mask_acp_initialize_frame(&initialize)
    } else {
        initialize
    };

    assert_eq!(
        container_stdin, expected_stdin,
        "{policy:?} should forward host stdin with the expected masking",
    );
    assert_eq!(
        host_stdout, blocked,
        "{policy:?} should not enforce the outbound denylist",
    );
}
