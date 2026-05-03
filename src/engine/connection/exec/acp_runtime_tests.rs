//! Unit tests for the Agentic Control Protocol (ACP) runtime adapter and
//! container-stdin sink task.

use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use ortho_config::serde_json::{self, Value};
use rstest::rstest;
use tokio::io::AsyncWrite;
use tokio::sync::mpsc;

use super::{
    OutboundFrameAssembler, OutboundPolicyAdapter, SINK_CHANNEL_CAPACITY, WriteCmd,
    run_container_stdin_sink,
};
use crate::engine::connection::exec::protocol::acp_policy::MethodDenylist;

/// Recording host-stdout writer that captures every byte and tracks shutdown.
#[derive(Clone, Default)]
struct RecordingWriter {
    bytes: Arc<Mutex<Vec<u8>>>,
    shutdown_called: Arc<Mutex<bool>>,
}

impl RecordingWriter {
    fn snapshot(&self) -> Vec<u8> {
        self.bytes.lock().expect("writer mutex").clone()
    }

    fn shutdown_observed(&self) -> bool {
        *self.shutdown_called.lock().expect("shutdown mutex")
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
            .expect("writer mutex")
            .extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        *self.shutdown_called.lock().expect("shutdown mutex") = true;
        Poll::Ready(Ok(()))
    }
}

/// Writer that always returns `BrokenPipe` on writes, used to simulate the
/// agent having already exited.
struct BrokenPipeWriter;

impl AsyncWrite for BrokenPipeWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "agent exited",
        )))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

