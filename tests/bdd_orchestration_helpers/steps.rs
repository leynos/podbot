//! Given/when/then steps for orchestration scenarios.
//!
//! These step definitions exercise the library orchestration boundary used by
//! the behavioural `bdd_orchestration` feature. They translate feature-file
//! preconditions into [`OrchestrationState`], configure mock container-engine
//! clients, and invoke the public orchestration APIs without depending on the
//! CLI adapter.
//!
//! The module works with `state` to persist scenario inputs and outcomes, and
//! with `crate::test_utils` to share deterministic exec-client helpers. This
//! keeps BDD scenarios focused on command behaviour while the mock utilities
//! provide repeatable engine responses.

use bollard::container::LogOutput;
use futures_util::stream;
use podbot::api::{CommandOutcome, ExecMode, ExecRequest};
#[cfg(feature = "experimental")]
use podbot::api::{RunRequest, list_containers, run_agent, run_token_daemon, stop_container};
#[cfg(feature = "experimental")]
use podbot::config::AppConfig;
use podbot::engine::{
    ContainerExecClient, CreateExecFuture, InspectExecFuture, ResizeExecFuture, StartExecFuture,
};
use rstest_bdd_macros::{given, when};

use super::StepResult;
use super::state::{OrchestrationResult, OrchestrationState};
use crate::test_utils::{TestStdinForwardingGuard, exec_outcome_with_client};

/// Invoke an orchestration operation and capture its outcome in state.
fn invoke_orchestration<F>(orchestration_state: &OrchestrationState, operation: F)
where
    F: FnOnce() -> podbot::error::Result<CommandOutcome>,
{
    match operation() {
        Ok(outcome) => orchestration_state
            .result
            .set(OrchestrationResult::Ok(outcome)),
        Err(e) => orchestration_state
            .result
            .set(OrchestrationResult::Err(e.to_string())),
    }
}

// The double and the create-exec, resize, and inspect expectations are shared
// with the interactive-exec suite; only the start-exec expectations below
// differ, because these scenarios do not pin the daemon options.
crate::define_exec_client_mock! {
    mock_base = OrcExecClient,
    mock_type = MockOrcExecClient,
    exec_mode = ExecMode,
    container_id = "orc-sandbox",
    exec_id = "orc-exec-id",
}

#[given("a mock container engine")]
fn given_mock_engine(orchestration_state: &OrchestrationState) {
    // State defaults already configure a working mock scenario.
    let _ = orchestration_state;
}

#[given("exec mode is attached")]
fn given_exec_mode_attached(orchestration_state: &OrchestrationState) {
    orchestration_state.mode.set(ExecMode::Attached);
}

#[given("exec mode is detached")]
fn given_exec_mode_detached(orchestration_state: &OrchestrationState) {
    orchestration_state.mode.set(ExecMode::Detached);
}

#[given("tty is enabled")]
fn given_tty_enabled(orchestration_state: &OrchestrationState) {
    orchestration_state.tty.set(true);
}

#[given("the command is {command}")]
fn given_command(orchestration_state: &OrchestrationState, command: String) {
    let parts: Vec<String> = command.split_whitespace().map(String::from).collect();
    orchestration_state.command.set(parts);
}

#[given("the daemon reports exit code {code}")]
fn given_daemon_exit_code(orchestration_state: &OrchestrationState, code: i64) {
    orchestration_state.exit_code.set(code);
}

#[when("exec orchestration is invoked")]
fn when_exec_orchestration_invoked(orchestration_state: &OrchestrationState) -> StepResult<()> {
    let mode = orchestration_state
        .mode
        .get()
        .ok_or_else(|| String::from("mode should be configured"))?;
    let tty = orchestration_state.tty.get().unwrap_or(false);
    let command = orchestration_state
        .command
        .get()
        .ok_or_else(|| String::from("command should be configured"))?;
    let exit_code = orchestration_state.exit_code.get().unwrap_or(0);

    let mut client = MockOrcExecClient::new();
    configure_create_exec(&mut client, false);
    configure_start_exec(&mut client, mode)?;
    configure_resize(&mut client, mode)?;
    configure_inspect(&mut client, Some(exit_code));

    let _stdin_forwarding_guard = TestStdinForwardingGuard::disable();
    let runtime =
        tokio::runtime::Runtime::new().map_err(|e| format!("failed to create runtime: {e}"))?;

    invoke_orchestration(orchestration_state, || {
        let request = ExecRequest::new("orc-sandbox", command.clone())?
            .with_mode(mode)
            .with_tty(tty);

        exec_outcome_with_client(&client, runtime.handle(), &request)
    });
    Ok(())
}

#[when("run orchestration is invoked")]
#[cfg(feature = "experimental")]
fn when_run_invoked(orchestration_state: &OrchestrationState) -> StepResult<()> {
    let config = AppConfig::default();
    let request = RunRequest::new("owner/name", "main").map_err(|e| e.to_string())?;
    invoke_orchestration(orchestration_state, || run_agent(&config, &request));
    Ok(())
}

#[when("stop orchestration is invoked with container {container}")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
#[cfg(feature = "experimental")]
fn when_stop_invoked(
    orchestration_state: &OrchestrationState,
    container: String,
) -> StepResult<()> {
    invoke_orchestration(orchestration_state, || stop_container(&container));
    Ok(())
}

#[when("list containers orchestration is invoked")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
#[cfg(feature = "experimental")]
fn when_list_containers_invoked(orchestration_state: &OrchestrationState) -> StepResult<()> {
    invoke_orchestration(orchestration_state, list_containers);
    Ok(())
}

#[when("token daemon orchestration is invoked with container {container}")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
#[cfg(feature = "experimental")]
fn when_token_daemon_invoked(
    orchestration_state: &OrchestrationState,
    container: String,
) -> StepResult<()> {
    invoke_orchestration(orchestration_state, || run_token_daemon(&container));
    Ok(())
}

/// Expects a single start-exec call appropriate to `mode`.
///
/// These scenarios assert orchestration outcomes rather than daemon options,
/// so the builder stays local to this module and accepts any options.
fn configure_start_exec(client: &mut MockOrcExecClient, mode: ExecMode) -> StepResult<()> {
    match mode {
        ExecMode::Attached | ExecMode::Protocol => {
            client.expect_start_exec().times(1).returning(move |_, _| {
                let output_stream = stream::iter(vec![Ok(LogOutput::StdOut {
                    message: Vec::from(&b"orc output"[..]).into(),
                })]);
                Box::pin(async move {
                    Ok(bollard::exec::StartExecResults::Attached {
                        output: Box::pin(output_stream),
                        input: Box::pin(tokio::io::sink()),
                    })
                })
            });
            Ok(())
        }
        ExecMode::Detached => {
            client.expect_start_exec().times(1).returning(|_, _| {
                Box::pin(async { Ok(bollard::exec::StartExecResults::Detached) })
            });
            Ok(())
        }
        other => Err(format!(
            "unexpected exec mode in start-exec expectation: {other:?}"
        )),
    }
}
