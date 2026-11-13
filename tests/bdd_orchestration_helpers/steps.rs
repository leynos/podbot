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

use std::collections::HashMap;

use bollard::container::LogOutput;
use bollard::models::{
    ContainerConfig, ContainerInspectResponse, ContainerState, ContainerSummary as EngineSummary,
    ContainerSummaryStateEnum,
};
use futures_util::stream;
use mockall::mock;
use podbot::api::{
    CommandOutcome, ExecMode, ExecRequest, list_containers_with_client, stop_container_with_client,
};
#[cfg(feature = "experimental")]
use podbot::api::{DaemonShutdown, RunRequest, run_agent, run_token_daemon};
#[cfg(feature = "experimental")]
use podbot::config::AppConfig;
use podbot::engine::{
    ContainerExecClient, ContainerLifecycleClient, CreateExecFuture, InspectContainerFuture,
    InspectExecFuture, ListContainersFuture, MANAGED_LABEL_KEY, MANAGED_LABEL_VALUE,
    RemoveContainerFuture, ResizeExecFuture, StartContainerFuture, StartExecFuture,
    StopContainerFuture,
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

mock! {
    LifecycleClient {}
    impl ContainerLifecycleClient for LifecycleClient {
        fn start_container(&self, container_id: &str) -> StartContainerFuture<'_>;
        fn stop_container(
            &self,
            container_id: &str,
            grace_seconds: i32,
        ) -> StopContainerFuture<'_>;
        fn remove_container(
            &self,
            container_id: &str,
            force: bool,
        ) -> RemoveContainerFuture<'_>;
        fn inspect_container(
            &self,
            container_id: &str,
        ) -> InspectContainerFuture<'_>;
        fn list_containers(&self, label_filter: &str) -> ListContainersFuture<'_>;
    }
}

/// Build an inspect response carrying the requested state and labels.
fn inspect_response(managed: bool, running: bool) -> ContainerInspectResponse {
    let labels = if managed {
        HashMap::from([(
            String::from(MANAGED_LABEL_KEY),
            String::from(MANAGED_LABEL_VALUE),
        )])
    } else {
        HashMap::new()
    };

    ContainerInspectResponse {
        id: Some(String::from("test-ctr")),
        state: Some(ContainerState {
            running: Some(running),
            ..ContainerState::default()
        }),
        config: Some(ContainerConfig {
            labels: Some(labels),
            ..ContainerConfig::default()
        }),
        ..ContainerInspectResponse::default()
    }
}

/// Build one engine summary for a managed container.
fn engine_summary(index: usize) -> EngineSummary {
    EngineSummary {
        id: Some(format!("ctr-{index}")),
        names: Some(vec![format!("/podbot-{index}")]),
        image: Some(String::from("podbot-sandbox:latest")),
        state: Some(ContainerSummaryStateEnum::RUNNING),
        status: Some(String::from("Up 1 minute")),
        created: Some(i64::try_from(index).unwrap_or_default()),
        labels: Some(HashMap::from([(
            String::from(MANAGED_LABEL_KEY),
            String::from(MANAGED_LABEL_VALUE),
        )])),
        ..EngineSummary::default()
    }
}

#[given("a mock container engine")]
fn given_mock_engine(orchestration_state: &OrchestrationState) {
    // State defaults already configure a working mock scenario.
    let _ = orchestration_state;
}

#[given("a mock lifecycle client with a running managed container")]
fn given_running_managed_container(orchestration_state: &OrchestrationState) {
    orchestration_state.managed.set(true);
    orchestration_state.running.set(true);
}

#[given("a mock lifecycle client with a running unmanaged container")]
fn given_running_unmanaged_container(orchestration_state: &OrchestrationState) {
    orchestration_state.managed.set(false);
    orchestration_state.running.set(true);
}

#[given("a mock lifecycle client listing {count} managed containers")]
fn given_listed_containers(orchestration_state: &OrchestrationState, count: usize) {
    orchestration_state.list_count.set(count);
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
fn when_stop_invoked(
    orchestration_state: &OrchestrationState,
    container: String,
) -> StepResult<()> {
    let managed = orchestration_state.managed.get().unwrap_or(true);
    let running = orchestration_state.running.get().unwrap_or(true);
    let mut client = MockLifecycleClient::new();
    client
        .expect_inspect_container()
        .times(1)
        .returning(move |_| {
            let response = inspect_response(managed, running);
            Box::pin(async move { Ok(response) })
        });
    if managed && running {
        client
            .expect_stop_container()
            .times(1)
            .returning(|_, _| Box::pin(async { Ok(()) }));
    } else {
        client.expect_stop_container().times(0);
    }

    let runtime = test_runtime()?;
    invoke_orchestration(orchestration_state, || {
        stop_container_with_client(&client, runtime.handle(), &container)
    });
    Ok(())
}

#[when("list containers orchestration is invoked")]
fn when_list_containers_invoked(orchestration_state: &OrchestrationState) -> StepResult<()> {
    let count = orchestration_state.list_count.get().unwrap_or(0);
    let mut client = MockLifecycleClient::new();
    client
        .expect_list_containers()
        .times(1)
        .returning(move |_| {
            let summaries = (0..count).map(engine_summary).collect();
            Box::pin(async move { Ok(summaries) })
        });

    let runtime = test_runtime()?;
    match list_containers_with_client(&client, runtime.handle()) {
        Ok(containers) => orchestration_state.listed.set(containers.len()),
        Err(error) => return Err(format!("listing should succeed: {error}")),
    }
    Ok(())
}

#[when("token daemon orchestration is invoked with a blank container")]
#[cfg(feature = "experimental")]
fn when_token_daemon_invoked(orchestration_state: &OrchestrationState) {
    let config = AppConfig::default();
    invoke_orchestration(orchestration_state, || {
        run_token_daemon(&config, "   ", DaemonShutdown::new())
    });
}

/// Build the runtime used to drive the injected lifecycle client.
fn test_runtime() -> StepResult<tokio::runtime::Runtime> {
    tokio::runtime::Runtime::new().map_err(|error| format!("failed to create runtime: {error}"))
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
