//! Unit tests for exec helper functions.

use super::*;
use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use bollard::exec::{CreateExecOptions, StartExecOptions};
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::engine::ExecMode;

struct CreateExecOptionsCase {
    mode: ExecMode,
    tty: bool,
    expected_stdin: Option<bool>,
    expected_stdout: Option<bool>,
    expected_stderr: Option<bool>,
    expected_tty: Option<bool>,
}

#[rstest::rstest]
#[case(CreateExecOptionsCase {
    mode: ExecMode::Attached,
    tty: false,
    expected_stdin: Some(true),
    expected_stdout: Some(true),
    expected_stderr: Some(true),
    expected_tty: Some(false),
})]
#[case(CreateExecOptionsCase {
    mode: ExecMode::Attached,
    tty: true,
    expected_stdin: Some(true),
    expected_stdout: Some(true),
    expected_stderr: Some(true),
    expected_tty: Some(true),
})]
#[case(CreateExecOptionsCase {
    mode: ExecMode::Detached,
    tty: false,
    expected_stdin: Some(false),
    expected_stdout: Some(false),
    expected_stderr: Some(false),
    expected_tty: Some(false),
})]
#[case(CreateExecOptionsCase {
    mode: ExecMode::Detached,
    tty: true,
    expected_stdin: Some(false),
    expected_stdout: Some(false),
    expected_stderr: Some(false),
    expected_tty: Some(false),
})]
#[case(CreateExecOptionsCase {
    mode: ExecMode::Protocol,
    tty: false,
    expected_stdin: Some(true),
    expected_stdout: Some(true),
    expected_stderr: Some(true),
    expected_tty: Some(false),
})]
#[case(CreateExecOptionsCase {
    mode: ExecMode::Protocol,
    tty: true,
    expected_stdin: Some(true),
    expected_stdout: Some(true),
    expected_stderr: Some(true),
    expected_tty: Some(false),
})]
fn build_create_exec_options_maps_mode_tty_env_and_command(#[case] case: CreateExecOptionsCase) {
    let request = ExecRequest::new(
        "sandbox",
        vec![String::from("echo"), String::from("hi")],
        case.mode,
    )
    .expect("request should be valid")
    .with_env(Some(vec![String::from("KEY=value")]))
    .with_tty(case.tty);

    let options: CreateExecOptions<String> = build_create_exec_options(&request);

    assert_eq!(options.attach_stdin, case.expected_stdin);
    assert_eq!(options.attach_stdout, case.expected_stdout);
    assert_eq!(options.attach_stderr, case.expected_stderr);
    assert_eq!(options.tty, case.expected_tty);
    assert_eq!(options.env, request.env().map(<[String]>::to_vec));
    assert_eq!(options.cmd, Some(request.command().to_vec()));
}

struct StartExecOptionsCase {
    mode: ExecMode,
    tty: bool,
    expected_detach: bool,
    expected_tty: bool,
    expected_output_capacity: Option<usize>,
}

#[rstest::rstest]
#[case(StartExecOptionsCase {
    mode: ExecMode::Attached,
    tty: false,
    expected_detach: false,
    expected_tty: false,
    expected_output_capacity: None,
})]
#[case(StartExecOptionsCase {
    mode: ExecMode::Attached,
    tty: true,
    expected_detach: false,
    expected_tty: true,
    expected_output_capacity: None,
})]
#[case(StartExecOptionsCase {
    mode: ExecMode::Detached,
    tty: false,
    expected_detach: true,
    expected_tty: false,
    expected_output_capacity: None,
})]
#[case(StartExecOptionsCase {
    mode: ExecMode::Detached,
    tty: true,
    expected_detach: true,
    expected_tty: false,
    expected_output_capacity: None,
})]
#[case(StartExecOptionsCase {
    mode: ExecMode::Protocol,
    tty: false,
    expected_detach: false,
    expected_tty: false,
    expected_output_capacity: Some(crate::engine::connection::exec::PROTOCOL_OUTPUT_CAPACITY),
})]
#[case(StartExecOptionsCase {
    mode: ExecMode::Protocol,
    tty: true,
    expected_detach: false,
    expected_tty: false,
    expected_output_capacity: Some(crate::engine::connection::exec::PROTOCOL_OUTPUT_CAPACITY),
})]
fn build_start_exec_options_maps_mode_tty_and_output_capacity(#[case] case: StartExecOptionsCase) {
    let request = ExecRequest::new("sandbox", vec![String::from("echo")], case.mode)
        .expect("request should be valid")
        .with_tty(case.tty);

    let options: StartExecOptions = build_start_exec_options(&request);

    assert_eq!(options.detach, case.expected_detach);
    assert_eq!(options.tty, case.expected_tty);
    assert_eq!(options.output_capacity, case.expected_output_capacity);
}

