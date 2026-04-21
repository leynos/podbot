//! ACP capability masking tests for the protocol stdin proxy.

use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use rstest::rstest;
use rstest_bdd::Slot;
use rstest_bdd_macros::{ScenarioState, given, scenario, then, when};
use tokio::io::{AsyncWrite, AsyncWriteExt, DuplexStream};

use super::*;

type StepResult<T> = Result<T, String>;

#[derive(Default, ScenarioState)]
struct AcpMaskingState {
    host_stdin: Slot<Vec<u8>>,
    expected_forwarded: Slot<Vec<u8>>,
    actual_forwarded: Slot<Vec<u8>>,
    succeeded: Slot<bool>,
}

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

fn initialize_without_blocked_capabilities() -> Vec<u8> {
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

fn malformed_initialize_bytes() -> Vec<u8> {
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

fn run_forwarding(host_stdin_bytes: &[u8]) -> (Vec<u8>, bool) {
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
    let trailing = session_new_bytes();
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
    let host_stdin_bytes = vec![b'x'; STDIN_BUFFER_CAPACITY + 1];
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
                forward_initial_protocol_frame_async(&mut buffered_stdin, &mut container_input),
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
        STDIN_BUFFER_CAPACITY,
        "only the bounded first-frame buffer should be held before streaming resumes"
    );

    drop(host_writer);
}

#[rstest::fixture]
fn acp_masking_state() -> AcpMaskingState {
    AcpMaskingState::default()
}

fn masked_initialize_with_follow_up() -> (Vec<u8>, Vec<u8>) {
    let mut host_stdin_bytes = initialize_frame("\n");
    host_stdin_bytes.extend_from_slice(&session_new_bytes());

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
    expected.extend_from_slice(&session_new_bytes());

    (host_stdin_bytes, expected)
}

#[given(
    "ACP stdin contains an initialize request with blocked capabilities and a follow-up request"
)]
fn acp_stdin_contains_blocked_initialize_and_follow_up(acp_masking_state: &AcpMaskingState) {
    let (host_stdin_bytes, expected) = masked_initialize_with_follow_up();
    acp_masking_state.host_stdin.set(host_stdin_bytes);
    acp_masking_state.expected_forwarded.set(expected);
}

#[given("ACP stdin contains malformed initialize bytes")]
fn acp_stdin_contains_malformed_initialize(acp_masking_state: &AcpMaskingState) {
    let malformed = malformed_initialize_bytes();
    acp_masking_state.host_stdin.set(malformed.clone());
    acp_masking_state.expected_forwarded.set(malformed);
}

#[given("ACP stdin contains initialize without blocked capabilities")]
fn acp_stdin_contains_safe_initialize(acp_masking_state: &AcpMaskingState) {
    let initialize = initialize_without_blocked_capabilities();
    acp_masking_state.host_stdin.set(initialize.clone());
    acp_masking_state.expected_forwarded.set(initialize);
}

#[when("ACP stdin forwarding runs")]
fn acp_stdin_forwarding_runs(acp_masking_state: &AcpMaskingState) -> StepResult<()> {
    let host_stdin_bytes = acp_masking_state
        .host_stdin
        .get()
        .ok_or_else(|| String::from("host stdin should be configured"))?;
    let (forwarded, _) = run_forwarding(&host_stdin_bytes);
    acp_masking_state.actual_forwarded.set(forwarded);
    acp_masking_state.succeeded.set(true);
    Ok(())
}

#[then("ACP stdin forwarding succeeds")]
fn acp_stdin_forwarding_succeeds(acp_masking_state: &AcpMaskingState) -> StepResult<()> {
    if acp_masking_state.succeeded.get() == Some(true) {
        Ok(())
    } else {
        Err(String::from("expected ACP stdin forwarding to succeed"))
    }
}

#[then("the forwarded ACP stdin matches the expected bytes")]
fn forwarded_acp_stdin_matches_expected(acp_masking_state: &AcpMaskingState) -> StepResult<()> {
    let actual = acp_masking_state
        .actual_forwarded
        .get()
        .ok_or_else(|| String::from("forwarded bytes should be recorded"))?;
    let expected = acp_masking_state
        .expected_forwarded
        .get()
        .ok_or_else(|| String::from("expected bytes should be recorded"))?;

    if actual == expected {
        Ok(())
    } else {
        Err(format!("expected {expected:?}, got {actual:?}"))
    }
}

#[scenario(
    path = "tests/features/acp_capability_masking.feature",
    name = "ACP initialize masks blocked capabilities before forwarding"
)]
fn acp_initialize_masks_blocked_capabilities(acp_masking_state: AcpMaskingState) {
    let _ = acp_masking_state;
}

#[scenario(
    path = "tests/features/acp_capability_masking.feature",
    name = "Malformed ACP initialize is forwarded unchanged"
)]
fn malformed_acp_initialize_is_forwarded_unchanged(acp_masking_state: AcpMaskingState) {
    let _ = acp_masking_state;
}

#[scenario(
    path = "tests/features/acp_capability_masking.feature",
    name = "ACP initialize without blocked capabilities stays unchanged"
)]
fn acp_initialize_without_blocked_capabilities_stays_unchanged(acp_masking_state: AcpMaskingState) {
    let _ = acp_masking_state;
}
