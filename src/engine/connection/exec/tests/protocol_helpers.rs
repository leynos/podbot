//! Protocol-mode exec tests verifying tty enforcement and stream behaviour.

use bollard::container::LogOutput;
use bollard::errors::Error as BollardError;
use futures_util::stream;
use rstest::rstest;
use serial_test::serial;

use super::*;

fn make_protocol_exec_request(
    container_id: &str,
    command: Vec<String>,
) -> Result<ExecRequest, PodbotError> {
    ExecRequest::new(container_id, command, ExecMode::Protocol)
}

fn default_protocol_command() -> Vec<String> {
    vec![
        String::from("codex"),
        String::from("app-server"),
        String::from("--listen"),
        String::from("stdio"),
    ]
}

fn assert_non_protocol_modes() {
    assert!(!ExecMode::Attached.is_protocol());
    assert!(!ExecMode::Detached.is_protocol());
}

fn setup_start_exec_protocol(client: &mut MockExecClient, output_messages: Vec<&'static [u8]>) {
    client
        .expect_start_exec()
        .times(1)
        .returning(move |_, options| {
            assert_eq!(
                options,
                Some(StartExecOptions {
                    detach: false,
                    tty: false,
                    output_capacity: Some(65_536)
                })
            );
            let output_chunks = output_messages
                .iter()
                .map(|message| {
                    Ok(LogOutput::StdOut {
                        message: Vec::from(*message).into(),
                    })
                })
                .collect::<Vec<Result<LogOutput, BollardError>>>();
            let output_stream = stream::iter(output_chunks);
            Box::pin(async move {
                Ok(bollard::exec::StartExecResults::Attached {
                    output: Box::pin(output_stream),
                    input: Box::pin(tokio::io::sink()),
                })
            })
        });
}

fn assert_protocol_request_properties(request: &ExecRequest) {
    assert!(!request.tty(), "protocol mode must enforce tty = false");
    assert!(
        request.mode().is_attached(),
        "protocol mode should be treated as attached"
    );
    assert!(
        request.mode().is_protocol(),
        "protocol mode should identify as protocol"
    );
}

fn assert_protocol_create_options(options: &CreateExecOptions<String>) {
    assert_eq!(options.attach_stdin, Some(true), "stdin must be attached");
    assert_eq!(options.attach_stdout, Some(true), "stdout must be attached");
    assert_eq!(options.attach_stderr, Some(true), "stderr must be attached");
    assert_eq!(options.tty, Some(false), "tty must be false");
}

fn assert_protocol_start_options(options: &StartExecOptions) {
    assert!(!options.detach, "detach must be false for protocol mode");
    assert!(!options.tty, "tty must be false for protocol mode");
    assert_eq!(
        options.output_capacity,
        Some(65_536),
        "output_capacity must be set to 64 KiB for protocol mode"
    );
}

fn assert_exit_code(result: Result<ExecResult, PodbotError>, expected: i64, context: &str) {
    let exec_result = result.expect(context);
    assert_eq!(exec_result.exit_code(), expected, "exit code should match");
}

#[rstest]
fn protocol_mode_enforces_tty_false_in_constructor() -> TestResult {
    let request = make_protocol_exec_request("sandbox", default_protocol_command())?;
    assert_protocol_request_properties(&request);
    Ok(())
}

#[rstest]
fn non_protocol_modes_do_not_identify_as_protocol() {
    assert_non_protocol_modes();
}

fn assert_tty_override_rejected(request: &ExecRequest) {
    assert!(
        !request.tty(),
        "with_tty(true) must be rejected for protocol"
    );
}

#[rstest]
fn protocol_mode_rejects_tty_override() -> TestResult {
    let request = make_protocol_exec_request("sandbox", default_protocol_command())?.with_tty(true);
    assert_tty_override_rejected(&request);
    Ok(())
}

#[rstest]
fn protocol_mode_create_options_have_correct_flags() -> TestResult {
    let request = make_protocol_exec_request("sandbox", default_protocol_command())?;
    let options = build_create_exec_options(&request);
    assert_protocol_create_options(&options);
    Ok(())
}

#[rstest]
fn protocol_mode_start_options_have_correct_flags() -> TestResult {
    let request = make_protocol_exec_request("sandbox", default_protocol_command())?;
    let options = build_start_exec_options(&request);
    assert_protocol_start_options(&options);
    Ok(())
}

struct ProtocolExecCase {
    exec_id: &'static str,
    output_messages: Vec<&'static [u8]>,
    inspect_exit_code: i64,
    expected_exit_code: i64,
    context: &'static str,
}

#[rstest]
#[case(ProtocolExecCase {
    exec_id: "proto-exec-1",
    output_messages: vec![&b"protocol-output"[..]],
    inspect_exit_code: 0,
    expected_exit_code: 0,
    context: "protocol exec should succeed",
})]
#[case(ProtocolExecCase {
    exec_id: "proto-exec-2",
    output_messages: vec![],
    inspect_exit_code: 42,
    expected_exit_code: 42,
    context: "protocol exec should return non-zero exit code",
})]
#[serial]
fn protocol_exec_maps_exit_code(
    runtime: RuntimeFixture,
    #[case] case: ProtocolExecCase,
) -> TestResult {
    let runtime_handle = runtime?;
    let mut client = MockExecClient::new();
    setup_create_exec_expectation(&mut client, case.exec_id, false);
    setup_start_exec_protocol(&mut client, case.output_messages);
    client.expect_resize_exec().never();
    setup_inspect_exec_once(&mut client, Some(case.inspect_exit_code));

    let request = make_protocol_exec_request("sandbox-proto", default_protocol_command())?;
    let result = runtime_handle
        .block_on(EngineConnector::exec_async_without_protocol_stdin_forwarding(&client, &request));
    assert_exit_code(result, case.expected_exit_code, case.context);
    Ok(())
}

#[rstest]
fn protocol_mode_rejects_detached_daemon_response(runtime: RuntimeFixture) -> TestResult {
    let runtime_handle = runtime?;
    let mut client = MockExecClient::new();
    setup_create_exec_simple(&mut client, "proto-exec-3");
    setup_start_exec_protocol_detached_response(&mut client);

    let request = make_protocol_exec_request("sandbox-proto", default_protocol_command())?;
    let result = runtime_handle.block_on(EngineConnector::exec_async(&client, &request));
    detached_helpers::assert_exec_failed_with_message(
        result,
        "detached start result",
        "protocol mode should reject detached daemon response",
    );
    Ok(())
}

fn setup_start_exec_protocol_detached_response(client: &mut MockExecClient) {
    client.expect_start_exec().times(1).returning(|_, options| {
        assert_eq!(
            options,
            Some(StartExecOptions {
                detach: false,
                tty: false,
                output_capacity: Some(65_536)
            })
        );
        Box::pin(async { Ok(bollard::exec::StartExecResults::Detached) })
    });
}
