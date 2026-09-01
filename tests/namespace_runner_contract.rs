//! Guard Podbot's repository-owned Linux runner assignments.

const NAMESPACE_RUNNER: &str = "runs-on: namespace-profile-default";

#[test]
fn repository_owned_linux_workflows_use_the_shared_namespace_profile() {
    for (workflow_name, workflow) in [
        ("audit", include_str!("../.github/workflows/audit.yml")),
        ("CI", include_str!("../.github/workflows/ci.yml")),
        (
            "main coverage",
            include_str!("../.github/workflows/coverage-main.yml"),
        ),
    ] {
        assert!(
            workflow.contains(NAMESPACE_RUNNER),
            "{workflow_name} must use {NAMESPACE_RUNNER}"
        );
    }
}
