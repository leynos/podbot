//! Shared helpers for exec request validation and Bollard option building.

use std::future::Future;
use std::io;
use std::pin::Pin;

use bollard::exec::{CreateExecOptions, StartExecOptions};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::task::JoinHandle;

use super::{ExecRequest, exec_failed};
use crate::error::{ConfigError, PodbotError};

pub(super) fn build_create_exec_options(request: &ExecRequest) -> CreateExecOptions<String> {
    let attached = request.mode().is_attached();
    CreateExecOptions::<String> {
        attach_stdin: Some(attached),
        attach_stdout: Some(attached),
        attach_stderr: Some(attached),
        tty: Some(attached && request.tty()),
        env: request.env().map(<[String]>::to_vec),
        cmd: Some(request.command().to_vec()),
        ..CreateExecOptions::default()
    }
}

pub(super) const fn build_start_exec_options(request: &ExecRequest) -> StartExecOptions {
    let output_capacity = match request.mode() {
        super::ExecMode::Protocol => Some(super::PROTOCOL_OUTPUT_CAPACITY),
        super::ExecMode::Attached | super::ExecMode::Detached => None,
    };

    StartExecOptions {
        detach: !request.mode().is_attached(),
        tty: request.mode().is_attached() && request.tty(),
        output_capacity,
    }
}

pub(super) fn validate_command(command: Vec<String>) -> Result<Vec<String>, PodbotError> {
    if command.is_empty() {
        return Err(PodbotError::from(ConfigError::MissingRequired {
            field: String::from("command"),
        }));
    }

    let executable = command.first().map(String::as_str).unwrap_or_default();
    if executable.trim().is_empty() {
        return Err(PodbotError::from(ConfigError::InvalidValue {
            field: String::from("command"),
            reason: String::from("command executable must not be empty"),
        }));
    }

    Ok(command)
}

pub(super) fn validate_required_field<'a>(
    field: &str,
    value: &'a str,
) -> Result<&'a str, PodbotError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(PodbotError::from(ConfigError::MissingRequired {
            field: String::from(field),
        }));
    }

    Ok(trimmed)
}

pub(super) fn map_create_exec_error(
    container_id: &str,
    error: impl std::fmt::Display,
) -> PodbotError {
    exec_failed(container_id, format!("create exec failed: {error}"))
}

pub(super) fn map_start_exec_error(
    container_id: &str,
    error: impl std::fmt::Display,
) -> PodbotError {
    exec_failed(container_id, format!("start exec failed: {error}"))
}

pub(super) fn spawn_stdin_forwarding_task<HostStdin, Forward, ForwardFuture>(
    host_stdin: HostStdin,
    input: Pin<Box<dyn AsyncWrite + Send>>,
    forward: Forward,
) -> JoinHandle<io::Result<()>>
where
    HostStdin: AsyncRead + Send + Unpin + 'static,
    Forward: FnOnce(HostStdin, Pin<Box<dyn AsyncWrite + Send>>) -> ForwardFuture + Send + 'static,
    ForwardFuture: Future<Output = io::Result<()>> + Send + 'static,
{
    tokio::spawn(async move { forward(host_stdin, input).await })
}

#[cfg(test)]
mod tests {
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
    fn build_create_exec_options_maps_mode_tty_env_and_command(
        #[case] case: CreateExecOptionsCase,
    ) {
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
    fn build_start_exec_options_maps_mode_tty_and_output_capacity(
        #[case] case: StartExecOptionsCase,
    ) {
        let request = ExecRequest::new("sandbox", vec![String::from("echo")], case.mode)
            .expect("request should be valid")
            .with_tty(case.tty);

        let options: StartExecOptions = build_start_exec_options(&request);

        assert_eq!(options.detach, case.expected_detach);
        assert_eq!(options.tty, case.expected_tty);
        assert_eq!(options.output_capacity, case.expected_output_capacity);
    }

    #[test]
    fn map_create_exec_error_includes_container_id_and_context() {
        let err = map_create_exec_error("sandbox", "boom");
        assert!(
            matches!(
                err,
                crate::error::PodbotError::Container(
                    crate::error::ContainerError::ExecFailed {
                        ref container_id,
                        ref message,
                    }
                ) if container_id == "sandbox"
                    && message == "create exec failed: boom"
            ),
            "unexpected error: {err:?}",
        );
    }

    #[test]
    fn map_start_exec_error_includes_container_id_and_context() {
        let err = map_start_exec_error("sandbox", "boom");
        assert!(
            matches!(
                err,
                crate::error::PodbotError::Container(
                    crate::error::ContainerError::ExecFailed {
                        ref container_id,
                        ref message,
                    }
                ) if container_id == "sandbox"
                    && message == "start exec failed: boom"
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
        let input: Pin<Box<dyn AsyncWrite + Send>> =
            Box::pin(SharedWriter::new(Arc::clone(&captured)));

        let handle = spawn_stdin_forwarding_task(
            stdin_reader,
            input,
            |mut host_stdin, mut sink| async move {
                let mut bytes = Vec::new();
                host_stdin.read_to_end(&mut bytes).await?;
                sink.write_all(&bytes).await?;
                sink.shutdown().await
            },
        );

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

    #[test]
    fn validate_command_rejects_empty_vec() {
        let err = validate_command(vec![]).expect_err("empty command should be rejected");
        assert!(
            matches!(
                err,
                crate::error::PodbotError::Config(
                    crate::error::ConfigError::MissingRequired { ref field }
                ) if field == "command"
            ),
            "unexpected error: {err:?}",
        );
    }

    #[test]
    fn validate_command_rejects_blank_executable() {
        let err = validate_command(vec![String::from("  ")])
            .expect_err("blank executable should be rejected");
        assert!(
            matches!(
                err,
                crate::error::PodbotError::Config(
                    crate::error::ConfigError::InvalidValue { ref field, .. }
                ) if field == "command"
            ),
            "unexpected error: {err:?}",
        );
    }

    #[test]
    fn validate_command_accepts_non_blank_executable() {
        let cmd = vec![String::from("echo"), String::from("hello")];
        let result = validate_command(cmd.clone()).expect("valid command should be accepted");
        assert_eq!(result, cmd);
    }

    #[test]
    fn validate_required_field_rejects_empty_string() {
        let err =
            validate_required_field("container", "").expect_err("empty field should be rejected");
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
    fn validate_required_field_rejects_whitespace_only() {
        let err = validate_required_field("container", "   ")
            .expect_err("whitespace-only field should be rejected");
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
}
