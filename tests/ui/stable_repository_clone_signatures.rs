//! Compile-pass test: stable repository-clone value types are usable from a
//! crate that depends only on the stable podbot API surface.

fn main() {
    // RepositoryRef is parseable and exposes owner/name.
    let repo = podbot::api::RepositoryRef::parse("leynos/podbot")
        .expect("valid repository ref");
    let _owner: &str = repo.owner();
    let _name: &str = repo.name();

    // BranchName is parseable and exposes as_str.
    let branch = podbot::api::BranchName::parse("main")
        .expect("valid branch name");
    let _branch_str: &str = branch.as_str();

    // WorkspacePath is parseable and exposes as_str.
    let workspace = podbot::api::WorkspacePath::parse("/work")
        .expect("valid workspace path");
    let _workspace_str: &str = workspace.as_str();
}