/// Builds a newline-terminated JSON-RPC 2.0 frame.
///
/// Pass `id = Some(…)` for requests; `id = None` for notifications.
fn make_jsonrpc_frame(method: &str, id: Option<&Value>) -> Vec<u8> {
    let value = id.map_or_else(
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
    let mut bytes = serde_json::to_vec(&value).expect("frame serializes");
    bytes.push(b'\n');
    bytes
}

fn permitted_frame() -> Vec<u8> {
    make_jsonrpc_frame("session/new", Some(&serde_json::json!(1)))
}

fn blocked_request_frame(id: &Value) -> Vec<u8> {
    make_jsonrpc_frame("terminal/create", Some(id))
}

fn blocked_notification_frame() -> Vec<u8> {
    make_jsonrpc_frame("fs/changed", None)
}

fn build_adapter() -> (OutboundPolicyAdapter, mpsc::Receiver<WriteCmd>) {
    let (tx, rx) = mpsc::channel::<WriteCmd>(SINK_CHANNEL_CAPACITY);
    let assembler = OutboundFrameAssembler::new(MethodDenylist::default_families());
    let adapter = OutboundPolicyAdapter::new(assembler, tx, "container-test");
    (adapter, rx)
}

async fn drain_channel(mut rx: mpsc::Receiver<WriteCmd>) -> Vec<WriteCmd> {
    let mut received = Vec::new();
    rx.close();
    while let Some(cmd) = rx.recv().await {
        received.push(cmd);
    }
    received
}

#[tokio::test]
async fn permitted_frame_writes_to_host_stdout_only() {
    let (mut adapter, rx) = build_adapter();
    let host_stdout = RecordingWriter::default();
    let recorder = host_stdout.clone();
    let mut writer: Pin<Box<dyn AsyncWrite + Send + Unpin>> = Box::pin(host_stdout);
    let frame = permitted_frame();

    adapter
        .handle_chunk(&frame, &mut writer)
        .await
        .expect("permitted frame writes succeed");
    adapter.finish();
    drop(adapter);

    assert_eq!(recorder.snapshot(), frame);
    let received = drain_channel(rx).await;
    assert!(received.is_empty(), "no commands should reach the sink");
}

#[tokio::test]
async fn blocked_request_skips_host_stdout_and_queues_synthesised_response() {
    let (mut adapter, rx) = build_adapter();
    let host_stdout = RecordingWriter::default();
    let recorder = host_stdout.clone();
    let mut writer: Pin<Box<dyn AsyncWrite + Send + Unpin>> = Box::pin(host_stdout);
    let frame = blocked_request_frame(&serde_json::json!(7));

    adapter
        .handle_chunk(&frame, &mut writer)
        .await
        .expect("blocked request handles cleanly");
    drop(adapter);

    assert!(
        recorder.snapshot().is_empty(),
        "blocked request must never reach host stdout",
    );
    let received = drain_channel(rx).await;
    let bytes = match received.as_slice() {
        [WriteCmd::Synthesised(bytes)] => bytes.clone(),
        other => panic!("expected one Synthesised, got {other:?}"),
    };
    let payload_end = bytes.len().checked_sub(1).expect("trailing newline");
    let payload_slice = bytes.get(..payload_end).expect("payload before newline");
    let parsed: Value =
        serde_json::from_slice(payload_slice).expect("synthesized response is valid JSON");
    assert_eq!(parsed.get("id"), Some(&serde_json::json!(7)));
    assert_eq!(
        parsed
            .get("error")
            .and_then(|err| err.get("data"))
            .and_then(|data| data.get("method")),
        Some(&serde_json::json!("terminal/create")),
    );
}

#[tokio::test]
async fn blocked_notification_drops_silently_without_sink_command() {
    let (mut adapter, rx) = build_adapter();
    let host_stdout = RecordingWriter::default();
    let recorder = host_stdout.clone();
    let mut writer: Pin<Box<dyn AsyncWrite + Send + Unpin>> = Box::pin(host_stdout);
    let frame = blocked_notification_frame();

    adapter
        .handle_chunk(&frame, &mut writer)
        .await
        .expect("blocked notification handles cleanly");
    drop(adapter);

    assert!(recorder.snapshot().is_empty());
    let received = drain_channel(rx).await;
    assert!(
        received.is_empty(),
        "notifications must not generate a response"
    );
}

#[tokio::test]
async fn permitted_frame_after_blocked_frame_still_reaches_host_stdout() {
    let (mut adapter, rx) = build_adapter();
    let host_stdout = RecordingWriter::default();
    let recorder = host_stdout.clone();
    let mut writer: Pin<Box<dyn AsyncWrite + Send + Unpin>> = Box::pin(host_stdout);
    let mut chunk = blocked_request_frame(&serde_json::json!(1));
    let permitted = permitted_frame();
    chunk.extend_from_slice(&permitted);

    adapter
        .handle_chunk(&chunk, &mut writer)
        .await
        .expect("mixed chunk handles cleanly");
    drop(adapter);

    assert_eq!(recorder.snapshot(), permitted);
    let received = drain_channel(rx).await;
    assert!(matches!(received.as_slice(), [WriteCmd::Synthesised(_)]));
}

#[tokio::test]
async fn frame_split_across_chunks_is_classified_after_assembly() {
    let (mut adapter, rx) = build_adapter();
    let host_stdout = RecordingWriter::default();
    let recorder = host_stdout.clone();
    let mut writer: Pin<Box<dyn AsyncWrite + Send + Unpin>> = Box::pin(host_stdout);
    let frame = blocked_request_frame(&serde_json::json!(2));
    let split_at = frame.len().div_euclid(2);
    let first = frame.get(..split_at).expect("split prefix");
    let second = frame.get(split_at..).expect("split suffix");

    adapter
        .handle_chunk(first, &mut writer)
        .await
        .expect("first chunk");
    adapter
        .handle_chunk(second, &mut writer)
        .await
        .expect("second chunk");
    drop(adapter);

    assert!(recorder.snapshot().is_empty());
    let received = drain_channel(rx).await;
    assert!(matches!(received.as_slice(), [WriteCmd::Synthesised(_)]));
}

#[tokio::test]
async fn sink_writes_forwards_then_synthesised_in_send_order() {
    let recorder = RecordingWriter::default();
    let writer_handle = recorder.clone();
    let writer: Pin<Box<dyn AsyncWrite + Send>> = Box::pin(recorder);
    let (tx, rx) = mpsc::channel::<WriteCmd>(SINK_CHANNEL_CAPACITY);
    let sink = tokio::spawn(run_container_stdin_sink(writer, rx));

    tx.send(WriteCmd::Forward(b"forward-one\n".to_vec()))
        .await
        .expect("forward send");
    tx.send(WriteCmd::Synthesised(b"synthesised-one\n".to_vec()))
        .await
        .expect("synthesised send");
    tx.send(WriteCmd::Forward(b"forward-two\n".to_vec()))
        .await
        .expect("second forward send");
    drop(tx);

    sink.await
        .expect("sink task joins")
        .expect("sink runs cleanly");

    let bytes = writer_handle.snapshot();
    assert_eq!(bytes, b"forward-one\nsynthesised-one\nforward-two\n");
    assert!(writer_handle.shutdown_observed());
}

#[tokio::test]
async fn sink_continues_after_broken_pipe_until_channel_closes() {
    let writer: Pin<Box<dyn AsyncWrite + Send>> = Box::pin(BrokenPipeWriter);
    let (tx, rx) = mpsc::channel::<WriteCmd>(SINK_CHANNEL_CAPACITY);
    let sink = tokio::spawn(run_container_stdin_sink(writer, rx));

    tx.send(WriteCmd::Forward(b"first\n".to_vec()))
        .await
        .expect("first forward");
    tx.send(WriteCmd::Synthesised(b"second\n".to_vec()))
        .await
        .expect("second send");
    drop(tx);

    sink.await
        .expect("sink task joins")
        .expect("sink absorbs broken pipe without erroring");
}

#[rstest]
#[case::lf(b"\n" as &[u8])]
#[case::crlf(b"\r\n" as &[u8])]
#[tokio::test]
async fn synthesised_response_preserves_blocked_frame_line_ending(#[case] line_ending: &[u8]) {
    let (mut adapter, rx) = build_adapter();
    let host_stdout = RecordingWriter::default();
    let mut writer: Pin<Box<dyn AsyncWrite + Send + Unpin>> = Box::pin(host_stdout);
    let mut frame = serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "terminal/create",
        "params": {},
    }))
    .expect("blocked request serializes");
    frame.extend_from_slice(line_ending);

    adapter
        .handle_chunk(&frame, &mut writer)
        .await
        .expect("chunk");
    drop(adapter);

    let received = drain_channel(rx).await;
    let bytes = match received.as_slice() {
        [WriteCmd::Synthesised(bytes)] => bytes.clone(),
        other => panic!("expected synthesised response, got {other:?}"),
    };
    assert!(
        bytes.ends_with(line_ending),
        "synthesised response should reuse the blocked frame's line ending",
    );
}

