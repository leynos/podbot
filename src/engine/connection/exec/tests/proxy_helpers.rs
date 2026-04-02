//! Unit tests for protocol byte-stream proxy helpers.

use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use bollard::container::LogOutput;
use bollard::errors::Error as BollardError;
use futures_util::stream;
use rstest::rstest;
use tokio::io::{AsyncWrite, AsyncWriteExt, DuplexStream};

use super::super::protocol::{ProtocolProxyIo, run_protocol_session_with_io_async};
use super::*;

#[derive(Clone, Copy)]
enum WriterFailureMode {
    Write,
    Flush,
}

struct RecordingWriter {
    bytes: Arc<Mutex<Vec<u8>>>,
    failure_mode: Option<WriterFailureMode>,
}

impl RecordingWriter {
    fn new() -> Self {
        Self {
            bytes: Arc::new(Mutex::new(Vec::new())),
            failure_mode: None,
        }
    }

    fn with_failure(failure_mode: WriterFailureMode) -> Self {
        Self {
            bytes: Arc::new(Mutex::new(Vec::new())),
            failure_mode: Some(failure_mode),
        }
    }
}

impl AsyncWrite for RecordingWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if matches!(self.failure_mode, Some(WriterFailureMode::Write)) {
            return Poll::Ready(Err(io::Error::other("writer failure")));
        }

        self.bytes
            .lock()
            .expect("writer mutex should not poison")
            .extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if matches!(self.failure_mode, Some(WriterFailureMode::Flush)) {
            return Poll::Ready(Err(io::Error::other("flush failure")));
        }

        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

struct RecordingInputWriter {
    bytes: Arc<Mutex<Vec<u8>>>,
    shutdown_called: Arc<Mutex<bool>>,
    fail_on_flush: bool,
}

impl RecordingInputWriter {
    fn new() -> Self {
        Self {
            bytes: Arc::new(Mutex::new(Vec::new())),
            shutdown_called: Arc::new(Mutex::new(false)),
            fail_on_flush: false,
        }
    }

