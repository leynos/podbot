//! Then step definitions for repository-cloning behavioural scenarios.

use podbot::error::{ConfigError, ContainerError, PodbotError};
use rstest_bdd_macros::then;

use super::state::{RepositoryCloningState, StepResult};

#[then("repository cloning succeeds")]
fn repository_cloning_succeeds(
    repository_cloning_state: &RepositoryCloningState,
) -> StepResult<()> {
    repository_cloning_state
        .outcome
        .with_ref(|outcome| match outcome {
            Ok(_) => Ok(()),
            Err(err) => Err(format!("Expected clone success, got error: {err}")),
        })
        .ok_or_else(|| String::from("outcome not set"))?
}

#[then("the workspace path is {string}")]
fn the_workspace_path_is(
    repository_cloning_state: &RepositoryCloningState,
    string: String,
) -> StepResult<()> {
    let expected = string.trim_matches('"');
    repository_cloning_state
        .outcome
        .with_ref(|outcome| match outcome {
            Ok(result) if result.workspace_path == expected => Ok(()),
            Ok(result) => Err(format!(
                "Expected workspace path '{expected}', got '{}'",
                result.workspace_path
            )),
            Err(err) => Err(format!("Expected clone success, got error: {err}")),
        })
        .ok_or_else(|| String::from("outcome not set"))?
}

#[then("the checked out branch is {string}")]
fn the_checked_out_branch_is(
    repository_cloning_state: &RepositoryCloningState,
    string: String,
) -> StepResult<()> {
    let expected = string.trim_matches('"');
    repository_cloning_state
        .outcome
        .with_ref(|outcome| match outcome {
            Ok(result) if result.checked_out_branch == expected => Ok(()),
            Ok(result) => Err(format!(
                "Expected branch '{expected}', got '{}'",
                result.checked_out_branch
            )),
            Err(err) => Err(format!("Expected clone success, got error: {err}")),
        })
        .ok_or_else(|| String::from("outcome not set"))?
}

#[then("the clone command used GIT_ASKPASS")]
fn the_clone_command_used_git_askpass(
    repository_cloning_state: &RepositoryCloningState,
) -> StepResult<()> {
    let observed_execs = repository_cloning_state
        .observed_execs
        .get()
        .ok_or_else(|| String::from("observed_execs not set"))?;
    let execs = observed_execs
        .lock()
        .map_err(|err| format!("observed_execs lock poisoned: {err}"))?;
    let Some(first_exec) = execs.first() else {
        return Err(String::from("no execs were observed"));
    };

    if first_exec
        .env
        .iter()
        .any(|entry| entry == "GIT_ASKPASS=/usr/local/bin/git-askpass")
        && first_exec
            .env
            .iter()
            .any(|entry| entry == "GIT_TERMINAL_PROMPT=0")
    {
        Ok(())
    } else {
        Err(format!(
            "expected git askpass environment, got {:?}",
            first_exec.env
        ))
    }
}

#[then("repository cloning fails with a configuration error")]
fn repository_cloning_fails_with_a_configuration_error(
    repository_cloning_state: &RepositoryCloningState,
) -> StepResult<()> {
    assert_error(repository_cloning_state, |err| {
        matches!(
            err,
            PodbotError::Config(
                ConfigError::InvalidValue { .. } | ConfigError::MissingRequired { .. }
            )
        )
    })
}

#[then("repository cloning fails with an exec error")]
fn repository_cloning_fails_with_an_exec_error(
    repository_cloning_state: &RepositoryCloningState,
) -> StepResult<()> {
    assert_error(repository_cloning_state, |err| {
        matches!(
            err,
            PodbotError::Container(ContainerError::ExecFailed { .. })
        )
    })
}

#[then("no repository clone exec was attempted")]
fn no_repository_clone_exec_was_attempted(
    repository_cloning_state: &RepositoryCloningState,
) -> StepResult<()> {
    let observed_execs = repository_cloning_state
        .observed_execs
        .get()
        .ok_or_else(|| String::from("observed_execs not set"))?;
    let execs = observed_execs
        .lock()
        .map_err(|err| format!("observed_execs lock poisoned: {err}"))?;

    if execs.is_empty() {
        Ok(())
    } else {
        Err(format!("expected no execs, got {execs:?}"))
    }
}

fn assert_error(
    repository_cloning_state: &RepositoryCloningState,
    predicate: impl Fn(&PodbotError) -> bool,
) -> StepResult<()> {
    repository_cloning_state
        .outcome
        .with_ref(|outcome| match outcome {
            Ok(result) => Err(format!("Expected error, got success: {result:?}")),
            Err(err) if predicate(err) => Ok(()),
            Err(err) => Err(format!("Unexpected error: {err:?}")),
        })
        .ok_or_else(|| String::from("outcome not set"))?
}
