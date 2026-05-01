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

/// Returns a serialised ACP `initialize` frame whose `clientCapabilities` contains only
/// entries that are not masked (e.g. `_meta`), so the forwarded bytes should be identical
/// to the input.
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

/// Returns bytes that look like an ACP `initialize` message but are not valid JSON (the
/// closing brace is missing). Used to exercise the malformed-frame pass-through path.
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

async fn build_host_stdin(bytes: &[u8]) -> io::Result<DuplexStream> {
    let capacity = bytes.len().max(1);
    let (mut writer, reader) = tokio::io::duplex(capacity);
    writer.write_all(bytes).await?;
    drop(writer);
    Ok(reader)
}

/// Runs the full ACP-enabled stdin forwarding pipeline synchronously and returns
/// `(forwarded_bytes, shutdown_called)`.
///
/// Used by BDD scenario steps that need to drive `forward_host_stdin_to_exec_async` with
/// in-memory byte streams rather than real file descriptors.
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

/// Returns `(host_stdin_bytes, expected_forwarded_bytes)` for an ACP session that begins
/// with a full `initialize` frame (containing blocked capabilities) followed by a second
/// frame.
///
/// The expected output has the blocked capabilities removed from the first frame while the
/// second frame is preserved byte-for-byte.
pub(super) fn masked_initialize_with_follow_up() -> (Vec<u8>, Vec<u8>) {
    let mut host_stdin_bytes = initialize_frame("\n");
    let follow_up = initialize_frame("\n");


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
