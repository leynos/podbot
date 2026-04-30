//! ACP capability masking tests for the protocol stdin proxy.

use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use rstest::rstest;
use tokio::io::{AsyncWrite, AsyncWriteExt, DuplexStream};

use super::acp_helpers::{
    ACP_FILE_SYSTEM_CAPABILITY, ACP_TERMINAL_CAPABILITY, MAX_FIRST_FRAME_BYTES,
    forward_initial_acp_frame_async, mask_acp_initialize_frame, split_frame_line_ending,
};
use super::*;

struct RecordingInputWriter {
    bytes: Arc<Mutex<Vec<u8>>>,
    shutdown_called: Arc<Mutex<bool>>,
}

impl RecordingInputWriter {
    fn new() -> Self {
        Self {
            bytes: Arc::new(Mutex::new(Vec::new())),
            shutdown_called: Arc::new(Mutex::new(false)),
        }
    }
}

impl AsyncWrite for RecordingInputWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.bytes
            .lock()
            .expect("writer mutex should not poison")
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
            .expect("shutdown mutex should not poison") = true;
        Poll::Ready(Ok(()))
    }
}

fn initialize_frame(line_ending: &str) -> Vec<u8> {
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientCapabilities": {
                "fs": {
                    "readTextFile": true,
                    "writeTextFile": true
                },
                "terminal": true,
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

    let mut frame = serde_json::to_vec(&payload).expect("initialize payload should serialize");
    frame.extend_from_slice(line_ending.as_bytes());
    frame
}

/// Builds a serialised ACP `initialize` frame whose `clientCapabilities`
/// contains only `_meta` (no blocked entries), terminated with `\n`.
pub(super) fn initialize_without_blocked_capabilities() -> Vec<u8> {
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

    let mut frame = serde_json::to_vec(&payload).expect("initialize payload should serialize");
    frame.push(b'\n');
    frame
}

fn initialize_with_only_blocked_capabilities(line_ending: &str) -> Vec<u8> {
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientCapabilities": {
                "fs": {
                    "readTextFile": true,
                    "writeTextFile": true
                },
                "terminal": true
            },
            "clientInfo": {
                "name": "podbot-tests",
                "version": "1.0.0"
            }
        }
    });

    let mut frame = serde_json::to_vec(&payload).expect("initialize payload should serialize");
    frame.extend_from_slice(line_ending.as_bytes());
    frame
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

fn parse_frame_payload(frame: &[u8]) -> serde_json::Value {
    let (payload, _) = split_frame_line_ending(frame);
    serde_json::from_slice(payload).expect("frame should contain JSON payload")
}

fn assert_masked_client_capabilities(message: &serde_json::Value) {
    let client_capabilities = message
        .get("params")
        .and_then(serde_json::Value::as_object)
        .and_then(|params| params.get("clientCapabilities"))
        .and_then(serde_json::Value::as_object)
        .expect("clientCapabilities should remain present");
    assert!(
        !client_capabilities.contains_key(ACP_FILE_SYSTEM_CAPABILITY),
        "fs capability should be removed"
    );
    assert!(
        !client_capabilities.contains_key(ACP_TERMINAL_CAPABILITY),
        "terminal capability should be removed"
    );
    assert!(
        client_capabilities.contains_key("_meta"),
        "unrelated capabilities should remain"
    );
}

fn client_capabilities(message: &serde_json::Value) -> &serde_json::Map<String, serde_json::Value> {
    message
        .get("params")
        .and_then(serde_json::Value::as_object)
        .and_then(|params| params.get("clientCapabilities"))
        .and_then(serde_json::Value::as_object)
        .expect("clientCapabilities should remain")
}