    fn with_flush_failure() -> Self {
        Self {
            bytes: Arc::new(Mutex::new(Vec::new())),
            shutdown_called: Arc::new(Mutex::new(false)),
            fail_on_flush: true,
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
        if self.fail_on_flush {
            return Poll::Ready(Err(io::Error::other("stdin flush failure")));
        }

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

async fn make_host_stdin(bytes: &[u8]) -> io::Result<DuplexStream> {
    let capacity = bytes.len().max(1);
    let (mut writer, reader) = tokio::io::duplex(capacity);
    writer.write_all(bytes).await?;
    drop(writer);
    Ok(reader)
}

fn make_protocol_request() -> Result<ExecRequest, PodbotError> {
    ExecRequest::new(
        "protocol-sandbox",
        vec![String::from("codex"), String::from("app-server")],
        ExecMode::Protocol,
    )
}

fn make_output_stream(
    chunks: Vec<Result<LogOutput, BollardError>>,
) -> Pin<Box<dyn futures_util::Stream<Item = Result<LogOutput, BollardError>> + Send>> {
    Box::pin(stream::iter(chunks))
}

fn assert_exec_failed_message(result: Result<(), PodbotError>, expected_fragment: &str) {
    match result {
        Err(PodbotError::Container(ContainerError::ExecFailed { message, .. }))
            if message.contains(expected_fragment) => {}
        other => panic!("expected exec failure containing '{expected_fragment}', got {other:?}"),
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "test helper wires protocol sessions with explicit stream handles"
)]
fn run_session(
    runtime: RuntimeFixture,
    stdin_bytes: &[u8],
    output: Pin<Box<dyn futures_util::Stream<Item = Result<LogOutput, BollardError>> + Send>>,
    container_input: Pin<Box<dyn AsyncWrite + Send>>,
    host_stdout: RecordingWriter,
    host_stderr: RecordingWriter,
) -> Result<(), PodbotError> {
    let runtime_handle = runtime.expect("runtime fixture should initialize");
    let request = make_protocol_request().expect("protocol request should build");
    let host_stdin = runtime_handle
        .block_on(make_host_stdin(stdin_bytes))
        .expect("host stdin should build");

    runtime_handle.block_on(run_protocol_session_with_io_async(
        &request,
        output,
        container_input,
        ProtocolProxyIo::new(host_stdin, host_stdout, host_stderr),
    ))
}

#[expect(
    clippy::type_complexity,
    reason = "test helper returns the paired captured writer buffers"
)]
fn run_routing_session(
    runtime: RuntimeFixture,
    output: Pin<Box<dyn futures_util::Stream<Item = Result<LogOutput, BollardError>> + Send>>,
) -> (
    Result<(), PodbotError>,
    Arc<Mutex<Vec<u8>>>,
    Arc<Mutex<Vec<u8>>>,
) {
    let host_stdout = RecordingWriter::new();
    let host_stderr = RecordingWriter::new();
    let stdout_bytes = host_stdout.bytes.clone();
    let stderr_bytes = host_stderr.bytes.clone();
    let result = run_session(
        runtime,
        b"",
        output,
        Box::pin(RecordingInputWriter::new()),
        host_stdout,
        host_stderr,
    );
    (result, stdout_bytes, stderr_bytes)
}

#[rstest]
fn protocol_proxy_forwards_stdin_bytes_and_shutdowns_input(runtime: RuntimeFixture) {
    let host_stdout = RecordingWriter::new();
    let host_stderr = RecordingWriter::new();
    let container_input = RecordingInputWriter::new();
    let expected_shutdown = container_input.shutdown_called.clone();
    let expected_bytes = container_input.bytes.clone();
    let output = make_output_stream(vec![Ok(LogOutput::StdOut {
        message: Vec::from(&b"stdout"[..]).into(),
    })]);

    let result = run_session(
        runtime,
        b"input payload",
        output,
        Box::pin(container_input),
        host_stdout,
        host_stderr,
    );

    assert!(result.is_ok(), "protocol proxy should succeed: {result:?}");
    assert_eq!(
        expected_bytes
            .lock()
            .expect("writer mutex should not poison")
            .clone(),
        b"input payload"
    );
    assert!(
        *expected_shutdown
            .lock()
            .expect("shutdown mutex should not poison"),
        "container stdin should be shut down after EOF"
    );
}

#[rstest]
fn protocol_proxy_routes_stdout_and_console_to_host_stdout(runtime: RuntimeFixture) {
    let output = make_output_stream(vec![
        Ok(LogOutput::StdOut {
            message: Vec::from(&b"alpha"[..]).into(),
        }),
        Ok(LogOutput::Console {
            message: Vec::from(&b"beta"[..]).into(),
        }),
    ]);

    let (result, stdout_bytes, stderr_bytes) = run_routing_session(runtime, output);

    assert!(result.is_ok(), "protocol proxy should succeed: {result:?}");
    assert_eq!(
        stdout_bytes
            .lock()
            .expect("writer mutex should not poison")
            .clone(),
        b"alphabeta"
    );
    assert!(
        stderr_bytes
            .lock()
            .expect("writer mutex should not poison")
            .is_empty(),
        "stderr should remain untouched"
    );
}

#[rstest]
fn protocol_proxy_routes_stderr_to_host_stderr(runtime: RuntimeFixture) {
    let output = make_output_stream(vec![
        Ok(LogOutput::StdErr {
            message: Vec::from(&b"warn"[..]).into(),
        }),
        Ok(LogOutput::StdOut {
            message: Vec::from(&b"ok"[..]).into(),
        }),
    ]);

    let (result, stdout_bytes, stderr_bytes) = run_routing_session(runtime, output);

    assert!(result.is_ok(), "protocol proxy should succeed: {result:?}");
    assert_eq!(
        stderr_bytes
            .lock()
            .expect("writer mutex should not poison")
            .clone(),
        b"warn"
    );
    assert_eq!(
        stdout_bytes
            .lock()
            .expect("writer mutex should not poison")
            .clone(),
        b"ok"
    );
}

#[rstest]
fn protocol_proxy_ignores_stdin_echo_chunks(runtime: RuntimeFixture) {
    let output = make_output_stream(vec![
        Ok(LogOutput::StdIn {
            message: Vec::from(&b"echo"[..]).into(),
        }),
        Ok(LogOutput::StdOut {
            message: Vec::from(&b"server"[..]).into(),
        }),
    ]);

    let (result, stdout_bytes, _) = run_routing_session(runtime, output);

    assert!(result.is_ok(), "protocol proxy should succeed: {result:?}");
    assert_eq!(
        stdout_bytes
            .lock()
            .expect("writer mutex should not poison")
            .clone(),
        b"server"
    );
}

#[rstest]
#[case(WriterFailureMode::Write, "failed writing stdout output")]
#[case(WriterFailureMode::Flush, "failed flushing stdout output")]
fn protocol_proxy_maps_stdout_failures(
    runtime: RuntimeFixture,
    #[case] failure_mode: WriterFailureMode,
    #[case] expected_fragment: &str,
) {
    let output = make_output_stream(vec![Ok(LogOutput::StdOut {
        message: Vec::from(&b"broken"[..]).into(),
    })]);

    let result = run_session(
        runtime,
        b"",
        output,
        Box::pin(RecordingInputWriter::new()),
        RecordingWriter::with_failure(failure_mode),
        RecordingWriter::new(),
    );

    assert_exec_failed_message(result, expected_fragment);
}

#[rstest]
#[case(WriterFailureMode::Write, "failed writing stderr output")]
#[case(WriterFailureMode::Flush, "failed flushing stderr output")]
fn protocol_proxy_maps_stderr_failures(
    runtime: RuntimeFixture,
    #[case] failure_mode: WriterFailureMode,
    #[case] expected_fragment: &str,
) {
    let output = make_output_stream(vec![Ok(LogOutput::StdErr {
        message: Vec::from(&b"broken"[..]).into(),
    })]);

    let result = run_session(
        runtime,
        b"",
        output,
        Box::pin(RecordingInputWriter::new()),
        RecordingWriter::new(),
        RecordingWriter::with_failure(failure_mode),
    );

    assert_exec_failed_message(result, expected_fragment);
}

#[rstest]
fn protocol_proxy_maps_container_input_flush_failure(runtime: RuntimeFixture) {
    let output = make_output_stream(vec![Ok(LogOutput::StdOut {
        message: Vec::from(&b"out"[..]).into(),
    })]);

    let result = run_session(
        runtime,
        b"stdin",
        output,
        Box::pin(RecordingInputWriter::with_flush_failure()),
        RecordingWriter::new(),
        RecordingWriter::new(),
    );

    assert_exec_failed_message(result, "failed forwarding stdin to exec input");
}

#[rstest]
fn protocol_proxy_maps_daemon_stream_errors(runtime: RuntimeFixture) {
    let output = make_output_stream(vec![Err(BollardError::RequestTimeoutError)]);

    let result = run_session(
        runtime,
        b"",
        output,
        Box::pin(RecordingInputWriter::new()),
        RecordingWriter::new(),
        RecordingWriter::new(),
    );

    assert_exec_failed_message(result, "exec stream failed");
}