#[rstest::rstest]
#[case::create("create exec failed: boom", map_create_exec_error("sandbox", "boom"))]
#[case::start("start exec failed: boom", map_start_exec_error("sandbox", "boom"))]
fn exec_error_mappers_include_container_id_and_prefix(
    #[case] expected_message: &str,
    #[case] err: crate::error::PodbotError,
) {
    assert!(
        matches!(
            err,
            crate::error::PodbotError::Container(
                crate::error::ContainerError::ExecFailed {
                    ref container_id,
                    ref message,
                }
            ) if container_id == "sandbox"
                && message == expected_message
        ),
        "unexpected error: {err:?}",
    );
}

struct SharedWriter {
    bytes: Arc<Mutex<Vec<u8>>>,
}

impl SharedWriter {
    fn new(bytes: Arc<Mutex<Vec<u8>>>) -> Self {
        Self { bytes }
    }
}

impl AsyncWrite for SharedWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<io::Result<usize>> {
        self.bytes
            .lock()
            .expect("writer buffer mutex should not be poisoned")
            .extend_from_slice(buf);
        std::task::Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn spawn_stdin_forwarding_task_forwards_bytes_and_returns_ok() {
    let (mut stdin_writer, stdin_reader) = tokio::io::duplex(64);
    stdin_writer
        .write_all(b"forwarded bytes")
        .await
        .expect("test stdin should accept bytes");
    stdin_writer
        .shutdown()
        .await
        .expect("stdin shutdown should succeed");

    let captured = Arc::new(Mutex::new(Vec::new()));
    let input: Pin<Box<dyn AsyncWrite + Send>> = Box::pin(SharedWriter::new(Arc::clone(&captured)));

    let handle =
        spawn_stdin_forwarding_task(stdin_reader, input, |mut host_stdin, mut sink| async move {
            let mut bytes = Vec::new();
            host_stdin.read_to_end(&mut bytes).await?;
            sink.write_all(&bytes).await?;
            sink.shutdown().await
        });

    handle
        .await
        .expect("forwarding task should join successfully")
        .expect("forwarding task should return Ok");

    assert_eq!(
        *captured
            .lock()
            .expect("writer buffer mutex should not be poisoned"),
        b"forwarded bytes",
    );
}

fn rejects_as_missing_required_command(err: &crate::error::PodbotError) -> bool {
    matches!(
        err,
        crate::error::PodbotError::Config(
            crate::error::ConfigError::MissingRequired { field }
        ) if field == "command"
    )
}

fn rejects_as_invalid_value_command(err: &crate::error::PodbotError) -> bool {
    matches!(
        err,
        crate::error::PodbotError::Config(
            crate::error::ConfigError::InvalidValue { field, .. }
        ) if field == "command"
    )
}

#[rstest::rstest]
#[case(
    vec![],
    "empty command should be rejected",
    rejects_as_missing_required_command as fn(&crate::error::PodbotError) -> bool,
)]
#[case(
    vec![String::from("  ")],
    "blank executable should be rejected",
    rejects_as_invalid_value_command as fn(&crate::error::PodbotError) -> bool,
)]
fn validate_command_rejects_invalid_input(
    #[case] input: Vec<String>,
    #[case] expectation_message: &str,
    #[case] is_expected_error: fn(&crate::error::PodbotError) -> bool,
) {
    let err = validate_command(input).expect_err(expectation_message);
    assert!(is_expected_error(&err), "unexpected error: {err:?}");
}

#[test]
fn validate_command_accepts_non_blank_executable() {
    let cmd = vec![String::from("echo"), String::from("hello")];
    let result = validate_command(cmd.clone()).expect("valid command should be accepted");
    assert_eq!(result, cmd);
}

#[rstest::rstest]
#[case::empty("")]
#[case::whitespace_only("   ")]
fn validate_required_field_rejects_blank_input(#[case] input: &str) {
    let err =
        validate_required_field("container", input).expect_err("blank field should be rejected");
    assert!(
        matches!(
            err,
            crate::error::PodbotError::Config(
                crate::error::ConfigError::MissingRequired { ref field }
            ) if field == "container"
        ),
        "unexpected error: {err:?}",
    );
}

#[test]
fn validate_required_field_accepts_non_blank_value() {
    let result = validate_required_field("container", "  sandbox  ")
        .expect("valid field should be accepted");
    assert_eq!(result, "sandbox");
}
