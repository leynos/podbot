//! Integration tests for repository-cloning public value objects.

use podbot::api::{BranchName, RepositoryRef, WorkspacePath};
use rstest::rstest;

#[rstest]
fn repository_clone_values_are_embeddable() -> Result<(), Box<dyn std::error::Error>> {
    let repository = RepositoryRef::parse("leynos/podbot").expect("repository should parse");
    let branch = BranchName::parse("main").expect("branch should parse");
    let workspace = WorkspacePath::parse("/work").expect("workspace should parse");

    if repository.owner() != "leynos" {
        return Err(format!("expected owner leynos, got {}", repository.owner()).into());
    }
    if repository.name() != "podbot" {
        return Err(format!("expected repository podbot, got {}", repository.name()).into());
    }
    if branch.as_str() != "main" {
        return Err(format!("expected branch main, got {}", branch.as_str()).into());
    }
    if workspace.as_str() != "/work" {
        return Err(format!("expected workspace /work, got {}", workspace.as_str()).into());
    }

    Ok(())
}
