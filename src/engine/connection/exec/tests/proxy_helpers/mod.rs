//! Shared protocol proxy test helpers and focused submodules.

mod error_mapping;
mod forwarding;
mod lifecycle_purity;
mod routing;

use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use bollard::container::LogOutput;
use bollard::errors::Error as BollardError;
use futures_util::stream;
use tokio::io::{AsyncWrite, AsyncWriteExt, DuplexStream};

use super::super::protocol::{ProtocolProxyIo, run_protocol_session_with_io_async};
use super::*;

#[derive(Clone, Copy)]
pub(super) enum WriterFailureMode {
    Write,
    Flush,
}

pub(super) struct RecordingWriter {
    pub(super) bytes: Arc<Mutex<Vec<u8>>>,
    failure_mode: Option<WriterFailureMode>,
}

impl RecordingWriter {
    pub(super) fn new() -> Self {
        Self {
            bytes: Arc::new(Mutex::new(Vec::new())),
            failure_mode: None,
        }
    }

    pub(super) fn with_failure(failure_mode: WriterFailureMode) -> Self {
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

pub(super) struct RecordingInputWriter {
    pub(super) bytes: Arc<Mutex<Vec<u8>>>,
    pub(super) shutdown_called: Arc<Mutex<bool>>,
    fail_on_flush: bool,
}

impl RecordingInputWriter {
    pub(super) fn new() -> Self {
        Self {
            bytes: Arc::new(Mutex::new(Vec::new())),
            shutdown_called: Arc::new(Mutex::new(false)),
            fail_on_flush: false,
        }
    }

    pub(super) fn with_flush_failure() -> Self {
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

pub(super) async fn make_host_stdin(bytes: &[u8]) -> io::Result<DuplexStream> {
    let capacity = bytes.len().max(1);
    let (mut writer, reader) = tokio::io::duplex(capacity);
    writer.write_all(bytes).await?;
    drop(writer);
    Ok(reader)
}

pub(super) fn make_protocol_request() -> Result<ExecRequest, PodbotError> {
    ExecRequest::new(
        "protocol-sandbox",
        vec![String::from("codex"), String::from("app-server")],
        ExecMode::Protocol,
    )
}

pub(super) fn make_output_stream(
    chunks: Vec<Result<LogOutput, BollardError>>,
) -> Pin<Box<dyn futures_util::Stream<Item = Result<LogOutput, BollardError>> + Send>> {
    Box::pin(stream::iter(chunks))
}

pub(super) fn assert_exec_failed_message(result: Result<(), PodbotError>, expected_fragment: &str) {
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
pub(super) fn run_session(
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
pub(super) fn run_routing_session(
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
