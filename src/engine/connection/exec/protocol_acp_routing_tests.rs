//! Verifies that `CapabilityPolicy` selects raw forwarding or runtime
//! enforcement for outbound ACP frames.

use std::io;

use bollard::container::LogOutput;
use futures_util::stream;
use rstest::rstest;

use super::{RecordingInputWriter, build_host_stdin};
use crate::engine::connection::exec::acp_test_support::jsonrpc_frame;
use crate::engine::connection::exec::protocol::{
    ProtocolProxyIo, ProtocolSessionOptions, run_protocol_session_with_io_async,
};
use crate::engine::connection::exec::session::CapabilityPolicy;
use crate::engine::connection::exec::{ExecMode, ExecRequest};
use crate::error::PodbotError;

fn protocol_request() -> Result<ExecRequest, PodbotError> {
    ExecRequest::new(
        "capability-policy-routing",
        vec![String::from("codex"), String::from("app-server")],
        ExecMode::Protocol,
    )
}

fn blocked_terminal_create_frame() -> Result<Vec<u8>, serde_json::Error> {
    jsonrpc_frame(Some(&serde_json::json!(7)), "terminal/create", b"\n")
}

fn run_policy_output_frame(
    policy: CapabilityPolicy,
    frame: &[u8],
) -> io::Result<(Vec<u8>, Vec<u8>)> {
    let runtime = tokio::runtime::Runtime::new()?;
    let request = protocol_request().map_err(io::Error::other)?;
    let host_stdin = runtime.block_on(build_host_stdin(&[]))?;
    let host_stdout = RecordingInputWriter::new();
    let host_stdout_recorder = host_stdout.clone();
    let host_stderr = RecordingInputWriter::new();
    let container_input = RecordingInputWriter::new();
    let container_stdin_recorder = container_input.clone();
    let output = stream::iter([Ok(LogOutput::StdOut {
        message: frame.to_vec().into(),
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

    Ok((
        host_stdout_recorder.snapshot(),
        container_stdin_recorder.snapshot(),
    ))
}

/// Pure query: returns `true` when `bytes` parses as the synthesized denial
/// response for the blocked `terminal/create` request with id 7.
fn synthesized_response_for_terminal_create(bytes: &[u8]) -> bool {
    let Ok(response) =
        serde_json::from_slice::<serde_json::Value>(bytes.strip_suffix(b"\n").unwrap_or(bytes))
    else {
        return false;
    };
    response.get("id") == Some(&serde_json::json!(7))
        && response
            .get("error")
            .and_then(|error| error.get("data"))
            .and_then(|data| data.get("method"))
            == Some(&serde_json::json!("terminal/create"))
}

#[rstest]
#[case::mask_and_deny_routes_through_enforcement_path(CapabilityPolicy::MaskAndDeny, false, true)]
#[case::disabled_policy_forwards_all_frames_raw(CapabilityPolicy::Disabled, true, false)]
#[case::mask_only_policy_forwards_blocked_frames_raw(CapabilityPolicy::MaskOnly, true, false)]
fn routes_output_frame_for_capability_policy(
    #[case] policy: CapabilityPolicy,
    #[case] expect_forward_raw: bool,
    #[case] expect_synthesized_response: bool,
) {
    let frame = blocked_terminal_create_frame().expect("blocked request should serialize");
    let (host_stdout, container_stdin) =
        run_policy_output_frame(policy, &frame).expect("policy session should complete");

    if expect_forward_raw {
        assert_eq!(
            host_stdout, frame,
            "{policy:?} should preserve the byte-transparent output path",
        );
    } else {
        assert_ne!(
            host_stdout, frame,
            "{policy:?} must not forward blocked frames verbatim",
        );
    }
    if expect_synthesized_response {
        assert!(
            synthesized_response_for_terminal_create(&container_stdin),
            "{policy:?} should write a synthesized denial response to container stdin",
        );
    } else {
        assert!(
            container_stdin.is_empty(),
            "{policy:?} should not write a synthesized denial response",
        );
    }
}
