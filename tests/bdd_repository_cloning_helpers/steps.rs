//! Given/when step definitions for repository-cloning behavioural scenarios.

use std::sync::{Arc, Mutex};

use bollard::exec::{CreateExecOptions, ResizeExecOptions, StartExecOptions};
use mockall::mock;
use podbot::api::{
    BranchName, CloneRepositoryParams, RepositoryRef, clone_repository_into_workspace,
};
use podbot::engine::{
    ContainerExecClient, CreateExecFuture, InspectExecFuture, ResizeExecFuture, StartExecFuture,
};
use rstest_bdd_macros::{given, when};

use super::state::{ObservedExec, RepositoryCloningState, StepResult};

mock! {
    #[derive(Debug)]
    ExecClient {}
    impl ContainerExecClient for ExecClient {
        fn create_exec(
            &self,
            container_id: &str,
            options: CreateExecOptions<String>,
        ) -> CreateExecFuture<'_>;
        fn start_exec(
            &self,
            exec_id: &str,
            options: Option<StartExecOptions>,
        ) -> StartExecFuture<'_>;
        fn inspect_exec(&self, exec_id: &str) -> InspectExecFuture<'_>;
        fn resize_exec(
            &self,
            exec_id: &str,
            options: ResizeExecOptions,
        ) -> ResizeExecFuture<'_>;
    }
}

#[given("repository input is {string}")]
fn repository_input_is(repository_cloning_state: &RepositoryCloningState, string: String) {
    repository_cloning_state
        .repository_input
        .set(strip_quotes(&string));
}

#[given("branch input is {string}")]
fn branch_input_is(repository_cloning_state: &RepositoryCloningState, string: String) {
    repository_cloning_state
        .branch_input
        .set(strip_quotes(&string));
}

#[given("workspace base directory is {string}")]
fn workspace_base_directory_is(repository_cloning_state: &RepositoryCloningState, string: String) {
    repository_cloning_state
        .workspace_base_dir
        .set(strip_quotes(&string));
}

#[given("git askpass helper path is {string}")]
fn git_askpass_helper_path_is(repository_cloning_state: &RepositoryCloningState, string: String) {
    repository_cloning_state
        .askpass_path
        .set(strip_quotes(&string));
}

#[given("repository clone execs will succeed")]
fn repository_clone_execs_will_succeed(repository_cloning_state: &RepositoryCloningState) {
    repository_cloning_state.clone_exit_code.set(0);
    repository_cloning_state.verification_exit_code.set(0);
}

#[given("repository clone exec will fail")]
fn repository_clone_exec_will_fail(repository_cloning_state: &RepositoryCloningState) {
    repository_cloning_state.clone_exit_code.set(128);
}

#[given("repository branch verification will fail")]
fn repository_branch_verification_will_fail(repository_cloning_state: &RepositoryCloningState) {
    repository_cloning_state.verification_exit_code.set(1);
}

#[when("repository cloning is requested for container {container_id}")]
fn repository_cloning_is_requested_for_container(
    repository_cloning_state: &RepositoryCloningState,
    container_id: String,
) -> StepResult<()> {
    let repository_input = required_slot(
        repository_cloning_state.repository_input.get(),
        "repository_input",
    )?;
    let branch_input = required_slot(repository_cloning_state.branch_input.get(), "branch_input")?;
    let workspace_base_dir = required_slot(
        repository_cloning_state.workspace_base_dir.get(),
        "workspace_base_dir",
    )?;
    let askpass_path = required_slot(repository_cloning_state.askpass_path.get(), "askpass_path")?;
    let observed_execs = required_slot(
        repository_cloning_state.observed_execs.get(),
        "observed_execs",
    )?;

    let invocation = CloneInvocation {
        container_id: &container_id,
        repository_input: &repository_input,
        branch_input: &branch_input,
        workspace_base_dir: &workspace_base_dir,
        askpass_path: &askpass_path,
    };
    let outcome = invoke_clone(&invocation, repository_cloning_state, &observed_execs);
    repository_cloning_state.outcome.set(outcome);

    Ok(())
}

struct CloneInvocation<'a> {
    container_id: &'a str,
    repository_input: &'a str,
    branch_input: &'a str,
    workspace_base_dir: &'a str,
    askpass_path: &'a str,
}

fn invoke_clone(
    invocation: &CloneInvocation<'_>,
    repository_cloning_state: &RepositoryCloningState,
    observed_execs: &Arc<Mutex<Vec<ObservedExec>>>,
) -> Result<podbot::engine::RepositoryCloneResult, podbot::error::PodbotError> {
    let repository = RepositoryRef::parse(invocation.repository_input)?;
    let branch = BranchName::parse(invocation.branch_input)?;
    let client = mock_exec_client(
        repository_cloning_state.clone_exit_code.get().unwrap_or(0),
        repository_cloning_state
            .verification_exit_code
            .get()
            .unwrap_or(0),
        observed_execs,
    );
    let runtime = tokio::runtime::Runtime::new().map_err(|err| {
        podbot::error::ContainerError::RuntimeCreationFailed {
            message: err.to_string(),
        }
    })?;
    let handle = runtime.handle().clone();

    clone_repository_into_workspace(&CloneRepositoryParams {
        client: &client,
        container_id: invocation.container_id,
        repository,
        branch,
        workspace_base_dir: invocation.workspace_base_dir,
        askpass_path: invocation.askpass_path,
        runtime_handle: &handle,
    })
}

fn mock_exec_client(
    clone_exit_code: i64,
    verification_exit_code: i64,
    observed_execs: &Arc<Mutex<Vec<ObservedExec>>>,
) -> MockExecClient {
    let mut client = MockExecClient::new();
    let exits = Arc::new(Mutex::new(vec![clone_exit_code, verification_exit_code]));
    let observed = Arc::clone(observed_execs);

    client.expect_create_exec().returning(move |_, options| {
        if let Ok(mut execs) = observed.lock() {
            execs.push(ObservedExec {
                command: options.cmd.unwrap_or_default(),
                env: options.env.unwrap_or_default(),
            });
        }

        Box::pin(async {
            Ok(bollard::exec::CreateExecResults {
                id: String::from("exec-id"),
            })
        })
    });
    client
        .expect_start_exec()
        .returning(|_, _| Box::pin(async { Ok(bollard::exec::StartExecResults::Detached) }));
    client.expect_inspect_exec().returning(move |_| {
        let code = exits
            .lock()
            .ok()
            .and_then(|mut exit_codes| {
                if exit_codes.is_empty() {
                    None
                } else {
                    Some(exit_codes.remove(0))
                }
            })
            .unwrap_or(1);

        Box::pin(async move {
            Ok(bollard::models::ExecInspectResponse {
                exit_code: Some(code),
                running: Some(false),
                ..Default::default()
            })
        })
    });

    client
}

fn required_slot<T>(value: Option<T>, name: &str) -> StepResult<T> {
    value.ok_or_else(|| format!("{name} not set"))
}

fn strip_quotes(value: &str) -> String {
    value.trim_matches('"').to_owned()
}
