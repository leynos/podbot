//! Behavioural tests for protocol exec byte proxying.

use std::io;

use bollard::container::LogOutput;
use bollard::errors::Error as BollardError;
use futures_util::stream;
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::{ScenarioState, given, scenario, then, when};
use tokio::io::{AsyncWriteExt, DuplexStream};

use super::super::protocol::{ProtocolProxyIo, run_protocol_session_with_io_async};
use super::proxy_helpers::{RecordingInputWriter, RecordingWriter, WriterFailureMode};
use super::*;

type StepResult<T> = Result<T, String>;

#[derive(Debug, Clone)]
enum ProtocolProxyOutcome {
    Success,
    Failure(String),
}

#[derive(Debug, Clone)]
enum OutputEvent {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    StreamError,
}

#[derive(Default, ScenarioState)]
struct ProtocolProxyState {
    host_stdin: Slot<Vec<u8>>,
    output_events: Slot<Vec<OutputEvent>>,
    fail_stdout_write: Slot<bool>,
    host_stdout: Slot<Vec<u8>>,
    host_stderr: Slot<Vec<u8>>,
    container_stdin: Slot<Vec<u8>>,
    outcome: Slot<ProtocolProxyOutcome>,
}

#[fixture]
fn protocol_proxy_state() -> ProtocolProxyState {
    let state = ProtocolProxyState::default();
    state.host_stdin.set(Vec::new());
    state.output_events.set(Vec::new());
    state.fail_stdout_write.set(false);
    state
}

async fn build_host_stdin(bytes: &[u8]) -> io::Result<DuplexStream> {
    let capacity = bytes.len().max(1);
    let (mut writer, reader) = tokio::io::duplex(capacity);
    writer.write_all(bytes).await?;
    drop(writer);
    Ok(reader)
}

fn protocol_request() -> Result<ExecRequest, PodbotError> {
    ExecRequest::new(
        "bdd-protocol-sandbox",
        vec![String::from("codex"), String::from("app-server")],
        ExecMode::Protocol,
    )
}

fn normalize_feature_text(text: &str) -> Vec<u8> {
    text.strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(text)
        .as_bytes()
        .to_vec()
}

#[given("host stdin is {text}")]
fn host_stdin_is(protocol_proxy_state: &ProtocolProxyState, text: String) {
    protocol_proxy_state
        .host_stdin
        .set(normalize_feature_text(&text));
}

#[given("container stdout emits {text}")]
fn container_stdout_emits(protocol_proxy_state: &ProtocolProxyState, text: String) {
    let mut events = protocol_proxy_state.output_events.get().unwrap_or_default();
    events.push(OutputEvent::Stdout(normalize_feature_text(&text)));
    protocol_proxy_state.output_events.set(events);
}

#[given("container stderr emits {text}")]
fn container_stderr_emits(protocol_proxy_state: &ProtocolProxyState, text: String) {
    let mut events = protocol_proxy_state.output_events.get().unwrap_or_default();
    events.push(OutputEvent::Stderr(normalize_feature_text(&text)));
    protocol_proxy_state.output_events.set(events);
}

#[given("host stdout write fails")]
fn host_stdout_write_fails(protocol_proxy_state: &ProtocolProxyState) {
    protocol_proxy_state.fail_stdout_write.set(true);
}

#[given("the output stream ends")]
fn the_output_stream_ends(protocol_proxy_state: &ProtocolProxyState) {
    let _ = protocol_proxy_state;
    // No-op marker step; stream ending is implicit when no error is set
}

#[given("the daemon stream fails with an error")]
fn the_daemon_stream_fails(protocol_proxy_state: &ProtocolProxyState) {
    let mut events = protocol_proxy_state.output_events.get().unwrap_or_default();
    events.push(OutputEvent::StreamError);
    protocol_proxy_state.output_events.set(events);
}

fn build_output_stream(events: Vec<OutputEvent>) -> Vec<Result<LogOutput, BollardError>> {
    events
        .into_iter()
        .map(|event| match event {
            OutputEvent::Stdout(bytes) => Ok(LogOutput::StdOut {
                message: bytes.into(),
            }),
            OutputEvent::Stderr(bytes) => Ok(LogOutput::StdErr {
                message: bytes.into(),
            }),
            OutputEvent::StreamError => Err(BollardError::DockerResponseServerError {
                status_code: 500,
                message: String::from("daemon stream error"),
            }),
        })
        .collect()
}

