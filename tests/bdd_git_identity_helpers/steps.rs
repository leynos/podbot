//! Given/when step definitions for Git identity behavioural scenarios.

use std::io;
use std::process::Output;

use bollard::exec::{CreateExecOptions, ResizeExecOptions, StartExecOptions};
use mockall::mock;
use podbot::api::{GitIdentityParams, configure_container_git_identity};
use podbot::engine::{
    ContainerExecClient, CreateExecFuture, HostCommandRunner, InspectExecFuture, ResizeExecFuture,
    StartExecFuture,
};
use rstest_bdd_macros::{given, when};

use super::state::{GitIdentityState, StepResult};
use super::test_helpers::{failure_output, success_output};

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
        fn create_exec(&self, container_id: &str, options: CreateExecOptions<String>)
            -> CreateExecFuture<'_>;
        fn start_exec(&self, exec_id: &str, options: Option<StartExecOptions>) -> StartExecFuture<'_>;
        fn inspect_exec(&self, exec_id: &str) -> InspectExecFuture<'_>;
        fn resize_exec(&self, exec_id: &str, options: ResizeExecOptions) -> ResizeExecFuture<'_>;
    }
}

/// Register an `expect_run_command` expectation on `runner` that matches
/// commands containing `config_key` and returns `success_output` when a
/// value is present or `failure_output` when absent.
fn register_user_config(
    runner: &mut MockHostRunner,
    config_key: &'static str,
    value: Option<&String>,
) {
    match value {
        Some(v) => {
            let owned = v.clone();
            runner
                .expect_run_command()
                .withf(move |_, args| args.contains(&config_key))
                .returning(move |_, _| Ok(success_output(&format!("{owned}\n"))));
        }
        None => {
            runner
                .expect_run_command()
                .withf(move |_, args| args.contains(&config_key))
                .returning(|_, _| Ok(failure_output()));
        }
    }
}

fn setup_mock_host_runner(
    host_name: Option<&String>,
    host_email: Option<&String>,
) -> MockHostRunner {
    let mut host_runner = MockHostRunner::new();
    register_user_config(&mut host_runner, "user.name", host_name);
    register_user_config(&mut host_runner, "user.email", host_email);
    host_runner
}

fn setup_mock_exec_client(should_fail: bool) -> MockExecClient {
    let mut exec_client = MockExecClient::new();

    exec_client
        .expect_create_exec()
        .withf(|_, options| {
            // Validate that the container exec is invoking:
            //   git config --global user.name ...
            // or:
            //   git config --global user.email ...
            let Some(cmd) = &options.cmd else {
                return false;
            };

            // We expect at least: ["git", "config", "--global", "user.name" or "user.email", ...]
            cmd.len() >= 4
                && cmd.first().is_some_and(|s| s == "git")
                && cmd.get(1).is_some_and(|s| s == "config")
                && cmd.get(2).is_some_and(|s| s == "--global")
                && cmd
                    .get(3)
                    .is_some_and(|s| s == "user.name" || s == "user.email")
        })
        .returning(|_, _| {
            Box::pin(async {
                Ok(bollard::exec::CreateExecResults {
                    id: String::from("exec-1"),
                })
            })
        });
    exec_client
        .expect_start_exec()
        .returning(|_, _| Box::pin(async { Ok(bollard::exec::StartExecResults::Detached) }));

    let exit_code = i64::from(should_fail);
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

/// Sets host git user.name from feature file.
/// "missing" is treated specially to indicate None.
#[given("host git user.name is {string}")]
fn host_git_name_is(git_identity_state: &GitIdentityState, string: String) {
    // rstest-bdd {string} captures include the surrounding quotes, so strip them
    let word = string.trim_matches('"').to_owned();
    if word == "missing" {
        git_identity_state.host_name.set(None);
    } else {
        git_identity_state.host_name.set(Some(word));
    }
}

/// Sets host git user.email from feature file.
/// "missing" is treated specially to indicate None.
#[given("host git user.email is {string}")]
fn host_git_email_is(git_identity_state: &GitIdentityState, string: String) {
    // rstest-bdd {string} captures include the surrounding quotes, so strip them
    let word = string.trim_matches('"').to_owned();
    if word == "missing" {
        git_identity_state.host_email.set(None);
    } else {
        git_identity_state.host_email.set(Some(word));
    }
}

#[given("the container engine is available")]
fn container_engine_is_available(git_identity_state: &GitIdentityState) {
    let _ = git_identity_state;
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
        .ok_or_else(|| String::from("host_name not set"))?;
    let host_email = git_identity_state
        .host_email
        .get()
        .ok_or_else(|| String::from("host_email not set"))?;
    let should_fail = git_identity_state
        .should_fail_exec
        .get()
        .ok_or_else(|| String::from("should_fail_exec not set"))?;

    // Convert Option<String> to Option<&String>
    // host_name is already Option<String> after get(), not Option<Option<String>>
    let host_runner = setup_mock_host_runner(host_name.as_ref(), host_email.as_ref());
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
