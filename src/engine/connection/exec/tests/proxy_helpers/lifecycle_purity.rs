//! Lifecycle stream-purity tests covering startup, steady-state, shutdown, and
//! error paths.

use bollard::container::LogOutput;
use bollard::errors::Error as BollardError;
use rstest::rstest;

use super::*;

#[expect(
    clippy::too_many_arguments,
    reason = "test helper needs all parameters to fully specify test case"
)]
fn run_stdout_purity_test(
    runtime: RuntimeFixture,
    output_chunks: Vec<Result<LogOutput, BollardError>>,
    expected_stdout: &[u8],
    success_msg: &str,
    stdout_msg: &str,
) {
    let output = make_output_stream(output_chunks);
    let host_stdout = RecordingWriter::new();
    let captured_stdout = host_stdout.bytes.clone();
    let result = run_session(
        runtime,
        b"",
        output,
        Box::pin(RecordingInputWriter::new()),
        host_stdout,
        RecordingWriter::new(),
    );
    assert!(result.is_ok(), "{success_msg}");
    let captured = captured_stdout.lock().expect("mutex should not poison");
    assert_eq!(captured.as_slice(), expected_stdout, "{stdout_msg}");
}

/// Single-chunk stdout purity: proxy delivers exactly one StdOut chunk's
/// bytes to host stdout. Parametrised across startup and shutdown to
/// confirm no prefix or suffix bytes are injected by session setup or
/// teardown.
#[rstest]
#[case::startup(
    b"STARTUP_OUTPUT" as &[u8],
    "startup should succeed",
    "host stdout must contain exactly the container output with no prefix"
)]
#[case::shutdown(
    b"output-before-shutdown" as &[u8],
    "shutdown should succeed cleanly",
    "host stdout must contain exactly the proxied bytes with no suffix"
)]
fn single_chunk_stdout_purity(
    runtime: RuntimeFixture,
    #[case] payload: &'static [u8],
    #[case] success_msg: &'static str,
    #[case] stdout_msg: &'static str,
) {
    run_stdout_purity_test(
        runtime,
        vec![Ok(LogOutput::StdOut {
            message: payload.to_vec().into(),
        })],
        payload,
        success_msg,
        stdout_msg,
    );
}

/// Lifecycle purity with no stdout: protocol proxy succeeds with empty host
/// stdout when the daemon never emits stdout bytes, ensuring no banner or
/// diagnostic bytes are injected.
#[rstest]
fn lifecycle_purity_no_stdout_bytes(runtime: RuntimeFixture) {
    run_stdout_purity_test(
        runtime,
        Vec::new(),
        b"",
        "session should succeed even with no stdout",
        "host stdout must remain empty when container never writes to stdout",
    );
}

/// Steady-state purity: protocol proxy delivers only container stdout and
/// console bytes, routing stderr separately, and suppressing stdin echoes.
#[rstest]
fn steady_state_purity_mixed_streams(runtime: RuntimeFixture) {
    let output_chunks = vec![
        Ok(LogOutput::StdOut {
            message: b"stdout-1".to_vec().into(),
        }),
        Ok(LogOutput::StdErr {
            message: b"stderr-1".to_vec().into(),
        }),
        Ok(LogOutput::StdIn {
            message: b"stdin-echo".to_vec().into(),
        }),
        Ok(LogOutput::Console {
            message: b"console-1".to_vec().into(),
        }),
        Ok(LogOutput::StdOut {
            message: b"stdout-2".to_vec().into(),
        }),
        Ok(LogOutput::StdErr {
            message: b"stderr-2".to_vec().into(),
        }),
    ];
    let output = make_output_stream(output_chunks);

    let host_stdout = RecordingWriter::new();
    let host_stderr = RecordingWriter::new();
    let captured_stdout = host_stdout.bytes.clone();
    let captured_stderr = host_stderr.bytes.clone();
    let result = run_session(
        runtime,
        b"",
        output,
        Box::pin(RecordingInputWriter::new()),
        host_stdout,
        host_stderr,
    );

    assert!(result.is_ok(), "steady-state should succeed");

    let stdout_data = captured_stdout.lock().expect("mutex should not poison");
    assert_eq!(
        stdout_data.as_slice(),
        b"stdout-1console-1stdout-2",
        "host stdout must contain only stdout and console bytes in order"
    );

    let stderr_data = captured_stderr.lock().expect("mutex should not poison");
    assert_eq!(
        stderr_data.as_slice(),
        b"stderr-1stderr-2",
        "host stderr must contain only stderr bytes in order"
    );
}

