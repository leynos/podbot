//! Shared protocol proxy test helpers and focused submodules.

mod error_mapping;
mod forwarding;
mod lifecycle_purity;
mod poisoned_capture;
mod routing;

use std::io;
use std::io::Cursor;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use bollard::container::LogOutput;
use bollard::errors::Error as BollardError;
use futures_util::stream;
use tokio::io::AsyncWrite;
use tokio::runtime::Runtime;

use super::super::protocol::{ProtocolProxyIo, run_protocol_session_with_io_async};
use super::*;

/// Builds the error a recording writer reports when its capture lock was
/// poisoned.
///
/// A capture lock is poisoned only by a panic while it was held, and that
/// panic has already failed the test. Reporting it as an error rather than
/// reading through it keeps the test from asserting on half-written bytes.
fn poisoned_capture() -> io::Error {
    io::Error::other("a recording writer's capture lock was poisoned by a panicking holder")
}

/// Returns a copy of the bytes a recording writer captured.
///
/// # Errors
///
/// Returns an error when the capture lock was poisoned.
pub(super) fn captured_bytes(bytes: &Mutex<Vec<u8>>) -> io::Result<Vec<u8>> {
    bytes
        .lock()
        .map(|captured| captured.clone())
        .map_err(|_| poisoned_capture())
}

/// Appends written bytes to a capture, reporting a poisoned lock as an error.
fn record_write(bytes: &Mutex<Vec<u8>>, buf: &[u8]) -> io::Result<usize> {
    let mut captured = bytes.lock().map_err(|_| poisoned_capture())?;
    captured.extend_from_slice(buf);
    Ok(buf.len())
}

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

        Poll::Ready(record_write(&self.bytes, buf))
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
        Poll::Ready(record_write(&self.bytes, buf))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.fail_on_flush {
            return Poll::Ready(Err(io::Error::other("stdin flush failure")));
        }

        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let shutdown = self
            .shutdown_called
            .lock()
            .map(|mut called| *called = true)
            .map_err(|_| poisoned_capture());
        Poll::Ready(shutdown)
    }
}

/// Builds the host stdin reader a protocol session forwards from.
///
/// An in-memory cursor yields the bytes and then end-of-file, which is all
/// the proxy reads. Unlike the duplex pair this replaces, it cannot fail to
/// be set up, so a fallible arrangement step disappears from the helper
/// rather than its panic moving somewhere else.
pub(super) fn make_host_stdin(bytes: &[u8]) -> Cursor<Vec<u8>> {
    Cursor::new(bytes.to_vec())
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
    runtime: &Runtime,
    stdin_bytes: &[u8],
    output: Pin<Box<dyn futures_util::Stream<Item = Result<LogOutput, BollardError>> + Send>>,
    container_input: Pin<Box<dyn AsyncWrite + Send>>,
    host_stdout: RecordingWriter,
    host_stderr: RecordingWriter,
) -> Result<(), PodbotError> {
    let request = make_protocol_request()?;
    let host_stdin = make_host_stdin(stdin_bytes);

    runtime.block_on(run_protocol_session_with_io_async(
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
    runtime: &Runtime,
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

/// Helper for lifecycle purity tests that only need to inspect stdout.
/// Creates recording writers, runs the session, and returns the result
/// with captured stdout bytes.
pub(super) fn run_lifecycle_session(
    runtime: &Runtime,
    stdin_bytes: &[u8],
    output: Pin<Box<dyn futures_util::Stream<Item = Result<LogOutput, BollardError>> + Send>>,
) -> (Result<(), PodbotError>, Arc<Mutex<Vec<u8>>>) {
    let host_stdout = RecordingWriter::new();
    let stdout_bytes = host_stdout.bytes.clone();
    let result = run_session(
        runtime,
        stdin_bytes,
        output,
        Box::pin(RecordingInputWriter::new()),
        host_stdout,
        RecordingWriter::new(),
    );
    (result, stdout_bytes)
}
