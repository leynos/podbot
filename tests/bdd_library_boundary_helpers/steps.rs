//! Given/when/then steps for library boundary scenarios.
//!
//! Provides `given` and `when` step definitions that configure mock
//! environments, construct library-facing load options, invoke exec
//! orchestration with mock clients, and call stub orchestration functions.

use std::sync::Arc;

use bollard::container::LogOutput;
use futures_util::stream;
use mockable::MockEnv;
use mockall::mock;
use podbot::api::{ExecMode, ExecRequest};
#[cfg(feature = "experimental")]
use podbot::api::{list_containers, run_agent, run_token_daemon, stop_container};
#[cfg(feature = "experimental")]
use podbot::config::AppConfig;
use podbot::config::{ConfigLoadOptions, ConfigOverrides, load_config_with_env};
use podbot::engine::{
    ContainerExecClient, CreateExecFuture, InspectExecFuture, ResizeExecFuture, StartExecFuture,
};
use rstest_bdd_macros::{given, when};

use super::StepResult;
#[cfg(feature = "experimental")]
use super::state::StubOutcomes;
use super::state::{ConfigResult, LibraryBoundaryState, LibraryResult};
use crate::test_utils::exec_outcome_with_client;

mock! {
    #[derive(Debug)]
    LibExecClient {}

    impl ContainerExecClient for LibExecClient {
        fn create_exec(&self, container_id: &str, options: bollard::exec::CreateExecOptions<String>) -> CreateExecFuture<'_>;
        fn start_exec(&self, exec_id: &str, options: Option<bollard::exec::StartExecOptions>) -> StartExecFuture<'_>;
        fn inspect_exec(&self, exec_id: &str) -> InspectExecFuture<'_>;
        fn resize_exec(&self, exec_id: &str, options: bollard::exec::ResizeExecOptions) -> ResizeExecFuture<'_>;
    }
}

#[given("a mock environment with engine socket configured")]
fn given_mock_env_with_engine_socket(library_boundary_state: &LibraryBoundaryState) {
    library_boundary_state
        .engine_socket_override
        .set(String::from("unix:///test/podbot.sock"));
}

#[given("explicit load options without config file discovery")]
fn given_explicit_load_options(library_boundary_state: &LibraryBoundaryState) {
    // Defaults already configured; nothing extra needed.
    let _ = library_boundary_state;
}

#[given("a mock container engine client")]
fn given_mock_engine_client(library_boundary_state: &LibraryBoundaryState) {
    library_boundary_state.create_exec_should_fail.set(false);
}

#[given("exec parameters for an attached echo command")]
fn given_exec_params(library_boundary_state: &LibraryBoundaryState) {
    // Params are constructed in the when step; this is a precondition marker.
    let _ = library_boundary_state;
}

#[given("a mock container engine client that fails on create exec")]
fn given_failing_mock_client(library_boundary_state: &LibraryBoundaryState) {
    library_boundary_state.create_exec_should_fail.set(true);
}

#[when("the library configuration loader is called")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
fn when_config_loader_called(library_boundary_state: &LibraryBoundaryState) -> StepResult<()> {
    let socket = library_boundary_state.engine_socket_override.get();

    let mut mock_env = MockEnv::new();
    mock_env.expect_string().returning(|_| None);

    let options = ConfigLoadOptions {
        config_path_hint: None,
        discover_config: false,
        overrides: ConfigOverrides {
            engine_socket: socket,
            image: None,
            agent_kind: None,
            agent_mode: None,
        },
        command_intent: podbot::config::CommandIntent::Any,
    };

    match load_config_with_env(&mock_env, &options) {
        Ok(config) => library_boundary_state
            .config_result
            .set(ConfigResult::Ok(Box::new(config))),
        Err(e) => library_boundary_state
            .config_result
            .set(ConfigResult::Err(Arc::new(e))),
    }
    Ok(())
}

#[when("the library exec function is called")]
fn when_exec_called(library_boundary_state: &LibraryBoundaryState) -> StepResult<()> {
    let should_fail = library_boundary_state
        .create_exec_should_fail
        .get()
        .unwrap_or(false);

    let mut client = MockLibExecClient::new();

    if should_fail {
        client.expect_create_exec().times(1).returning(|_, _| {
            Box::pin(async {
                Err(bollard::errors::Error::DockerResponseServerError {
                    status_code: 500,
                    message: String::from("create exec failed"),
                })
            })
        });
    } else {
        client.expect_create_exec().times(1).returning(|_, _| {
            Box::pin(async {
                Ok(bollard::exec::CreateExecResults {
                    id: String::from("lib-exec-id"),
                })
            })
        });

        client.expect_start_exec().times(1).returning(|_, _| {
            let output_stream = stream::iter(vec![Ok(LogOutput::StdOut {
                message: Vec::from(&b"lib output"[..]).into(),
            })]);
            Box::pin(async move {
                Ok(bollard::exec::StartExecResults::Attached {
                    output: Box::pin(output_stream),
                    input: Box::pin(tokio::io::sink()),
                })
            })
        });

        client
            .expect_resize_exec()
            .times(0)
            .returning(|_, _| Box::pin(async { Ok(()) }));

        client.expect_inspect_exec().times(1).returning(|_| {
            let inspect = bollard::models::ExecInspectResponse {
                running: Some(false),
                exit_code: Some(0),
                ..bollard::models::ExecInspectResponse::default()
            };
            Box::pin(async move { Ok(inspect) })
        });
    }

    let runtime =
        tokio::runtime::Runtime::new().map_err(|e| format!("failed to create runtime: {e}"))?;

    let request = ExecRequest::new(
        "lib-sandbox",
        vec![String::from("echo"), String::from("hello")],
    )
    .map_err(|e| format!("failed to build exec request: {e}"))?
    .with_mode(ExecMode::Attached)
    .with_tty(false);

    let result = exec_outcome_with_client(&client, runtime.handle(), &request);

    match result {
        Ok(outcome) => library_boundary_state
            .exec_result
            .set(LibraryResult::Ok(outcome)),
        Err(e) => library_boundary_state
            .exec_result
            .set(LibraryResult::Err(Arc::new(e))),
    }
    Ok(())
}

#[when("each stub orchestration function is called")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
#[cfg(feature = "experimental")]
fn when_stubs_called(library_boundary_state: &LibraryBoundaryState) -> StepResult<()> {
    let config = AppConfig::default();
    let mut results = Vec::new();

    match run_agent(&config) {
        Ok(outcome) => results.push(LibraryResult::Ok(outcome)),
        Err(e) => results.push(LibraryResult::Err(Arc::new(e))),
    }
    match list_containers() {
        Ok(outcome) => results.push(LibraryResult::Ok(outcome)),
        Err(e) => results.push(LibraryResult::Err(Arc::new(e))),
    }
    match stop_container("test-ctr") {
        Ok(outcome) => results.push(LibraryResult::Ok(outcome)),
        Err(e) => results.push(LibraryResult::Err(Arc::new(e))),
    }
    match run_token_daemon("test-ctr") {
        Ok(outcome) => results.push(LibraryResult::Ok(outcome)),
        Err(e) => results.push(LibraryResult::Err(Arc::new(e))),
    }

    library_boundary_state
        .stub_outcomes
        .set(StubOutcomes { results });
    Ok(())
}
