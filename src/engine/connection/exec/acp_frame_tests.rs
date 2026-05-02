//! Unit tests for the streaming Agentic Control Protocol (ACP) frame
//! assembler.
//!
//! These tests exercise the chunk-splitting, multi-chunk reassembly,
//! buffer-overflow fallback, and end-of-stream-drop behaviours of
//! [`OutboundFrameAssembler`] without invoking any I/O or async runtime.

use ortho_config::serde_json::{self, Value};
use rstest::rstest;

use super::{
    FallbackReason, FrameOutput, MAX_RUNTIME_FRAME_BYTES, OutboundFrameAssembler,
};
use crate::engine::connection::exec::protocol::acp_policy::{FrameDecision, MethodDenylist};

fn permitted_frame(method: &str, line_ending: &[u8]) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": {},
    }))
    .expect("frame serializes");
    bytes.extend_from_slice(line_ending);
    bytes
}

fn blocked_request_frame(id: Value, method: &str) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": {},
    }))
    .expect("frame serializes");
    bytes.push(b'\n');
    bytes
}

fn assembler() -> OutboundFrameAssembler {
    OutboundFrameAssembler::new(MethodDenylist::default_families())
}

fn collect_forward_bytes(outputs: &[FrameOutput]) -> Vec<u8> {
    outputs
        .iter()
        .filter_map(|output| match output {
            FrameOutput::Forward(bytes) => Some(bytes.clone()),
            FrameOutput::Decision(_, _) => None,
        })
        .fold(Vec::new(), |mut acc, mut bytes| {
            acc.append(&mut bytes);
            acc
        })
}

#[test]
fn single_permitted_frame_is_forwarded_verbatim() {
    let mut framer = assembler();
    let frame = permitted_frame("session/new", b"\n");

    let (outputs, fallback) = framer.ingest_chunk(&frame);

    assert!(fallback.is_none());
    assert_eq!(outputs, vec![FrameOutput::Forward(frame.clone())]);
    assert!(framer.finish().is_none(), "no residual buffered bytes");
}

#[test]
fn multiple_frames_in_one_chunk_split_on_each_newline() {
    let mut framer = assembler();
    let mut chunk = permitted_frame("session/new", b"\n");
    chunk.extend_from_slice(&permitted_frame("session/update", b"\n"));

    let (outputs, fallback) = framer.ingest_chunk(&chunk);

    assert!(fallback.is_none());
    assert_eq!(outputs.len(), 2);
    assert_eq!(collect_forward_bytes(&outputs), chunk);
}

#[test]
fn frame_split_across_two_chunks_reassembles_correctly() {
    let mut framer = assembler();
    let frame = permitted_frame("session/new", b"\n");
    let split_at = frame.len() / 2;
    let first = frame.get(..split_at).expect("split prefix");
    let second = frame.get(split_at..).expect("split suffix");

    let (outputs_a, fallback_a) = framer.ingest_chunk(first);
    let (outputs_b, fallback_b) = framer.ingest_chunk(second);

    assert!(fallback_a.is_none() && fallback_b.is_none());
    assert!(outputs_a.is_empty(), "no frame is complete after first chunk");
    assert_eq!(outputs_b, vec![FrameOutput::Forward(frame)]);
}

#[test]
fn frame_split_across_three_chunks_reassembles_correctly() {
    let mut framer = assembler();
    let frame = permitted_frame("session/update", b"\n");
    let third = frame.len() / 3;
    let two_thirds = (frame.len() * 2) / 3;
    let parts = [
        frame.get(..third).expect("first third"),
        frame.get(third..two_thirds).expect("middle third"),
        frame.get(two_thirds..).expect("last third"),
    ];

    let mut all_outputs = Vec::new();
    for part in parts {
        let (outputs, fallback) = framer.ingest_chunk(part);
        assert!(fallback.is_none());
        all_outputs.extend(outputs);
    }

    assert_eq!(all_outputs, vec![FrameOutput::Forward(frame)]);
}

#[test]
fn blocked_request_emits_decision_with_line_ending() {
    let mut framer = assembler();
    let frame = blocked_request_frame(serde_json::json!(7), "terminal/create");

    let (outputs, fallback) = framer.ingest_chunk(&frame);

    assert!(fallback.is_none());
    assert_eq!(outputs.len(), 1);
    match outputs.first() {
        Some(FrameOutput::Decision(FrameDecision::BlockRequest { id, method }, line_ending)) => {
            assert_eq!(id, &serde_json::json!(7));
            assert_eq!(method, "terminal/create");
            assert_eq!(line_ending, b"\n");
        }
        other => panic!("expected blocked request decision, got {other:?}"),
    }
}