fn params(message: &serde_json::Value) -> &serde_json::Map<String, serde_json::Value> {
    message
        .get("params")
        .and_then(serde_json::Value::as_object)
        .expect("params should remain")
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
pub(super) fn run_forwarding(host_stdin_bytes: &[u8]) -> (Vec<u8>, bool) {
    run_forwarding_with_rewrite(host_stdin_bytes, true)
}

fn run_forwarding_with_rewrite(
    host_stdin_bytes: &[u8],
    rewrite_acp_initialize: bool,
) -> (Vec<u8>, bool) {
    let runtime = tokio::runtime::Runtime::new().expect("runtime should build");
    let host_stdin = runtime
        .block_on(build_host_stdin(host_stdin_bytes))
        .expect("host stdin should build");
    let container_input = RecordingInputWriter::new();
    let forwarded_bytes = container_input.bytes.clone();
    let shutdown_called = container_input.shutdown_called.clone();

    runtime
        .block_on(forward_host_stdin_to_exec_async(
            host_stdin,
            Box::pin(container_input),
            rewrite_acp_initialize,
        ))
        .expect("stdin forwarding should succeed");

    (
        forwarded_bytes
            .lock()
            .expect("writer mutex should not poison")
            .clone(),
        *shutdown_called
            .lock()
            .expect("shutdown mutex should not poison"),
    )
}

#[test]
fn forwarding_leaves_initialize_unchanged_when_acp_rewrite_is_disabled() {
    let host_stdin_bytes = initialize_frame("\n");

    let (forwarded, shutdown_called) = run_forwarding_with_rewrite(&host_stdin_bytes, false);

    assert_eq!(
        forwarded, host_stdin_bytes,
        "generic protocol sessions should retain raw byte-stream semantics"
    );
    assert!(shutdown_called, "stdin forwarding should shut down input");
}

#[rstest]
#[case("\n")]
#[case("\r\n")]
fn mask_acp_initialize_frame_removes_blocked_capabilities(#[case] line_ending: &str) {
    let frame = initialize_frame(line_ending);
    let masked = mask_acp_initialize_frame(&frame);
    let payload = parse_frame_payload(&masked);

    assert_eq!(
        split_frame_line_ending(&masked).1,
        line_ending.as_bytes(),
        "line ending should be preserved"
    );
    assert_masked_client_capabilities(&payload);
}

#[test]
fn mask_acp_initialize_frame_removes_empty_client_capabilities() {
    let frame = initialize_with_only_blocked_capabilities("\n");
    let masked = mask_acp_initialize_frame(&frame);
    let payload = parse_frame_payload(&masked);
    let params = payload
        .get("params")
        .and_then(serde_json::Value::as_object)
        .expect("initialize params should remain present");

    assert!(
        !params.contains_key("clientCapabilities"),
        "clientCapabilities should be removed when all entries are masked"
    );
    assert_eq!(
        params.get("protocolVersion"),
        Some(&serde_json::json!(1)),
        "protocolVersion should remain unchanged"
    );
    assert_eq!(
        params.get("clientInfo"),
        Some(&serde_json::json!({
            "name": "podbot-tests",
            "version": "1.0.0"
        })),
        "clientInfo should remain unchanged"
    );
}

#[test]
fn mask_acp_initialize_frame_removes_only_fs_when_terminal_absent() {
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientCapabilities": {
                "fs": { "readTextFile": true },
                "auth": { "token": true }
            },
            "clientInfo": { "name": "podbot-tests", "version": "1.0.0" }
        }
    });
    let mut frame = serde_json::to_vec(&payload).expect("payload should serialise");
    frame.push(b'\n');

    let masked = mask_acp_initialize_frame(&frame);
    let result = parse_frame_payload(&masked);
    let caps = client_capabilities(&result);

    assert!(!caps.contains_key("fs"), "fs should be removed");
    assert!(caps.contains_key("auth"), "auth should be preserved");
}

#[test]
fn mask_acp_initialize_frame_removes_only_terminal_when_fs_absent() {
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientCapabilities": {
                "terminal": true,
                "logging": { "level": "info" }
            },
            "clientInfo": { "name": "podbot-tests", "version": "1.0.0" }
        }
    });
    let mut frame = serde_json::to_vec(&payload).expect("payload should serialise");
    frame.push(b'\n');

    let masked = mask_acp_initialize_frame(&frame);
    let result = parse_frame_payload(&masked);
    let caps = client_capabilities(&result);

    assert!(!caps.contains_key("terminal"), "terminal should be removed");
    assert!(caps.contains_key("logging"), "logging should be preserved");
}

#[test]
fn mask_acp_initialize_frame_preserves_all_unrelated_capabilities() {
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientCapabilities": {
                "fs": { "readTextFile": true },
                "terminal": true,
                "auth": { "token": true },
                "logging": { "level": "debug" }
            },
            "clientInfo": { "name": "podbot-tests", "version": "1.0.0" }
        }
    });
    let mut frame = serde_json::to_vec(&payload).expect("payload should serialise");
    frame.push(b'\n');

    let masked = mask_acp_initialize_frame(&frame);
    let result = parse_frame_payload(&masked);
    let caps = client_capabilities(&result);

    assert!(!caps.contains_key("fs"), "fs should be removed");
    assert!(!caps.contains_key("terminal"), "terminal should be removed");
    assert!(caps.contains_key("auth"), "auth should be preserved");
    assert!(caps.contains_key("logging"), "logging should be preserved");
}

