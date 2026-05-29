//! Then step definitions for the end-to-end repository-cloning scenarios.

use std::sync::Arc;

use podbot::error::{ContainerError, PodbotError};
use rstest_bdd_macros::then;
use testcontainers::core::ExecCommand;

use super::state::{RepositoryCloningE2eState, SandboxBundle, StepResult};

#[then("repository cloning succeeds")]
pub(crate) fn repository_cloning_succeeds(
    repository_cloning_e2e_state: &RepositoryCloningE2eState,
) -> StepResult<()> {
    repository_cloning_e2e_state
        .outcome
        .with_ref(|outcome| match outcome {
            Ok(_) => Ok(()),
            Err(err) => Err(format!("Expected clone success, got error: {err}")),
        })
        .ok_or_else(|| String::from("outcome not set"))?
}

#[then("repository cloning fails with an exec error")]
pub(crate) fn repository_cloning_fails_with_an_exec_error(
    repository_cloning_e2e_state: &RepositoryCloningE2eState,
) -> StepResult<()> {
    repository_cloning_e2e_state
        .outcome
        .with_ref(|outcome| match outcome {
            Ok(result) => Err(format!("Expected error, got success: {result:?}")),
            Err(PodbotError::Container(ContainerError::ExecFailed { .. })) => Ok(()),
            Err(other) => Err(format!("Unexpected error: {other:?}")),
        })
        .ok_or_else(|| String::from("outcome not set"))?
}

fn check_git_dir(state: &RepositoryCloningE2eState, path: &str) -> StepResult<i64> {
    let bundle = required_bundle(state)?;
    exec_in_container(
        &bundle,
        vec![
            String::from("test"),
            String::from("-d"),
            format!("{path}/.git"),
        ],
    )
}

#[then("the workspace at {string} contains a git repository")]
pub(crate) fn the_workspace_at_contains_a_git_repository(
    repository_cloning_e2e_state: &RepositoryCloningE2eState,
    string: String,
) -> StepResult<()> {
    let path = strip_quotes(&string);
    let exit_code = check_git_dir(repository_cloning_e2e_state, &path)?;
    if exit_code == 0 {
        Ok(())
    } else {
        Err(format!(
            "expected {path}/.git directory to exist, test exited {exit_code}"
        ))
    }
}

#[then("the workspace at {string} does not contain a git repository")]
pub(crate) fn the_workspace_at_does_not_contain_a_git_repository(
    repository_cloning_e2e_state: &RepositoryCloningE2eState,
    string: String,
) -> StepResult<()> {
    let path = strip_quotes(&string);
    let exit_code = check_git_dir(repository_cloning_e2e_state, &path)?;
    if exit_code == 0 {
        Err(format!(
            "expected {path}/.git directory to be absent, but it exists"
        ))
    } else {
        Ok(())
    }
}

#[then("the checked out branch in the workspace is {string}")]
pub(crate) fn the_checked_out_branch_in_the_workspace_is(
    repository_cloning_e2e_state: &RepositoryCloningE2eState,
    string: String,
) -> StepResult<()> {
    let expected = strip_quotes(&string);
    let workspace = repository_cloning_e2e_state
        .workspace_base_dir
        .get()
        .ok_or_else(|| String::from("workspace_base_dir not set"))?;
    let bundle = required_bundle(repository_cloning_e2e_state)?;
    let observed_stdout = exec_capture_stdout(
        &bundle,
        vec![
            String::from("git"),
            String::from("-C"),
            workspace,
            String::from("rev-parse"),
            String::from("--abbrev-ref"),
            String::from("HEAD"),
        ],
    )?;
    let observed = observed_stdout.trim();
    if observed == expected {
        Ok(())
    } else {
        Err(format!(
            "expected checked-out branch '{expected}', observed '{observed}'"
        ))
    }
}

fn required_bundle(state: &RepositoryCloningE2eState) -> StepResult<Arc<SandboxBundle>> {
    state
        .bundle
        .get()
        .ok_or_else(|| String::from("bundle not set"))
}

struct ExecOutput {
    exit_code: i64,
    stdout: Vec<u8>,
}

fn exec_raw(bundle: &SandboxBundle, cmd: Vec<String>) -> StepResult<ExecOutput> {
    let container = bundle
        .container
        .as_ref()
        .ok_or_else(|| String::from("container has already been torn down"))?;
    bundle.runtime.block_on(async move {
        let mut exec = container
            .exec(ExecCommand::new(cmd))
            .await
            .map_err(|err| format!("container.exec failed: {err}"))?;
        let stdout = exec
            .stdout_to_vec()
            .await
            .map_err(|err| format!("stdout drain failed: {err}"))?;
        let exit_code = exec
            .exit_code()
            .await
            .map_err(|err| format!("exit_code failed: {err}"))?
            .ok_or_else(|| String::from("exec exit code missing"))?;
        Ok(ExecOutput { exit_code, stdout })
    })
}

fn exec_in_container(bundle: &SandboxBundle, cmd: Vec<String>) -> StepResult<i64> {
    exec_raw(bundle, cmd).map(|out| out.exit_code)
}

fn exec_capture_stdout(bundle: &SandboxBundle, cmd: Vec<String>) -> StepResult<String> {
    let out = exec_raw(bundle, cmd)?;
    String::from_utf8(out.stdout).map_err(|err| format!("stdout was not utf-8: {err}"))
}

fn strip_quotes(value: &str) -> String {
    value.trim_matches('"').to_owned()
}
