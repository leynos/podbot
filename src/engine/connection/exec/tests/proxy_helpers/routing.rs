//! Protocol proxy tests focused on stdout/stderr routing.

use bollard::container::LogOutput;
use rstest::rstest;

use super::*;

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
