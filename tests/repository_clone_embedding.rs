//! Integration tests for repository-cloning public value objects.

use podbot::api::{BranchName, RepositoryRef, WorkspacePath};
use rstest::rstest;

#[rstest]
fn repository_clone_values_are_embeddable() {
    let repository = RepositoryRef::parse("leynos/podbot").expect("repository should parse");
    let branch = BranchName::parse("main").expect("branch should parse");
    let workspace = WorkspacePath::parse("/work").expect("workspace should parse");

    assert_eq!(repository.owner(), "leynos");
    assert_eq!(repository.name(), "podbot");
    assert_eq!(branch.as_str(), "main");
    assert_eq!(workspace.as_str(), "/work");
}