struct CapturedIo {
    stdout: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    stderr: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    stdin: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
}

fn store_proxy_results(
    state: &ProtocolProxyState,
    captured: &CapturedIo,
    result: Result<(), PodbotError>,
) {
    state.host_stdout.set(
        captured
            .stdout
            .lock()
            .expect("writer mutex should not poison")
            .clone(),
    );
    state.host_stderr.set(
        captured
            .stderr
            .lock()
            .expect("writer mutex should not poison")
            .clone(),
    );
    state.container_stdin.set(
        captured
            .stdin
            .lock()
            .expect("writer mutex should not poison")
            .clone(),
    );

    match result {
        Ok(()) => state.outcome.set(ProtocolProxyOutcome::Success),
        Err(error) => state
            .outcome
            .set(ProtocolProxyOutcome::Failure(error.to_string())),
    }
}

#[when("the protocol proxy runs")]
fn the_protocol_proxy_runs(protocol_proxy_state: &ProtocolProxyState) -> StepResult<()> {
    let request = protocol_request().map_err(|error| error.to_string())?;
    let host_stdin_bytes = protocol_proxy_state.host_stdin.get().unwrap_or_default();
    let output_events = protocol_proxy_state.output_events.get().unwrap_or_default();
    let fail_stdout_write = protocol_proxy_state
        .fail_stdout_write
        .get()
        .unwrap_or(false);

    let output_chunks = build_output_stream(output_events);

    let runtime = tokio::runtime::Runtime::new()
        .map_err(|error| format!("failed to create runtime: {error}"))?;
    let host_stdin = runtime
        .block_on(build_host_stdin(&host_stdin_bytes))
        .map_err(|error| format!("failed to build host stdin: {error}"))?;
    let host_stdout = if fail_stdout_write {
        RecordingWriter::with_failure(WriterFailureMode::Write)
    } else {
        RecordingWriter::new()
    };
    let host_stderr = RecordingWriter::new();
    let container_stdin = RecordingInputWriter::new();
    let captured = CapturedIo {
        stdout: host_stdout.bytes.clone(),
        stderr: host_stderr.bytes.clone(),
        stdin: container_stdin.bytes.clone(),
    };

    let result = runtime.block_on(run_protocol_session_with_io_async(
        &request,
        Box::pin(stream::iter(output_chunks)),
        Box::pin(container_stdin),
        ProtocolProxyIo::new(host_stdin, host_stdout, host_stderr),
    ));

    store_proxy_results(protocol_proxy_state, &captured, result);

    Ok(())
}

fn assert_channel_receives(
    expected: &[u8],
    actual: Option<Vec<u8>>,
    channel: &str,
) -> StepResult<()> {
    let recorded = actual.ok_or_else(|| format!("{channel} should be recorded"))?;
    if recorded == expected {
        Ok(())
    } else {
        Err(format!("expected {channel} {expected:?}, got {recorded:?}"))
    }
}

macro_rules! channel_receives_step {
    ($fn_name:ident, $step_text:literal, $field:ident, $label:literal) => {
        #[then($step_text)]
        fn $fn_name(protocol_proxy_state: &ProtocolProxyState, text: String) -> StepResult<()> {
            assert_channel_receives(
                &normalize_feature_text(&text),
                protocol_proxy_state.$field.get(),
                $label,
            )
        }
    };
}

channel_receives_step!(
    host_stdout_receives,
    "host stdout receives {text}",
    host_stdout,
    "host stdout"
);
channel_receives_step!(
    host_stderr_receives,
    "host stderr receives {text}",
    host_stderr,
    "host stderr"
);
channel_receives_step!(
    container_stdin_receives,
    "container stdin receives {text}",
    container_stdin,
    "container stdin"
);

