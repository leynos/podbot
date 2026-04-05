//! Protocol proxy tests focused on stdin forwarding behaviour.

use std::future::pending;

use bollard::container::LogOutput;
use rstest::rstest;
use tokio::io::{AsyncWriteExt, duplex};

use super::super::super::protocol::{ProtocolProxyIo, run_protocol_session_with_io_async};
use super::*;

#[rstest]
fn protocol_proxy_forwards_stdin_bytes_and_shuts_down_input(runtime: RuntimeFixture) {
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
fn protocol_proxy_non_eof_stdin_does_not_hang(runtime: RuntimeFixture) {
    let runtime_handle = runtime.expect("runtime fixture should initialize");
    let request = make_protocol_request().expect("protocol request should build");
    let (mut stdin_write, stdin_read) = duplex(1024);
    let background_task = runtime_handle.spawn(async move {
        // `background_task` keeps `stdin_write` alive, and the `write_all`
        // result is deliberately discarded via `drop(...)` so the helper task
        // does not propagate write failures or block the non-EOF test path.
        drop(stdin_write.write_all(b"partial input").await);
        pending::<()>().await;
    });
    let output = make_output_stream(vec![]);
    let result = runtime_handle.block_on(run_protocol_session_with_io_async(
        &request,
        output,
        Box::pin(RecordingInputWriter::new()),
        ProtocolProxyIo::new(stdin_read, RecordingWriter::new(), RecordingWriter::new()),
    ));
    background_task.abort();

    assert_exec_failed_message(
        result,
        "stdin forwarding did not complete before protocol session shutdown",
    );
}
