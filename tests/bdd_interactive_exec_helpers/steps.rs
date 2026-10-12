//! Given/When steps for interactive execution scenarios.

use bollard::container::LogOutput;
use futures_util::stream;
use podbot::engine::{
    ContainerExecClient, CreateExecFuture, EngineConnector, ExecMode, ExecRequest,
    InspectExecFuture, ResizeExecFuture, StartExecFuture,
};
use rstest_bdd_macros::{given, when};

use super::state::{ExecutionOutcome, InteractiveExecState};
use crate::test_utils::TestStdinForwardingGuard;

pub type StepResult<T> = Result<T, String>;

// The double and the create-exec, resize, and inspect expectations are shared
// with the orchestration suite; only the start-exec expectations below differ.
crate::define_exec_client_mock! {
    mock_base = ExecClient,
    mock_type = MockExecClient,
    exec_mode = ExecMode,
    container_id = "bdd-sandbox",
    exec_id = "bdd-exec-id",
}

#[given("attached execution mode is selected")]
fn attached_execution_mode_selected(interactive_exec_state: &InteractiveExecState) {
    interactive_exec_state.mode.set(ExecMode::Attached);
}

#[given("detached execution mode is selected")]
fn detached_execution_mode_selected(interactive_exec_state: &InteractiveExecState) {
    interactive_exec_state.mode.set(ExecMode::Detached);
}

#[given("protocol execution mode is selected")]
fn protocol_execution_mode_selected(interactive_exec_state: &InteractiveExecState) {
    interactive_exec_state.mode.set(ExecMode::Protocol);
}

#[given("tty allocation is enabled")]
fn tty_allocation_enabled(interactive_exec_state: &InteractiveExecState) {
    interactive_exec_state.tty_enabled.set(true);
}

#[given("tty allocation is disabled")]
fn tty_allocation_disabled(interactive_exec_state: &InteractiveExecState) {
    interactive_exec_state.tty_enabled.set(false);
}

#[given("command is {command}")]
fn command_is(interactive_exec_state: &InteractiveExecState, command: String) {
    let command_parts: Vec<String> = command.split_whitespace().map(String::from).collect();
    interactive_exec_state.command.set(command_parts);
}

#[given("command exit code is {code}")]
fn command_exit_code_is(interactive_exec_state: &InteractiveExecState, code: i64) {
    interactive_exec_state.exit_code.set(code);
}

#[given("daemon create-exec call fails")]
fn daemon_create_exec_call_fails(interactive_exec_state: &InteractiveExecState) {
    interactive_exec_state.create_exec_should_fail.set(true);
}

#[given("daemon omits exit code from inspect response")]
fn daemon_omits_exit_code(interactive_exec_state: &InteractiveExecState) {
    interactive_exec_state.omit_exit_code.set(true);
}

#[when("execution is requested")]
fn execution_is_requested(interactive_exec_state: &InteractiveExecState) -> StepResult<()> {
    let mode = interactive_exec_state
        .mode
        .get()
        .ok_or_else(|| String::from("mode should be configured"))?;
    let tty_enabled = interactive_exec_state.tty_enabled.get().unwrap_or(true);
    let command = interactive_exec_state
        .command
        .get()
        .ok_or_else(|| String::from("command should be configured"))?;
    let create_exec_should_fail = interactive_exec_state
        .create_exec_should_fail
        .get()
        .unwrap_or(false);
    let omit_exit_code = interactive_exec_state.omit_exit_code.get().unwrap_or(false);
    let exit_code = interactive_exec_state.exit_code.get().unwrap_or(0);

    let request = ExecRequest::new("bdd-sandbox", command, mode)
        .map_err(|error| format!("failed to build request: {error}"))?
        .with_tty(tty_enabled);

    let mut client = MockExecClient::new();
    configure_create_exec(&mut client, create_exec_should_fail);
    if !create_exec_should_fail {
        configure_start_exec(&mut client, mode, tty_enabled)?;
        configure_resize(&mut client, mode)?;
        configure_inspect(&mut client, (!omit_exit_code).then_some(exit_code));
    }

    let runtime = tokio::runtime::Runtime::new()
        .map_err(|error| format!("failed to create runtime: {error}"))?;
    let _stdin_forwarding_guard = TestStdinForwardingGuard::disable();
    let execution_result = runtime.block_on(EngineConnector::exec_async(&client, &request));

    match execution_result {
        Ok(result) => interactive_exec_state
            .outcome
            .set(ExecutionOutcome::Success {
                exit_code: result.exit_code(),
            }),
        Err(error) => interactive_exec_state
            .outcome
            .set(ExecutionOutcome::Failure {
                message: error.to_string(),
            }),
    }

    Ok(())
}

/// Returns the exact start-exec options this suite expects for `mode`.
///
/// Pure query so the expectation builder stays small; unknown modes surface
/// as step errors rather than panics.
fn expected_start_options(
    mode: ExecMode,
    tty_enabled: bool,
) -> StepResult<bollard::exec::StartExecOptions> {
    match mode {
        ExecMode::Attached => Ok(bollard::exec::StartExecOptions {
            detach: false,
            tty: tty_enabled,
            output_capacity: None,
        }),
        ExecMode::Protocol => Ok(bollard::exec::StartExecOptions {
            detach: false,
            tty: false,
            output_capacity: Some(65_536),
        }),
        ExecMode::Detached => Ok(bollard::exec::StartExecOptions {
            detach: true,
            tty: false,
            output_capacity: None,
        }),
        other => Err(format!(
            "unexpected exec mode in start-exec expectation: {other:?}"
        )),
    }
}

/// Builds the canned attached start-exec response used by these scenarios.
fn attached_start_results() -> bollard::exec::StartExecResults {
    let output_stream = stream::iter(vec![Ok(LogOutput::StdOut {
        message: Vec::from(&b"bdd output"[..]).into(),
    })]);
    bollard::exec::StartExecResults::Attached {
        output: Box::pin(output_stream),
        input: Box::pin(tokio::io::sink()),
    }
}

/// Expects the start-exec options this suite requires for `mode`.
///
/// Unlike the orchestration suite, these scenarios assert the exact options
/// handed to the daemon, so the builder stays local to this module. `withf`
/// pins the options; a mismatch fails the test via mockall's
/// unmatched-expectation report rather than a panic here.
fn configure_start_exec(
    client: &mut MockExecClient,
    mode: ExecMode,
    tty_enabled: bool,
) -> StepResult<()> {
    let expected = expected_start_options(mode, tty_enabled)?;
    let is_detached = matches!(mode, ExecMode::Detached);
    client
        .expect_start_exec()
        .times(1)
        .withf(move |_, options| *options == Some(expected))
        .returning(move |_, _| {
            Box::pin(async move {
                Ok(if is_detached {
                    bollard::exec::StartExecResults::Detached
                } else {
                    attached_start_results()
                })
            })
        });
    Ok(())
}