#[then("the protocol proxy fails with an exec error")]
fn the_protocol_proxy_fails(protocol_proxy_state: &ProtocolProxyState) -> StepResult<()> {
    let outcome = protocol_proxy_state
        .outcome
        .get()
        .ok_or_else(|| String::from("proxy outcome should be recorded"))?;

    match outcome {
        ProtocolProxyOutcome::Failure(message)
            if message.contains("failed to execute command in container") =>
        {
            Ok(())
        }
        ProtocolProxyOutcome::Failure(message) => {
            Err(format!("expected exec failure, got: {message}"))
        }
        ProtocolProxyOutcome::Success => Err(String::from("expected proxy failure, got success")),
    }
}

#[then("host stdout concatenates {text1} and {text2}")]
fn host_stdout_concatenates(
    protocol_proxy_state: &ProtocolProxyState,
    text1: String,
    text2: String,
) -> StepResult<()> {
    let mut expected = normalize_feature_text(&text1);
    expected.extend_from_slice(&normalize_feature_text(&text2));
    assert_channel_receives(
        &expected,
        protocol_proxy_state.host_stdout.get(),
        "host stdout",
    )
}

#[then("host stdout contains no prefix or suffix bytes")]
fn host_stdout_contains_no_extra_bytes(
    protocol_proxy_state: &ProtocolProxyState,
) -> StepResult<()> {
    // First, ensure the proxy run itself was successful.
    let outcome = protocol_proxy_state
        .outcome
        .get()
        .ok_or_else(|| String::from("proxy outcome should be recorded"))?;
    if let ProtocolProxyOutcome::Failure(message) = outcome {
        return Err(format!(
            "expected successful proxy run, got failure: {message}"
        ));
    }

    // Then, explicitly assert that host stdout contains exactly the concatenated
    // container stdout events with no prefix or suffix bytes.
    let output_events = protocol_proxy_state.output_events.get().unwrap_or_default();
    let expected: Vec<u8> = output_events
        .into_iter()
        .filter_map(|event| match event {
            OutputEvent::Stdout(bytes) => Some(bytes),
            _ => None,
        })
        .flatten()
        .collect();

    assert_channel_receives(
        &expected,
        protocol_proxy_state.host_stdout.get(),
        "host stdout",
    )
}

#[then("host stdout contains only {text} without error messages")]
fn host_stdout_contains_only_partial_output(
    protocol_proxy_state: &ProtocolProxyState,
    text: String,
) -> StepResult<()> {
    assert_channel_receives(
        &normalize_feature_text(&text),
        protocol_proxy_state.host_stdout.get(),
        "host stdout",
    )
}

#[scenario(
    path = "tests/features/protocol_proxy.feature",
    name = "Protocol proxy writes stdout bytes to host stdout"
)]
fn protocol_proxy_writes_stdout_bytes(protocol_proxy_state: ProtocolProxyState) {
    let _ = protocol_proxy_state;
}

#[scenario(
    path = "tests/features/protocol_proxy.feature",
    name = "Protocol proxy writes stderr bytes to host stderr"
)]
fn protocol_proxy_writes_stderr_bytes(protocol_proxy_state: ProtocolProxyState) {
    let _ = protocol_proxy_state;
}

#[scenario(
    path = "tests/features/protocol_proxy.feature",
    name = "Protocol proxy forwards host stdin to container stdin"
)]
fn protocol_proxy_forwards_host_stdin(protocol_proxy_state: ProtocolProxyState) {
    let _ = protocol_proxy_state;
}

#[scenario(
    path = "tests/features/protocol_proxy.feature",
    name = "Protocol proxy fails when host stdout cannot be written"
)]
fn protocol_proxy_fails_when_host_stdout_breaks(protocol_proxy_state: ProtocolProxyState) {
    let _ = protocol_proxy_state;
}

#[scenario(
    path = "tests/features/protocol_proxy.feature",
    name = "Protocol proxy maintains stream purity through startup to shutdown"
)]
fn protocol_proxy_maintains_lifecycle_purity(protocol_proxy_state: ProtocolProxyState) {
    let _ = protocol_proxy_state;
}

#[scenario(
    path = "tests/features/protocol_proxy.feature",
    name = "Protocol proxy maintains purity when stream errors occur"
)]
fn protocol_proxy_maintains_purity_on_error(protocol_proxy_state: ProtocolProxyState) {
    let _ = protocol_proxy_state;
}
