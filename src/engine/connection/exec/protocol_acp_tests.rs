//! ACP capability masking tests for the protocol stdin proxy.
//!
//! This module hosts the shared test harness (recording writers, frame
//! builders, and the synchronous forwarding runner) and delegates the
//! individual test groups to sibling submodules.

use std::io;

use tokio::io::{AsyncWriteExt, DuplexStream};

use super::*;
use crate::engine::connection::exec::acp_helpers::{
    ACP_FILE_SYSTEM_CAPABILITY, ACP_TERMINAL_CAPABILITY, MAX_FIRST_FRAME_BYTES,
    forward_initial_acp_frame_async, mask_acp_initialize_frame, split_frame_line_ending,
};
use crate::engine::connection::exec::acp_test_support::RecordingWriter as RecordingInputWriter;

fn initialize_frame_with_capabilities(
    capabilities: &serde_json::Value,
    line_ending: &str,
) -> Result<Vec<u8>, serde_json::Error> {
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientCapabilities": capabilities,
            "clientInfo": {
                "name": "podbot-tests",
                "version": "1.0.0"
            }
        }
    });
    let mut frame = serde_json::to_vec(&payload)?;
    frame.extend_from_slice(line_ending.as_bytes());
    Ok(frame)
}

/// Builds the blocked `fs`/`terminal` capability object, optionally with an
/// unrelated `_meta` entry that masking must preserve.
fn blocked_capabilities(include_meta: bool) -> serde_json::Value {
    let mut capabilities = serde_json::json!({
        "fs": { "readTextFile": true, "writeTextFile": true },
        "terminal": true
    });
    if include_meta && let Some(object) = capabilities.as_object_mut() {
        object.insert(String::from("_meta"), serde_json::json!({ "custom": true }));
    }
    capabilities
}

fn initialize_frame(line_ending: &str) -> Result<Vec<u8>, serde_json::Error> {
    initialize_frame_with_capabilities(&blocked_capabilities(true), line_ending)
}

/// Builds a serialised ACP `initialize` frame whose `clientCapabilities`
/// contains only `_meta` (no blocked entries), terminated with `\n`.
pub(super) fn initialize_without_blocked_capabilities() -> Result<Vec<u8>, serde_json::Error> {
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientCapabilities": {
                "_meta": {
                    "custom": true
                }
            }
        }
    });

    let mut frame = serde_json::to_vec(&payload)?;
    frame.push(b'\n');
    Ok(frame)
}

fn initialize_with_only_blocked_capabilities(
    line_ending: &str,
) -> Result<Vec<u8>, serde_json::Error> {
    initialize_frame_with_capabilities(&blocked_capabilities(false), line_ending)
}

fn session_new_bytes() -> Vec<u8> {
    br#"{"jsonrpc":"2.0","id":1,"method":"session/new","params":{"cwd":"/workspace"}}"#.to_vec()
}

/// Returns a byte sequence that is syntactically invalid JSON (missing the
/// closing `}`), used to verify that malformed frames are forwarded unchanged.
pub(super) fn malformed_initialize_bytes() -> Vec<u8> {
    br#"{"jsonrpc":"2.0","method":"initialize","params":{"clientCapabilities":{"terminal":true}}"#
        .to_vec()
}

fn parse_frame_payload(frame: &[u8]) -> Result<serde_json::Value, serde_json::Error> {
    let (payload, _) = split_frame_line_ending(frame);
    serde_json::from_slice(payload)
}

/// Verifies that the blocked `fs` and `terminal` capabilities have been
/// removed while unrelated entries survive, returning a descriptive error on
/// the first violated expectation.
fn check_masked_client_capabilities(message: &serde_json::Value) -> Result<(), String> {
    let caps = client_capabilities(message)
        .ok_or_else(|| String::from("clientCapabilities should remain present"))?;
    if caps.contains_key(ACP_FILE_SYSTEM_CAPABILITY) {
        return Err(String::from("fs capability should be removed"));
    }
    if caps.contains_key(ACP_TERMINAL_CAPABILITY) {
        return Err(String::from("terminal capability should be removed"));
    }
    if !caps.contains_key("_meta") {
        return Err(String::from("unrelated capabilities should remain"));
    }
    Ok(())
}

fn client_capabilities(
    message: &serde_json::Value,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    message
        .get("params")
        .and_then(serde_json::Value::as_object)
        .and_then(|params| params.get("clientCapabilities"))
        .and_then(serde_json::Value::as_object)
}

fn params(message: &serde_json::Value) -> Option<&serde_json::Map<String, serde_json::Value>> {
    message.get("params").and_then(serde_json::Value::as_object)
}

async fn build_host_stdin(bytes: &[u8]) -> io::Result<DuplexStream> {
    let capacity = bytes.len().max(1);
    let (mut writer, reader) = tokio::io::duplex(capacity);
    writer.write_all(bytes).await?;
    drop(writer);
    Ok(reader)
}

/// Runs ACP stdin forwarding synchronously with `rewrite_acp_initialize =
/// true`, returning the bytes written to the container input and whether
/// `poll_shutdown` was called.
pub(super) fn run_forwarding(host_stdin_bytes: &[u8]) -> io::Result<(Vec<u8>, bool)> {
    run_forwarding_with_rewrite(host_stdin_bytes, true)
}

fn run_forwarding_with_rewrite(
    host_stdin_bytes: &[u8],
    rewrite_acp_initialize: bool,
) -> io::Result<(Vec<u8>, bool)> {
    let runtime = tokio::runtime::Runtime::new()?;
    let host_stdin = runtime.block_on(build_host_stdin(host_stdin_bytes))?;
    let container_input = RecordingInputWriter::new();
    let recorder = container_input.clone();

    runtime.block_on(forward_host_stdin_to_exec_async(
        host_stdin,
        Box::pin(container_input),
        rewrite_acp_initialize,
    ))?;

    Ok((recorder.snapshot(), recorder.shutdown_observed()))
}

/// Constructs a host-stdin byte sequence containing a masked `initialize`
/// frame followed by a follow-up frame, and returns both the raw input bytes
/// and the expected post-masking output bytes for BDD assertion.
pub(super) fn masked_initialize_with_follow_up() -> Result<(Vec<u8>, Vec<u8>), serde_json::Error> {
    let mut host_stdin_bytes = initialize_frame("\n")?;
    let follow_up = initialize_frame("\n")?;
    host_stdin_bytes.extend_from_slice(&follow_up);

    let expected_initialize = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientCapabilities": {
                "_meta": {
                    "custom": true
                }
            },
            "clientInfo": {
                "name": "podbot-tests",
                "version": "1.0.0"
            }
        }
    });

    let mut expected = serde_json::to_vec(&expected_initialize)?;
    expected.push(b'\n');
    expected.extend_from_slice(&follow_up);

    Ok((host_stdin_bytes, expected))
}

#[path = "protocol_acp_masking_tests.rs"]
mod masking_tests;

#[path = "protocol_acp_routing_tests.rs"]
mod capability_policy_routing;

#[path = "protocol_acp_forwarding_tests.rs"]
mod forwarding_tests;

#[path = "protocol_acp_policy_integration_tests.rs"]
mod policy_integration_tests;

#[path = "protocol_acp_bdd_tests.rs"]
mod bdd_tests;
