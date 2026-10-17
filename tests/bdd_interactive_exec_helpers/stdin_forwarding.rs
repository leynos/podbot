//! Production stdin regressions isolate host input in one exact-test child.
//!
//! This harness belongs only to the interactive-exec regression cases. It uses
//! the existing daemon double and a recording exec-input writer; it must not
//! become a general subprocess runner or a production environment adapter.

use std::io;
use std::pin::Pin;
use std::process::{Output, Stdio};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use bollard::container::LogOutput;
use bollard::errors::Error as BollardError;
use futures_util::stream;
use mockable::Env;
use podbot::engine::{
    ContainerExecClient, CreateExecFuture, EngineConnector, ExecMode, ExecRequest,
    InspectExecFuture, ResizeExecFuture, StartExecFuture,
};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::runtime::Runtime;
use tokio::sync::Notify;

/// Child selection is private to this test executable, never a production knob.
const CHILD_CASE_ENV: &str = "PODBOT_INTERACTIVE_STDIN_CHILD_CASE";
/// The production environment key; only Command.env configures the child value.
const STDIN_SETTING_ENV: &str = "PODBOT_DISABLE_STDIN_FORWARDING_FOR_TESTS";
/// Marker bytes sent through the child's real process stdin, including EOF.
pub(crate) const INPUT_MARKER: &[u8] = b"podbot-production-stdin-marker\n";
/// Bound the exact child, including its runtime startup and captured diagnostics.
const CHILD_TIMEOUT: Duration = Duration::from_secs(20);
/// Bound exec completion even when a regression blocks the forwarding handshake.
const SESSION_TIMEOUT: Duration = Duration::from_secs(5);
/// Allow the protocol's deliberately held-open disabled reader to remain idle.
const OUTPUT_WAIT: Duration = Duration::from_secs(2);

/// Fallible setup result used by the existing daemon-double macro.
type StepResult<T> = Result<T, String>;

crate::define_exec_client_mock! {
    mock_base = StdinExecClient,
    mock_type = MockStdinExecClient,
    exec_mode = ExecMode,
    container_id = "bdd-sandbox",
    exec_id = "bdd-exec-id",
}

/// Distinguish actual engine observations from the enclosing child's outcome.
pub(crate) enum CaseExecution {
    /// The exact child drove the non-test library's host-stdin path.
    Direct {
        /// Bytes actually written to the mocked daemon's exec input.
        bytes: Vec<u8>,
        /// Exit code returned after the real session's shutdown path completed.
        exit_code: i64,
    },
    /// The enclosing ordinary test ran its one selected child to completion.
    Subprocess {
        /// Native exit status and both diagnostic streams from the exact child.
        output: Output,
        /// Full libtest case name used to reject zero-test child runs.
        case_name: String,
    },
}

/// Bytes and completion notification observed at the actual exec-input boundary.
#[derive(Default)]
struct InputCapture {
    /// Raw bytes, independent of stdout and of the caller's expected marker.
    bytes: Mutex<Vec<u8>>,
    /// Released only after the forwarding task shuts down its exec-input writer.
    closed: Notify,
}

/// Record writes made by production forwarding, rather than discarding stdin.
struct RecordingInput(Arc<InputCapture>);

impl AsyncWrite for RecordingInput {
    fn poll_write(
        self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let result = self
            .0
            .bytes
            .lock()
            .map_err(|_| io::Error::other("exec-input capture lock poisoned"))
            .map(|mut captured| {
                captured.extend_from_slice(bytes);
                bytes.len()
            });
        Poll::Ready(result)
    }

    fn poll_flush(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.0.closed.notify_one();
        Poll::Ready(Ok(()))
    }
}

