//! Given/when step definitions for the end-to-end repository-cloning scenarios.

use std::sync::Arc;

use podbot::api::{AskpassPath, BranchName, RepositoryRef, WorkspacePath};
use podbot::engine::{RepositoryCloneRequest, clone_repository_into_workspace};
use rstest_bdd_macros::{given, when};

use super::container::{askpass_path, launch_sandbox_bundle, workspace_path};
use super::state::{RepositoryCloningE2eState, SandboxBundle, StepResult};

#[given("a sandbox container running with git installed")]
pub(crate) fn a_sandbox_container_running_with_git_installed(
    repository_cloning_e2e_state: &RepositoryCloningE2eState,
) -> StepResult<()> {
    // Starting a container is expensive and not idempotent. Skip the work if
    // a previous invocation in the same scenario has already populated the
    // bundle slot.
    if repository_cloning_e2e_state.bundle.is_filled() {
        return Ok(());
    }

    let bundle = launch_sandbox_bundle()?;
    repository_cloning_e2e_state.bundle.set(Arc::new(bundle));
    repository_cloning_e2e_state
        .askpass_path
        .set(String::from(askpass_path()));
    repository_cloning_e2e_state
        .workspace_base_dir
        .set(String::from(workspace_path()));
    Ok(())
}

/// The bare repository name the container setup script prepares.
const FIXTURE_BARE_REPOSITORY: &str = "leynos/podbot";
/// The branch the prepared bare repository advertises.
const FIXTURE_BARE_BRANCH: &str = "main";

#[given("a local bare repository {repository} has branch {branch}")]
pub(crate) fn a_local_bare_repository_has_branch(
    repository_cloning_e2e_state: &RepositoryCloningE2eState,
    repository: String,
    branch: String,
) {
    // The bare repository at /srv/test-repos/leynos/podbot.git is pre-baked
    // by the container setup script with branch `main`. Fail fast if a
    // scenario asks for any other coordinates so the feature text stays
    // coupled to the fixture rather than silently no-op'ing on a mismatch.
    let _ = repository_cloning_e2e_state;
    assert_eq!(
        repository, FIXTURE_BARE_REPOSITORY,
        "container setup script only prepares the {FIXTURE_BARE_REPOSITORY:?} bare repository",
    );
    assert_eq!(
        branch, FIXTURE_BARE_BRANCH,
        "container setup script only prepares branch {FIXTURE_BARE_BRANCH:?}",
    );
}

#[given("the container rewrites GitHub URLs to the local repository server")]
pub(crate) fn the_container_rewrites_github_urls(
    repository_cloning_e2e_state: &RepositoryCloningE2eState,
) {
    // The /root/.gitconfig file written during container setup contains the
    // `url."file:///srv/test-repos/".insteadOf=https://github.com/` rewrite,
    // so no extra wiring is required for this step.
    let _ = repository_cloning_e2e_state;
}

#[given("the git askpass helper path is {string}")]
pub(crate) fn the_git_askpass_helper_path_is(
    repository_cloning_e2e_state: &RepositoryCloningE2eState,
    string: String,
) {
    repository_cloning_e2e_state
        .askpass_path
        .set(strip_quotes(&string));
}

#[given("the workspace base directory is {string}")]
pub(crate) fn the_workspace_base_directory_is(
    repository_cloning_e2e_state: &RepositoryCloningE2eState,
    string: String,
) {
    repository_cloning_e2e_state
        .workspace_base_dir
        .set(strip_quotes(&string));
}

#[when("repository cloning is requested for repository {repository} on branch {branch}")]
pub(crate) fn repository_cloning_is_requested(
    repository_cloning_e2e_state: &RepositoryCloningE2eState,
    repository: String,
    branch: String,
) -> StepResult<()> {
    let inputs = read_request_inputs(repository_cloning_e2e_state)?;
    let repository_ref =
        RepositoryRef::parse(&repository).map_err(|e| format!("invalid repository: {e}"))?;
    let branch_name = BranchName::parse(&branch).map_err(|e| format!("invalid branch: {e}"))?;
    let outcome = execute_clone(&inputs, &repository_ref, &branch_name);
    repository_cloning_e2e_state.outcome.set(outcome);
    Ok(())
}

struct CloneRequestInputs {
    bundle: Arc<SandboxBundle>,
    workspace_base_dir: WorkspacePath,
    askpass_path: AskpassPath,
}

fn read_request_inputs(state: &RepositoryCloningE2eState) -> StepResult<CloneRequestInputs> {
    let workspace_raw = required_slot(state.workspace_base_dir.get(), "workspace_base_dir")?;
    let askpass_raw = required_slot(state.askpass_path.get(), "askpass_path")?;
    Ok(CloneRequestInputs {
        bundle: required_slot(state.bundle.get(), "bundle")?,
        workspace_base_dir: WorkspacePath::parse(&workspace_raw)
            .map_err(|e| format!("invalid workspace_base_dir: {e}"))?,
        askpass_path: AskpassPath::parse(&askpass_raw)
            .map_err(|e| format!("invalid askpass_path: {e}"))?,
    })
}

fn execute_clone(
    inputs: &CloneRequestInputs,
    repository: &RepositoryRef,
    branch: &BranchName,
) -> Result<podbot::engine::RepositoryCloneResult, podbot::error::PodbotError> {
    clone_repository_into_workspace(
        inputs.bundle.runtime.handle(),
        inputs.bundle.docker.as_ref(),
        &RepositoryCloneRequest {
            container_id: &inputs.bundle.container_id,
            repository,
            branch,
            workspace_base_dir: &inputs.workspace_base_dir,
            askpass_path: &inputs.askpass_path,
        },
    )
}

fn required_slot<T>(value: Option<T>, name: &str) -> StepResult<T> {
    value.ok_or_else(|| format!("{name} not set"))
}

fn strip_quotes(value: &str) -> String {
    value.trim_matches('"').to_owned()
}