/// Error-path purity: protocol proxy fails without contaminating host stdout
/// when the daemon stream errors midway.
#[rstest]
fn error_path_purity_no_error_bytes_to_stdout(runtime: RuntimeFixture) {
    let output_chunks = vec![
        Ok(LogOutput::StdOut {
            message: b"output-before-error".to_vec().into(),
        }),
        Err(BollardError::DockerResponseServerError {
            status_code: 500,
            message: String::from("daemon stream error"),
        }),
    ];
    let output = make_output_stream(output_chunks);

    let host_stdout = RecordingWriter::new();
    let captured_stdout = host_stdout.bytes.clone();
    let result = run_session(
        runtime,
        b"",
        output,
        Box::pin(RecordingInputWriter::new()),
        host_stdout,
        RecordingWriter::new(),
    );

    assert!(
        result.is_err(),
        "error path should surface the daemon stream error"
    );
    assert_exec_failed_message(result, "exec stream failed");

    let captured = captured_stdout.lock().expect("mutex should not poison");
    assert_eq!(
        captured.as_slice(),
        b"output-before-error",
        "host stdout must contain only the bytes from chunks that succeeded"
    );
}

/// Regression test: zero stdout bytes before the first proxied protocol byte
/// and after the final proxied byte. This guards the stdout-purity contract
/// stated in the design document and prevents future code from accidentally
/// adding banners, diagnostics, or framing bytes to the protocol stdout path.
#[rstest]
fn regression_zero_bytes_before_first_and_after_last_proxied_byte(runtime: RuntimeFixture) {
    let known_output = b"PROTOCOL_OUTPUT";
    let output_chunks = vec![Ok(LogOutput::StdOut {
        message: known_output.to_vec().into(),
    })];
    let output = make_output_stream(output_chunks);

    let (result, captured_stdout) = run_lifecycle_session(runtime, b"", output);

    assert!(
        result.is_ok(),
        "regression test session should complete successfully"
    );

    let captured = captured_stdout.lock().expect("mutex should not poison");
    assert_eq!(
        captured.as_slice(),
        known_output,
        "host stdout must contain exactly the known protocol output with zero \
         prefix bytes and zero suffix bytes"
    );

    // Additional verification: the byte length must match exactly
    assert_eq!(
        captured.len(),
        known_output.len(),
        "captured byte count must match the known output byte count exactly, \
         proving no extra bytes were written"
    );
}

/// Regression test: bounded buffering of stdout preserves all bytes across
/// multiple chunks whose total size exceeds the internal buffer size (64 KiB).
/// This specifically exercises the BufReader/BufWriter + copy loop behaviour
/// at and around the buffer boundary to ensure no bytes are lost, duplicated,
/// or reordered when proxying stdout.
#[rstest]
fn regression_stdout_bounded_buffering_preserves_all_bytes(runtime: RuntimeFixture) {
    // Use a total size > 64 KiB to cross the buffer boundary. We pick an odd
    // size to avoid aligning perfectly with any internal buffer sizes.
    const TOTAL_SIZE: usize = 70 * 1024; // 70 KiB
    const CHUNK_SIZE: usize = 8 * 1024 + 123; // ~8 KiB, intentionally non-power-of-two

    let mut expected = Vec::with_capacity(TOTAL_SIZE);
    let mut output_chunks = Vec::new();

    let mut remaining = TOTAL_SIZE;
    let mut byte_value: u8 = 0;

    while remaining > 0 {
        let this_chunk = remaining.min(CHUNK_SIZE);
        let mut chunk = Vec::with_capacity(this_chunk);

        for _ in 0..this_chunk {
            // Deterministic but non-trivial pattern so that reordering or
            // duplication would be visible in the final concat.
            chunk.push(byte_value);
            expected.push(byte_value);
            byte_value = byte_value.wrapping_add(1);
        }

        output_chunks.push(Ok(LogOutput::StdOut {
            message: chunk.into(),
        }));

        remaining -= this_chunk;
    }

    let output = make_output_stream(output_chunks);

    let (result, captured_stdout) = run_lifecycle_session(runtime, b"", output);

    assert!(
        result.is_ok(),
        "bounded buffering test should succeed even with >64 KiB data"
    );

    // The host stdout must exactly equal the concatenation of all stdout chunks,
    // with no extra or missing bytes.
    let captured = captured_stdout.lock().expect("mutex should not poison");
    assert_eq!(
        captured.as_slice(),
        expected.as_slice(),
        "host stdout must be exactly the concatenation of all StdOut chunks \
         even when total size exceeds the bounded buffer size"
    );
}
