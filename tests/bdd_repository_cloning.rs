//! Behavioural tests for repository cloning.

mod bdd_repository_cloning_helpers;

pub use bdd_repository_cloning_helpers::{RepositoryCloningState, repository_cloning_state};
use bdd_repository_cloning_helpers::{assertions, steps};
use rstest_bdd_macros::scenario;

#[scenario(
    path = "tests/features/repository_cloning.feature",
    name = "Repository clone succeeds"
)]
fn repository_clone_succeeds(
    repository_cloning_state: RepositoryCloningState,
) -> Result<(), String> {
    steps::repository_input_is(&repository_cloning_state, String::from("\"leynos/podbot\""));
    steps::branch_input_is(&repository_cloning_state, String::from("\"main\""));
    steps::workspace_base_directory_is(&repository_cloning_state, String::from("\"/work\""));
    steps::git_askpass_helper_path_is(
        &repository_cloning_state,
        String::from("\"/usr/local/bin/git-askpass\""),
    );
    steps::repository_clone_execs_will_succeed(&repository_cloning_state);
    steps::repository_cloning_is_requested_for_container(
        &repository_cloning_state,
        String::from("sandbox-clone"),
    )?;
    assertions::repository_cloning_succeeds(&repository_cloning_state)?;
    assertions::the_workspace_path_is(&repository_cloning_state, String::from("\"/work\""))?;
    assertions::the_checked_out_branch_is(&repository_cloning_state, String::from("\"main\""))?;
    assertions::the_clone_command_used_git_askpass(&repository_cloning_state)
}

#[scenario(
    path = "tests/features/repository_cloning.feature",
    name = "Malformed repository input fails before exec"
)]
fn malformed_repository_input_fails_before_exec(
    repository_cloning_state: RepositoryCloningState,
) -> Result<(), String> {
    steps::repository_input_is(
        &repository_cloning_state,
        String::from("\"leynos /podbot\""),
    );
    steps::branch_input_is(&repository_cloning_state, String::from("\"main\""));
    steps::workspace_base_directory_is(&repository_cloning_state, String::from("\"/work\""));
    steps::git_askpass_helper_path_is(
        &repository_cloning_state,
        String::from("\"/usr/local/bin/git-askpass\""),
    );
    steps::repository_cloning_is_requested_for_container(
        &repository_cloning_state,
        String::from("sandbox-clone"),
    )?;
    assertions::repository_cloning_fails_with_a_configuration_error(&repository_cloning_state)?;
    assertions::no_repository_clone_exec_was_attempted(&repository_cloning_state)
}

#[scenario(
    path = "tests/features/repository_cloning.feature",
    name = "Relative workspace path fails before exec"
)]
fn relative_workspace_path_fails_before_exec(
    repository_cloning_state: RepositoryCloningState,
) -> Result<(), String> {
    steps::repository_input_is(&repository_cloning_state, String::from("\"leynos/podbot\""));
    steps::branch_input_is(&repository_cloning_state, String::from("\"main\""));
    steps::workspace_base_directory_is(&repository_cloning_state, String::from("\"work\""));
    steps::git_askpass_helper_path_is(
        &repository_cloning_state,
        String::from("\"/usr/local/bin/git-askpass\""),
    );
    steps::repository_cloning_is_requested_for_container(
        &repository_cloning_state,
        String::from("sandbox-clone"),
    )?;
    assertions::repository_cloning_fails_with_a_configuration_error(&repository_cloning_state)?;
    assertions::no_repository_clone_exec_was_attempted(&repository_cloning_state)
}

#[scenario(
    path = "tests/features/repository_cloning.feature",
    name = "Clone exec failure is reported"
)]
fn clone_exec_failure_is_reported(
    repository_cloning_state: RepositoryCloningState,
) -> Result<(), String> {
    steps::repository_input_is(&repository_cloning_state, String::from("\"leynos/podbot\""));
    steps::branch_input_is(&repository_cloning_state, String::from("\"main\""));
    steps::workspace_base_directory_is(&repository_cloning_state, String::from("\"/work\""));
    steps::git_askpass_helper_path_is(
        &repository_cloning_state,
        String::from("\"/usr/local/bin/git-askpass\""),
    );
    steps::repository_clone_exec_will_fail(&repository_cloning_state);
    steps::repository_cloning_is_requested_for_container(
        &repository_cloning_state,
        String::from("sandbox-clone"),
    )?;
    assertions::repository_cloning_fails_with_an_exec_error(&repository_cloning_state)
}

#[scenario(
    path = "tests/features/repository_cloning.feature",
    name = "Branch verification failure is reported"
)]
fn branch_verification_failure_is_reported(
    repository_cloning_state: RepositoryCloningState,
) -> Result<(), String> {
    steps::repository_input_is(&repository_cloning_state, String::from("\"leynos/podbot\""));
    steps::branch_input_is(&repository_cloning_state, String::from("\"main\""));
    steps::workspace_base_directory_is(&repository_cloning_state, String::from("\"/work\""));
    steps::git_askpass_helper_path_is(
        &repository_cloning_state,
        String::from("\"/usr/local/bin/git-askpass\""),
    );
    steps::repository_branch_verification_will_fail(&repository_cloning_state);
    steps::repository_cloning_is_requested_for_container(
        &repository_cloning_state,
        String::from("sandbox-clone"),
    )?;
    assertions::repository_cloning_fails_with_an_exec_error(&repository_cloning_state)
}
