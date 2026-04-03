//! Protocol proxy tests focused on unhappy-path error mapping.

use std::pin::Pin;
use std::task::{Context, Poll};

use bollard::container::LogOutput;
use bollard::errors::Error as BollardError;
use rstest::rstest;
use tokio::io::AsyncRead;

use super::super::super::protocol::{ProtocolProxyIo, run_protocol_session_with_io_async};
use super::*;

struct PendingReader;

impl AsyncRead for PendingReader {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Poll::Pending
    }
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

#[rstest]
fn protocol_proxy_times_out_incomplete_stdin_forwarding(runtime: RuntimeFixture) {
    let runtime_handle = runtime.expect("runtime fixture should initialize");
    let request = make_protocol_request().expect("protocol request should build");
    let result = runtime_handle.block_on(run_protocol_session_with_io_async(
        &request,
        make_output_stream(vec![]),
        Box::pin(RecordingInputWriter::new()),
        ProtocolProxyIo::new(
            PendingReader,
            RecordingWriter::new(),
            RecordingWriter::new(),
        ),
    ));

    assert_exec_failed_message(
        result,
        "stdin forwarding did not complete before protocol session shutdown",
    );
}
