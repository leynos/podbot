//! Given/When steps for interactive execution scenarios.

use bollard::container::LogOutput;
use bollard::errors::Error as BollardError;
use futures_util::stream;
use mockall::mock;
use podbot::engine::{
    ContainerExecClient, CreateExecFuture, EngineConnector, ExecMode, ExecRequest,
    InspectExecFuture, ResizeExecFuture, StartExecFuture,
};
use rstest_bdd_macros::{given, when};

use super::state::{ExecutionOutcome, InteractiveExecState};

pub type StepResult<T> = Result<T, String>;

mock! {
    #[derive(Debug)]
    ExecClient {}

    impl ContainerExecClient for ExecClient {
        fn create_exec(&self, container_id: &str, options: bollard::exec::CreateExecOptions<String>) -> CreateExecFuture<'_>;
        fn start_exec(&self, exec_id: &str, options: Option<bollard::exec::StartExecOptions>) -> StartExecFuture<'_>;
        fn inspect_exec(&self, exec_id: &str) -> InspectExecFuture<'_>;
        fn resize_exec(&self, exec_id: &str, options: bollard::exec::ResizeExecOptions) -> ResizeExecFuture<'_>;
    }
}

#[given("attached execution mode is selected")]
fn attached_execution_mode_selected(interactive_exec_state: &InteractiveExecState) {
    interactive_exec_state.mode.set(ExecMode::Attached);
}

#[given("detached execution mode is selected")]
fn detached_execution_mode_selected(interactive_exec_state: &InteractiveExecState) {
    interactive_exec_state.mode.set(ExecMode::Detached);
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
    configure_create_exec_expectation(&mut client, create_exec_should_fail);
    if !create_exec_should_fail {
        configure_start_exec_expectation(&mut client, mode, tty_enabled);
        configure_resize_expectation(&mut client, mode);
        configure_inspect_expectation(&mut client, omit_exit_code, exit_code);
    }

    let runtime = tokio::runtime::Runtime::new()
        .map_err(|error| format!("failed to create runtime: {error}"))?;
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

fn configure_create_exec_expectation(client: &mut MockExecClient, should_fail: bool) {
    if should_fail {
        client
            .expect_create_exec()
            .times(1)
            .returning(|_, _| Box::pin(async { Err(BollardError::RequestTimeoutError) }));
        return;
    }

    client
        .expect_create_exec()
        .times(1)
        .returning(|container_id, options| {
            assert_eq!(container_id, "bdd-sandbox");
            assert!(options.cmd.is_some(), "command should be forwarded");
            Box::pin(async {
                Ok(bollard::exec::CreateExecResults {
                    id: String::from("bdd-exec-id"),
                })
            })
        });
}

fn configure_start_exec_expectation(
    client: &mut MockExecClient,
    mode: ExecMode,
    tty_enabled: bool,
) {
    match mode {
        ExecMode::Attached => {
            client
                .expect_start_exec()
                .times(1)
                .returning(move |_, options| {
                    assert_eq!(
                        options,
                        Some(bollard::exec::StartExecOptions {
                            detach: false,
                            tty: tty_enabled,
                            output_capacity: None
                        })
                    );
                    let output_stream = stream::iter(vec![Ok(LogOutput::StdOut {
                        message: Vec::from(&b"bdd output"[..]).into(),
                    })]);
                    Box::pin(async move {
                        Ok(bollard::exec::StartExecResults::Attached {
                            output: Box::pin(output_stream),
                            input: Box::pin(tokio::io::sink()),
                        })
                    })
                });
        }
        ExecMode::Detached => {
            client.expect_start_exec().times(1).returning(|_, options| {
                assert_eq!(
                    options,
                    Some(bollard::exec::StartExecOptions {
                        detach: true,
                        tty: false,
                        output_capacity: None
                    })
                );
                Box::pin(async { Ok(bollard::exec::StartExecResults::Detached) })
            });
        }
    }
}

fn configure_resize_expectation(client: &mut MockExecClient, mode: ExecMode) {
    match mode {
        ExecMode::Attached => {
            client
                .expect_resize_exec()
                .times(0..)
                .returning(|_, _| Box::pin(async { Ok(()) }));
        }
        ExecMode::Detached => {
            client.expect_resize_exec().never();
        }
    }
}

fn configure_inspect_expectation(
    client: &mut MockExecClient,
    omit_exit_code: bool,
    exit_code: i64,
) {
    client.expect_inspect_exec().times(1).returning(move |_| {
        let inspect = bollard::models::ExecInspectResponse {
            running: Some(false),
            exit_code: (!omit_exit_code).then_some(exit_code),
            ..bollard::models::ExecInspectResponse::default()
        };
        Box::pin(async move { Ok(inspect) })
    });
}