/// Hold daemon output open until forwarded input closes, or disabled input idles.
///
/// An enabled case cannot finish before the real forwarding task completes.
/// Disabled protocol stdin stays open intentionally, so its bounded idle wait
/// ends output and exercises the production settlement timeout instead.
async fn output_after_input(
    capture: Arc<InputCapture>,
    disabled: bool,
) -> Result<LogOutput, BollardError> {
    let settled = tokio::time::timeout(OUTPUT_WAIT, capture.closed.notified()).await;
    if settled.is_err() && !disabled {
        return Err(BollardError::IOError {
            err: io::Error::new(
                io::ErrorKind::TimedOut,
                "enabled stdin never reached exec input EOF",
            ),
        });
    }
    Ok(LogOutput::StdOut {
        message: Vec::<u8>::new().into(),
    })
}

/// Configure the existing mock scaffold with a recording attached input stream.
fn client_for_case(
    mode: ExecMode,
    disabled: bool,
    capture: &Arc<InputCapture>,
) -> StepResult<MockStdinExecClient> {
    let mut client = MockStdinExecClient::new();
    configure_create_exec(&mut client, false);
    configure_resize(&mut client, mode)?;
    configure_inspect(&mut client, Some(0));
    let observed_input = Arc::clone(capture);
    client
        .expect_start_exec()
        .times(1)
        .return_once(move |_, _| {
            let output_capture = Arc::clone(&observed_input);
            Box::pin(async move {
                Ok(bollard::exec::StartExecResults::Attached {
                    output: Box::pin(stream::once(output_after_input(output_capture, disabled))),
                    input: Box::pin(RecordingInput(observed_input)),
                })
            })
        });
    Ok(client)
}

/// Build only the runtime needed by this one case; no ambient stdin is read here.
fn case_runtime() -> StepResult<Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("stdin regression runtime setup failed: {error}"))
}

/// Drive the library from the child's effective setting and retain its observed bytes.
///
/// The enclosing case still checks against its nominal setting, so counter-controls
/// can expose a mismatch between the runtime configuration and expected bytes.
fn run_direct(mode: ExecMode, environment: &impl Env) -> StepResult<CaseExecution> {
    let capture = Arc::new(InputCapture::default());
    let disabled = environment
        .raw(STDIN_SETTING_ENV)
        .is_ok_and(|value| value == "1");
    let client = client_for_case(mode, disabled, &capture)?;
    let request = ExecRequest::new("bdd-sandbox", vec![String::from("cat")], mode)
        .map_err(|error| format!("stdin regression request failed: {error}"))?
        .with_tty(false);
    let runtime = case_runtime()?;
    let result = runtime.block_on(async {
        tokio::time::timeout(
            SESSION_TIMEOUT,
            EngineConnector::exec_async(&client, &request),
        )
        .await
    });
    runtime.shutdown_timeout(Duration::from_secs(1));
    let execution = result
        .map_err(|error| format!("stdin exec session exceeded deadline: {error}"))?
        .map_err(|error| format!("stdin exec session failed: {error}"))?;
    let bytes = capture
        .bytes
        .lock()
        .map_err(|_| String::from("exec-input capture lock poisoned"))?
        .clone();
    Ok(CaseExecution::Direct {
        bytes,
        exit_code: execution.exit_code(),
    })
}

/// Spawn only the current exact libtest case with isolated environment and stdin.
///
/// The child owns its piped stdio. Killing on drop is restricted to this owned
/// child when its deadline or setup fails; shared services are never touched.
async fn run_subprocess(case_name: &str, setting: &str) -> StepResult<Output> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("cannot locate this test executable: {error}"))?;
    let mut command = tokio::process::Command::new(executable);
    command
        .args(["--exact", case_name, "--nocapture", "--test-threads=1"])
        .env(CHILD_CASE_ENV, case_name)
        .env(STDIN_SETTING_ENV, setting)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    tokio::time::timeout(CHILD_TIMEOUT, async {
        let mut child = command
            .spawn()
            .map_err(|error| format!("cannot start stdin case {case_name}: {error}"))?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| format!("stdin case {case_name} has no owned input pipe"))?;
        stdin
            .write_all(INPUT_MARKER)
            .await
            .map_err(|error| format!("cannot feed stdin case {case_name}: {error}"))?;
        stdin
            .shutdown()
            .await
            .map_err(|error| format!("cannot finish stdin case {case_name}: {error}"))?;
        drop(stdin);
        child
            .wait_with_output()
            .await
            .map_err(|error| format!("cannot collect stdin case {case_name}: {error}"))
    })
    .await
    .map_err(|error| {
        format!("stdin child {case_name} setting {setting} exceeded deadline: {error}")
    })?
}

