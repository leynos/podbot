//! Given and When step implementations for Git identity scenarios.

use std::sync::{Arc, Mutex};

use bollard::exec::{
    CreateExecOptions, CreateExecResults, ResizeExecOptions, StartExecOptions, StartExecResults,
};
use bollard::models::ExecInspectResponse;
use podbot::engine::{
    ContainerExecClient, CreateExecFuture, EngineConnector, GitIdentity, GitIdentityReader,
    InspectExecFuture, ResizeExecFuture, StartExecFuture,
};
use rstest_bdd_macros::{given, when};

use super::state::{GitIdentityState, StepResult};

// =============================================================================
// Mock types
// =============================================================================

/// Deterministic reader that returns state-configured identity values.
struct MockReader {
    identity: GitIdentity,
}

impl GitIdentityReader for MockReader {
    fn read_git_identity(&self) -> GitIdentity {
        self.identity.clone()
    }
}

/// Mock exec client that records commands and optionally fails.
struct MockExecClient {
    commands: Arc<Mutex<Vec<Vec<String>>>>,
    should_fail: bool,
}

impl ContainerExecClient for MockExecClient {
    fn create_exec(
        &self,
        _container_id: &str,
        options: CreateExecOptions<String>,
    ) -> CreateExecFuture<'_> {
        let cmd = options.cmd.clone().unwrap_or_default();
        self.commands
            .lock()
            .expect("lock poisoned")
            .push(cmd);

        let fail = self.should_fail;
        Box::pin(async move {
            if fail {
                Err(bollard::errors::Error::DockerResponseServerError {
                    status_code: 500,
                    message: String::from("mock exec failure"),
                })
            } else {
                Ok(CreateExecResults {
                    id: String::from("bdd-exec-id"),
                })
            }
        })
    }

    fn start_exec(
        &self,
        _exec_id: &str,
        _options: Option<StartExecOptions>,
    ) -> StartExecFuture<'_> {
        Box::pin(async { Ok(StartExecResults::Detached) })
    }

    fn inspect_exec(&self, _exec_id: &str) -> InspectExecFuture<'_> {
        Box::pin(async {
            Ok(ExecInspectResponse {
                running: Some(false),
                exit_code: Some(0),
                ..ExecInspectResponse::default()
            })
        })
    }

    fn resize_exec(
        &self,
        _exec_id: &str,
        _options: ResizeExecOptions,
    ) -> ResizeExecFuture<'_> {
        Box::pin(async { Ok(()) })
    }
}

// =============================================================================
// Given steps
// =============================================================================

#[given("host Git user name is configured as {name}")]
fn host_git_user_name_configured(
    git_identity_state: &GitIdentityState,
    name: String,
) {
    git_identity_state.host_name.set(Some(name));
}

#[given("host Git user name is absent")]
fn host_git_user_name_absent(git_identity_state: &GitIdentityState) {
    git_identity_state.host_name.set(None);
}

#[given("host Git user email is configured as {email}")]
fn host_git_user_email_configured(
    git_identity_state: &GitIdentityState,
    email: String,
) {
    git_identity_state.host_email.set(Some(email));
}

#[given("host Git user email is absent")]
fn host_git_user_email_absent(git_identity_state: &GitIdentityState) {
    git_identity_state.host_email.set(None);
}

#[given("container exec will fail")]
fn container_exec_will_fail(git_identity_state: &GitIdentityState) {
    git_identity_state.should_fail_exec.set(true);
}

// =============================================================================
// When steps
// =============================================================================

#[when("Git identity is applied to the container")]
fn git_identity_is_applied(
    git_identity_state: &GitIdentityState,
) -> StepResult<()> {
    let name = git_identity_state.host_name.get().flatten();
    let email = git_identity_state.host_email.get().flatten();

    let should_fail = git_identity_state
        .should_fail_exec
        .get()
        .unwrap_or(false);

    let commands = Arc::new(Mutex::new(Vec::new()));
    let client = MockExecClient {
        commands: Arc::clone(&commands),
        should_fail,
    };

    let reader = MockReader {
        identity: GitIdentity::new(name, email),
    };

    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| format!("failed to create runtime: {e}"))?;

    let result = runtime.block_on(EngineConnector::configure_git_identity_async(
        &client,
        "bdd-container-id",
        &reader,
    ));

    match result {
        Ok(identity_result) => {
            git_identity_state.outcome.set(Some(identity_result));
        }
        Err(error) => {
            return Err(format!("unexpected error: {error}"));
        }
    }

    let captured = commands.lock().expect("lock poisoned").clone();
    git_identity_state.captured_commands.set(captured);

    Ok(())
}