#[test]
fn mask_acp_initialize_frame_passes_through_frame_without_line_ending() {
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "initialize",
        "params": {
            "clientCapabilities": { "fs": { "readTextFile": true }, "terminal": true }
        }
    });
    // Deliberately omit the trailing newline.
    let frame = serde_json::to_vec(&payload).expect("payload should serialise");

    let masked = mask_acp_initialize_frame(&frame);

    // Without a line ending the splitter returns the whole slice as payload
    // with an empty line-ending suffix.  The JSON itself is still rewritten,
    // but the result must not gain a spurious newline byte.
    assert!(
        !masked.ends_with(b"\n"),
        "masked frame should not gain a trailing newline"
    );
    let result: serde_json::Value =
        serde_json::from_slice(&masked).expect("result should be valid JSON");
    assert!(
        params(&result).get("clientCapabilities").is_none(),
        "capabilities should still be masked even without a line ending"
    );
}

#[test]
fn mask_acp_initialize_frame_leaves_non_initialize_messages_unchanged() {
    let mut frame = session_new_bytes();
    frame.push(b'\n');

    assert_eq!(mask_acp_initialize_frame(&frame), frame);
}

#[test]
fn mask_acp_initialize_frame_leaves_malformed_input_unchanged() {
    let frame = malformed_initialize_bytes();

    assert_eq!(mask_acp_initialize_frame(&frame), frame);
}

#[test]
fn forwarding_masks_initialize_and_preserves_trailing_bytes() {
    let mut host_stdin_bytes = initialize_frame("\n");
    let trailing = initialize_frame("\n");
    host_stdin_bytes.extend_from_slice(&trailing);

    let (forwarded, shutdown_called) = run_forwarding(&host_stdin_bytes);
    let newline_index = forwarded
        .iter()
        .position(|byte| *byte == b'\n')
        .expect("masked initialize should remain line terminated");
    let initialize_frame = forwarded
        .get(..=newline_index)
        .expect("masked initialize frame should remain addressable");
    let trailing_forwarded = forwarded
        .get(newline_index + 1..)
        .expect("trailing bytes should remain addressable");
    let payload = parse_frame_payload(initialize_frame);

    assert_masked_client_capabilities(&payload);
    assert_eq!(
        trailing_forwarded,
        trailing.as_slice(),
        "trailing bytes should pass through unchanged"
    );
    assert!(shutdown_called, "stdin forwarding should shut down input");
}

#[test]
fn forwarding_does_not_wait_indefinitely_for_oversized_initial_frame() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime should build");
    let host_stdin_bytes = vec![b'x'; MAX_FIRST_FRAME_BYTES + 1];
    let (host_writer, host_reader) = runtime
        .block_on(async {
            let (mut host_writer, host_reader) = tokio::io::duplex(host_stdin_bytes.len());
            host_writer.write_all(&host_stdin_bytes).await?;
            io::Result::Ok((host_writer, host_reader))
        })
        .expect("host stdin should accept oversized initial bytes");

    let mut buffered_stdin =
        tokio::io::BufReader::with_capacity(STDIN_BUFFER_CAPACITY, host_reader);
    let recording_input = RecordingInputWriter::new();
    let forwarded_bytes = recording_input.bytes.clone();
    let mut container_input: Pin<Box<dyn AsyncWrite + Send>> = Box::pin(recording_input);

    runtime
        .block_on(async {
            tokio::time::timeout(
                STDIN_SETTLE_TIMEOUT,
                forward_initial_acp_frame_async(&mut buffered_stdin, &mut container_input),
            )
            .await
        })
        .expect("initial forwarding should not wait for newline or EOF")
        .expect("initial forwarding should succeed");

    assert_eq!(
        forwarded_bytes
            .lock()
            .expect("writer mutex should not poison")
            .len(),
        MAX_FIRST_FRAME_BYTES,
        "only the bounded first-frame buffer should be held before streaming resumes"
    );

    drop(host_writer);
}

/// Constructs a host-stdin byte sequence containing a masked `initialize`
/// frame followed by a follow-up frame, and returns both the raw input bytes
/// and the expected post-masking output bytes for BDD assertion.
pub(super) fn masked_initialize_with_follow_up() -> (Vec<u8>, Vec<u8>) {
    let mut host_stdin_bytes = initialize_frame("\n");
    let follow_up = initialize_frame("\n");
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

    let mut expected = serde_json::to_vec(&expected_initialize)
        .expect("expected initialize payload should serialize");
    expected.push(b'\n');
    expected.extend_from_slice(&follow_up);

    (host_stdin_bytes, expected)
}

#[path = "protocol_acp_bdd_tests.rs"]
mod bdd_tests;