/// Run every normal case as a parent and its exact child, without ignored tests.
///
/// Child selection uses the existing Env reader. Only Command.env configures a
/// child process; neither branch mutates the running process's environment.
/// For example, attached mode with `"0"` observes `INPUT_MARKER` at exec input.
///
/// # Errors
///
/// Returns setup, pipe, child-deadline or exec-session errors with their case
/// context. Native child test failures remain in the returned process output
/// for the recognized test body to inspect.
pub(crate) fn exercise_case(mode: ExecMode, setting: &str) -> StepResult<CaseExecution> {
    let thread = std::thread::current();
    let case_name = thread
        .name()
        .ok_or_else(|| String::from("stdin regression has no libtest case name"))?;
    let env = mockable::DefaultEnv::new();
    if env
        .raw(CHILD_CASE_ENV)
        .is_ok_and(|selected| selected == case_name)
    {
        return run_direct(mode, &env);
    }
    let output = case_runtime()?.block_on(run_subprocess(case_name, setting))?;
    Ok(CaseExecution::Subprocess {
        output,
        case_name: String::from(case_name),
    })
}

/// Prove a forced-disabled protocol child fails at its nominal byte assertion.
#[test]
fn forced_disabled_protocol_case_reaches_capture_assertion() {
    /// Exact existing child case whose nominal expectation requires forwarded bytes.
    const CASE_NAME: &str = "production_stdin_forwarding::case_4_protocol_enabled";

    let runtime = case_runtime().expect("the child runner runtime should initialize");
    let output_result = runtime.block_on(run_subprocess(CASE_NAME, "1"));
    runtime.shutdown_timeout(Duration::from_secs(1));
    let output = output_result.expect("the exact protocol case should finish without setup errors");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(101),
        "the child must exit with the Rust test harness's assertion-failure status"
    );
    assert!(
        stdout.contains(&format!("test {CASE_NAME} ... FAILED")),
        "the exact protocol case must be selected and fail: {stdout}"
    );
    assert!(
        stdout.contains("0 passed; 1 failed; 0 ignored"),
        "the child must run exactly one failing, non-ignored case: {stdout}"
    );
    assert!(
        stderr.contains("capture actual exec-input bytes for Protocol/0"),
        "failure must reach the nominal Protocol/0 capture assertion: {stderr}"
    );
    assert!(
        stderr.contains("left: []"),
        "disabled forwarding must capture no exec-input bytes: {stderr}"
    );
    assert!(
        stderr.contains(&format!("right: {INPUT_MARKER:?}")),
        "the nominal enabled case must still expect the input marker: {stderr}"
    );

    for diagnostic in [
        "enabled stdin never reached exec input EOF",
        "stdin exec session exceeded deadline",
        "stdin exec session failed",
        "stdin regression runtime setup failed",
        "cannot locate this test executable",
        "cannot start stdin case",
        "has no owned input pipe",
        "cannot feed stdin case",
        "cannot finish stdin case",
        "cannot collect stdin case",
        "stdin child",
        "MockStdinExecClient",
        "No matching expectation found",
    ] {
        assert!(
            !stdout.contains(diagnostic) && !stderr.contains(diagnostic),
            "the child must fail at the capture assertion, not {diagnostic:?}:\n{stdout}\n{stderr}"
        );
    }
}