#[tokio::test]
async fn blocked_request_synthesised_before_channel_close_is_flushed() {
    let recorder = RecordingWriter::default();
    let writer_handle = recorder.clone();
    let writer: Pin<Box<dyn AsyncWrite + Send>> = Box::pin(recorder);
    let (tx, rx) = mpsc::channel::<WriteCmd>(SINK_CHANNEL_CAPACITY);
    let sink = tokio::spawn(run_container_stdin_sink(writer, rx));

    let assembler = OutboundFrameAssembler::new(MethodDenylist::default_families());
    let mut adapter = OutboundPolicyAdapter::new(assembler, tx.clone(), "container-test");
    let host_stdout = RecordingWriter::default();
    let mut host_writer: Pin<Box<dyn AsyncWrite + Send + Unpin>> = Box::pin(host_stdout);
    let frame = blocked_request_frame(&serde_json::json!(11));

    adapter
        .handle_chunk(&frame, &mut host_writer)
        .await
        .expect("chunk");
    adapter.finish();
    drop(adapter);
    drop(tx);

    sink.await.expect("sink joins").expect("sink runs cleanly");

    let bytes = writer_handle.snapshot();
    assert!(
        !bytes.is_empty(),
        "synthesised response must be flushed before container stdin closes",
    );
    let parsed: Value = serde_json::from_slice(
        bytes
            .strip_suffix(b"\n")
            .expect("synthesised response ends with newline"),
    )
    .expect("synthesised response is valid JSON");
    assert_eq!(parsed.get("id"), Some(&serde_json::json!(11)));
}
