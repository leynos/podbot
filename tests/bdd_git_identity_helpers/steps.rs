//! Given/when step definitions for Git identity behavioural scenarios.

use std::io;
use std::os::unix::process::ExitStatusExt;
use std::process::{ExitStatus, Output};

use mockall::mock;
use podbot::api::{GitIdentityParams, configure_container_git_identity};
use podbot::engine::{
    ContainerExecClient, CreateExecFuture, EngineConnector, ExecMode, ExecRequest, ExecResult,
    HostCommandRunner, InspectExecFuture, ResizeExecFuture, StartExecFuture,
};
use podbot::error::{ContainerError, PodbotError};
use rstest_bdd_macros::{given, when};

use super::state::{GitIdentityState, StepResult};

mock! {
    #[derive(Debug)]
    HostRunner {}
    impl HostCommandRunner for HostRunner {
        fn run_command<'a>(
            &self,
            program: &'a str,
            args: &'a [&'a str],
        ) -> io::Result<Output>;
    }
}

mock! {
    #[derive(Debug)]
    ExecClient {}
    impl ContainerExecClient for ExecClient {
        fn create_exec(&self, container_id: &str, command: Vec<String>, tty: bool)
            -> CreateExecFuture<'_>;
        fn start_exec(&self, exec_id: &str, tty: bool) -> StartExecFuture<'_>;
        fn inspect_exec(&self, exec_id: &str) -> InspectExecFuture<'_>;
        fn resize_exec(&self, exec_id: &str, width: u16, height: u16) -> ResizeExecFuture<'_>;
    }
}

fn success_output(stdout: &str) -> Output {
    Output {
        status: ExitStatus::from_raw(0),
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
    }
}

fn failure_output() -> Output {
    Output {
        status: ExitStatus::from_raw(256),
        stdout: Vec::new(),
        stderr: b"error".to_vec(),
    }
}

fn setup_mock_host_runner(
    host_name: &Option<String>,
    host_email: &Option<String>,
) -> MockHostRunner {
    let mut host_runner = MockHostRunner::new();

    match host_name {
        Some(name) => {
            let name_clone = name.clone();
            host_runner
                .expect_run_command()
                .withf(|_, args| args.contains(&"user.name"))
                .returning(move |_, _| Ok(success_output(&format!("{name_clone}\n"))));
        }
        None => {
            host_runner
                .expect_run_command()
                .withf(|_, args| args.contains(&"user.name"))
                .returning(|_, _| Ok(failure_output()));
        }
    }

    match host_email {
        Some(email) => {
            let email_clone = email.clone();
            host_runner
                .expect_run_command()
                .withf(|_, args| args.contains(&"user.email"))
                .returning(move |_, _| Ok(success_output(&format!("{email_clone}\n"))));
        }
        None => {
            host_runner
                .expect_run_command()
                .withf(|_, args| args.contains(&"user.email"))
                .returning(|_, _| Ok(failure_output()));
        }
    }

    host_runner
}

fn setup_mock_exec_client(should_fail: bool) -> MockExecClient {
    let mut exec_client = MockExecClient::new();

    exec_client
        .expect_create_exec()
        .returning(|_, _, _| Box::pin(async { Ok(String::from("exec-1")) }));
    exec_client
        .expect_start_exec()
        .returning(|_, _| Box::pin(async { Ok(()) }));

    let exit_code = if should_fail { 1 } else { 0 };
    exec_client.expect_inspect_exec().returning(move |_| {
        Box::pin(async move {
            Ok(bollard::models::ExecInspectResponse {
                exit_code: Some(exit_code),
                running: Some(false),
                ..Default::default()
            })
        })
    });

    exec_client
}

#[given("host git user.name is {name}")]
fn host_git_name_is_name(git_identity_state: &GitIdentityState, name: String) {
    git_identity_state.host_name.set(Some(name));
}

#[given("host git user.email is {email}")]
fn host_git_email_is_email(git_identity_state: &GitIdentityState, email: String) {
    git_identity_state.host_email.set(Some(email));
}

#[given("host git user.name is missing")]
fn host_git_name_is_missing(git_identity_state: &GitIdentityState) {
    git_identity_state.host_name.set(None);
}

#[given("host git user.email is missing")]
fn host_git_email_is_missing(git_identity_state: &GitIdentityState) {
    git_identity_state.host_email.set(None);
}

#[given("the container engine is available")]
fn container_engine_is_available(_git_identity_state: &GitIdentityState) {
    // No-op: this is just narrative confirmation that exec will succeed.
}

#[given("the container engine exec will fail")]
fn container_engine_exec_will_fail(git_identity_state: &GitIdentityState) {
    git_identity_state.should_fail_exec.set(true);
}

#[when("git identity configuration is requested for container {container_id}")]
fn git_identity_configuration_is_requested(
    git_identity_state: &GitIdentityState,
    container_id: String,
) -> StepResult<()> {
    git_identity_state.container_id.set(container_id.clone());

    let host_name = git_identity_state
        .host_name
        .get()
        .ok_or_else(|| String::from("host_name not set"))?
        .clone();
    let host_email = git_identity_state
        .host_email
        .get()
        .ok_or_else(|| String::from("host_email not set"))?
        .clone();
    let should_fail = git_identity_state
        .should_fail_exec
        .get()
        .ok_or_else(|| String::from("should_fail_exec not set"))?;

    let host_runner = setup_mock_host_runner(&host_name, &host_email);
    let exec_client = setup_mock_exec_client(should_fail);

    // Create runtime and execute
    let runtime =
        tokio::runtime::Runtime::new().map_err(|e| format!("Failed to create runtime: {e}"))?;
    let handle = runtime.handle().clone();

    let params = GitIdentityParams {
        client: &exec_client,
        host_runner: &host_runner,
        container_id: &container_id,
        runtime_handle: &handle,
    };

    let result = configure_container_git_identity(&params);
    git_identity_state.outcome.set(result);

    Ok(())
}
