//! Behavioural tests for protocol exec byte proxying.

use std::io;

use bollard::container::LogOutput;
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

#[derive(Default, ScenarioState)]
struct ProtocolProxyState {
    host_stdin: Slot<Vec<u8>>,
    stdout_chunks: Slot<Vec<Vec<u8>>>,
    stderr_chunks: Slot<Vec<Vec<u8>>>,
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
    state.stdout_chunks.set(Vec::new());
    state.stderr_chunks.set(Vec::new());
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
    let mut chunks = protocol_proxy_state.stdout_chunks.get().unwrap_or_default();
    chunks.push(normalize_feature_text(&text));
    protocol_proxy_state.stdout_chunks.set(chunks);
}

#[given("container stderr emits {text}")]
fn container_stderr_emits(protocol_proxy_state: &ProtocolProxyState, text: String) {
    let mut chunks = protocol_proxy_state.stderr_chunks.get().unwrap_or_default();
    chunks.push(normalize_feature_text(&text));
    protocol_proxy_state.stderr_chunks.set(chunks);
}

#[given("host stdout write fails")]
fn host_stdout_write_fails(protocol_proxy_state: &ProtocolProxyState) {
    protocol_proxy_state.fail_stdout_write.set(true);
}

#[when("the protocol proxy runs")]
fn the_protocol_proxy_runs(protocol_proxy_state: &ProtocolProxyState) -> StepResult<()> {
    let request = protocol_request().map_err(|error| error.to_string())?;
    let host_stdin_bytes = protocol_proxy_state.host_stdin.get().unwrap_or_default();
    let stdout_chunks = protocol_proxy_state.stdout_chunks.get().unwrap_or_default();
    let stderr_chunks = protocol_proxy_state.stderr_chunks.get().unwrap_or_default();
    let fail_stdout_write = protocol_proxy_state
        .fail_stdout_write
        .get()
        .unwrap_or(false);

    let mut output_chunks = Vec::new();
    for chunk in stdout_chunks {
        output_chunks.push(Ok(LogOutput::StdOut {
            message: chunk.into(),
        }));
    }
    for chunk in stderr_chunks {
        output_chunks.push(Ok(LogOutput::StdErr {
            message: chunk.into(),
        }));
    }

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
    let captured_stdout = host_stdout.bytes.clone();
    let captured_stderr = host_stderr.bytes.clone();
    let captured_stdin = container_stdin.bytes.clone();

    let result = runtime.block_on(run_protocol_session_with_io_async(
        &request,
        Box::pin(stream::iter(output_chunks)),
        Box::pin(container_stdin),
        ProtocolProxyIo::new(host_stdin, host_stdout, host_stderr),
    ));

    protocol_proxy_state.host_stdout.set(
        captured_stdout
            .lock()
            .expect("writer mutex should not poison")
            .clone(),
    );
    protocol_proxy_state.host_stderr.set(
        captured_stderr
            .lock()
            .expect("writer mutex should not poison")
            .clone(),
    );
    protocol_proxy_state.container_stdin.set(
        captured_stdin
            .lock()
            .expect("writer mutex should not poison")
            .clone(),
    );

    match result {
        Ok(()) => protocol_proxy_state
            .outcome
            .set(ProtocolProxyOutcome::Success),
        Err(error) => protocol_proxy_state
            .outcome
            .set(ProtocolProxyOutcome::Failure(error.to_string())),
    }

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
