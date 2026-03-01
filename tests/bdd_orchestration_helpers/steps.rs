//! Given/when/then steps for orchestration scenarios.

use bollard::container::LogOutput;
use futures_util::stream;
use mockall::mock;
use podbot::api::{
    CommandOutcome, ExecParams, exec, list_containers, run_agent, run_token_daemon, stop_container,
};
use podbot::config::AppConfig;
use podbot::engine::{
    ContainerExecClient, CreateExecFuture, ExecMode, InspectExecFuture, ResizeExecFuture,
    StartExecFuture,
};
use rstest_bdd_macros::{given, when};

use super::StepResult;
use super::state::{OrchestrationResult, OrchestrationState};

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

mock! {
    #[derive(Debug)]
    OrcExecClient {}

    impl ContainerExecClient for OrcExecClient {
        fn create_exec(&self, container_id: &str, options: bollard::exec::CreateExecOptions<String>) -> CreateExecFuture<'_>;
        fn start_exec(&self, exec_id: &str, options: Option<bollard::exec::StartExecOptions>) -> StartExecFuture<'_>;
        fn inspect_exec(&self, exec_id: &str) -> InspectExecFuture<'_>;
        fn resize_exec(&self, exec_id: &str, options: bollard::exec::ResizeExecOptions) -> ResizeExecFuture<'_>;
    }
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
    configure_create_exec(&mut client);
    configure_start_exec(&mut client, mode);
    configure_resize(&mut client, mode);
    configure_inspect(&mut client, exit_code);

    let runtime =
        tokio::runtime::Runtime::new().map_err(|e| format!("failed to create runtime: {e}"))?;

    invoke_orchestration(orchestration_state, || {
        exec(ExecParams {
            connector: &client,
            container: "orc-sandbox",
            command: command.clone(),
            mode,
            tty,
            runtime_handle: runtime.handle(),
        })
    });
    Ok(())
}

#[when("run orchestration is invoked")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
fn when_run_invoked(orchestration_state: &OrchestrationState) -> StepResult<()> {
    let config = AppConfig::default();
    invoke_orchestration(orchestration_state, || run_agent(&config));
    Ok(())
}

#[when("stop orchestration is invoked with container {container}")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
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
fn when_list_containers_invoked(orchestration_state: &OrchestrationState) -> StepResult<()> {
    invoke_orchestration(orchestration_state, list_containers);
    Ok(())
}

#[when("token daemon orchestration is invoked with container {container}")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
fn when_token_daemon_invoked(
    orchestration_state: &OrchestrationState,
    container: String,
) -> StepResult<()> {
    invoke_orchestration(orchestration_state, || run_token_daemon(&container));
    Ok(())
}

fn configure_create_exec(client: &mut MockOrcExecClient) {
    client.expect_create_exec().times(1).returning(|_, _| {
        Box::pin(async {
            Ok(bollard::exec::CreateExecResults {
                id: String::from("orc-exec-id"),
            })
        })
    });
}

fn configure_start_exec(client: &mut MockOrcExecClient, mode: ExecMode) {
    match mode {
        ExecMode::Attached => {
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
        }
        ExecMode::Detached => {
            client.expect_start_exec().times(1).returning(|_, _| {
                Box::pin(async { Ok(bollard::exec::StartExecResults::Detached) })
            });
        }
    }
}

fn configure_resize(client: &mut MockOrcExecClient, mode: ExecMode) {
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

fn configure_inspect(client: &mut MockOrcExecClient, exit_code: i64) {
    client.expect_inspect_exec().times(1).returning(move |_| {
        let inspect = bollard::models::ExecInspectResponse {
            running: Some(false),
            exit_code: Some(exit_code),
            ..bollard::models::ExecInspectResponse::default()
        };
        Box::pin(async move { Ok(inspect) })
    });
}