#[test]
fn blocked_notification_emits_decision_without_id() {
    let mut framer = assembler();
    let mut frame = serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "method": "fs/changed",
    }))
    .expect("frame serializes");
    frame.push(b'\n');

    let (outputs, _) = framer.ingest_chunk(&frame);

    match outputs.first() {
        Some(FrameOutput::Decision(FrameDecision::BlockNotification { method }, line_ending)) => {
            assert_eq!(method, "fs/changed");
            assert_eq!(line_ending, b"\n");
        }
        other => panic!("expected blocked notification decision, got {other:?}"),
    }
}

#[rstest]
#[case::lf(b"\n" as &[u8])]
#[case::crlf(b"\r\n" as &[u8])]
fn line_ending_is_preserved_in_decision_output(#[case] line_ending: &[u8]) {
    let mut framer = assembler();
    let mut frame = serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "terminal/create",
    }))
    .expect("frame serializes");
    frame.extend_from_slice(line_ending);

    let (outputs, _) = framer.ingest_chunk(&frame);

    match outputs.first() {
        Some(FrameOutput::Decision(_, observed_line_ending)) => {
            assert_eq!(observed_line_ending.as_slice(), line_ending);
        }
        other => panic!("expected decision with line ending, got {other:?}"),
    }
}

#[test]
fn frame_with_escaped_newline_in_string_treated_as_single_frame() {
    let mut framer = assembler();
    let mut frame = serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "session/new",
        "params": {"text": "line one\\nline two"},
    }))
    .expect("frame with escaped newline serializes");
    frame.push(b'\n');

    let (outputs, _) = framer.ingest_chunk(&frame);

    assert_eq!(outputs, vec![FrameOutput::Forward(frame)]);
}

#[test]
fn permitted_frame_after_blocked_frame_still_forwards() {
    let mut framer = assembler();
    let mut chunk = blocked_request_frame(serde_json::json!(1), "terminal/create");
    let permitted = permitted_frame("session/new", b"\n");
    chunk.extend_from_slice(&permitted);

    let (outputs, _) = framer.ingest_chunk(&chunk);

    assert_eq!(outputs.len(), 2);
    assert!(matches!(
        outputs.first(),
        Some(FrameOutput::Decision(FrameDecision::BlockRequest { .. }, _))
    ));
    match outputs.get(1) {
        Some(FrameOutput::Forward(bytes)) => assert_eq!(bytes, &permitted),
        other => panic!("expected permitted forward, got {other:?}"),
    }
}

#[test]
fn buffer_overflow_flushes_buffered_bytes_and_enters_raw_fallback() {
    let mut framer = assembler();
    let oversize = vec![b'X'; MAX_RUNTIME_FRAME_BYTES + 1024];

    let (outputs, fallback) = framer.ingest_chunk(&oversize);

    assert_eq!(fallback, Some(FallbackReason::BufferOverflow));
    assert!(framer.is_raw_fallback());
    assert_eq!(collect_forward_bytes(&outputs), oversize);
}

#[test]
fn raw_fallback_forwards_subsequent_chunks_unchanged() {
    let mut framer = assembler();
    let oversize = vec![b'Y'; MAX_RUNTIME_FRAME_BYTES + 1];
    let _ = framer.ingest_chunk(&oversize);

    let (outputs, fallback) = framer.ingest_chunk(b"trailing-bytes\n");

    assert!(fallback.is_none());
    assert_eq!(outputs, vec![FrameOutput::Forward(b"trailing-bytes\n".to_vec())]);
}

#[test]
fn finish_drops_residual_partial_frame_and_reports_byte_count() {
    let mut framer = assembler();
    let partial = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session/new\"";

    let _ = framer.ingest_chunk(partial);
    let fallback = framer.finish();

    assert_eq!(
        fallback,
        Some(FallbackReason::DroppedPartialFrame {
            byte_count: partial.len(),
        })
    );
}

#[test]
fn finish_returns_none_when_buffer_empty() {
    let mut framer = assembler();
    let frame = permitted_frame("session/new", b"\n");

    let _ = framer.ingest_chunk(&frame);
    assert!(framer.finish().is_none());
}

#[test]
fn finish_returns_none_after_raw_fallback() {
    let mut framer = assembler();
    let oversize = vec![b'Z'; MAX_RUNTIME_FRAME_BYTES + 1];
    let _ = framer.ingest_chunk(&oversize);

    assert!(framer.finish().is_none());
}

#[test]
fn empty_chunk_produces_no_output() {
    let mut framer = assembler();
    let (outputs, fallback) = framer.ingest_chunk(b"");
    assert!(outputs.is_empty());
    assert!(fallback.is_none());
}
