//! Repository-cloning API boundary.
//!
//! Validates user-supplied repository and branch values before delegating to
//! the container engine helper that performs the clone inside the sandbox.

use crate::engine::{
    ContainerExecClient, RepositoryCloneRequest, RepositoryCloneResult,
    clone_repository_into_workspace as engine_clone_repository,
};
use crate::error::{ConfigError, Result as PodbotResult};

/// A GitHub repository in `owner/name` form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryRef {
    owner: String,
    name: String,
}

impl RepositoryRef {
    /// Validate and construct a repository reference from `owner/name`.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::InvalidValue` when the input is not exactly two
    /// non-empty path segments separated by a single slash.
    ///
    /// # Examples
    ///
    /// ```
    /// use podbot::api::RepositoryRef;
    ///
    /// let repo = RepositoryRef::parse("leynos/podbot")?;
    /// assert_eq!(repo.owner(), "leynos");
    /// assert_eq!(repo.name(), "podbot");
    /// # Ok::<(), podbot::error::PodbotError>(())
    /// ```
    pub fn parse(value: impl AsRef<str>) -> PodbotResult<Self> {
        let trimmed = value.as_ref().trim();
        let Some((owner, name)) = trimmed.split_once('/') else {
            return Err(invalid_repository_ref());
        };

        if owner.is_empty() || name.is_empty() || name.contains('/') {
            return Err(invalid_repository_ref());
        }

        Ok(Self {
            owner: String::from(owner),
            name: String::from(name),
        })
    }

    /// Return the repository owner segment.
    #[must_use]
    pub fn owner(&self) -> &str {
        &self.owner
    }

    /// Return the repository name segment.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

fn invalid_repository_ref() -> crate::error::PodbotError {
    ConfigError::InvalidValue {
        field: String::from("repo"),
        reason: String::from("expected repository in owner/name form"),
    }
    .into()
}

/// A required Git branch name supplied by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchName(String);

impl BranchName {
    /// Validate and construct a branch name.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::MissingRequired` when the branch is empty after
    /// trimming whitespace.
    ///
    /// # Examples
    ///
    /// ```
    /// use podbot::api::BranchName;
    ///
    /// let branch = BranchName::parse("main")?;
    /// assert_eq!(branch.as_str(), "main");
    /// # Ok::<(), podbot::error::PodbotError>(())
    /// ```
    pub fn parse(value: impl AsRef<str>) -> PodbotResult<Self> {
        let trimmed = value.as_ref().trim();

        if trimmed.is_empty() {
            return Err(ConfigError::MissingRequired {
                field: String::from("branch"),
            }
            .into());
        }

        Ok(Self(String::from(trimmed)))
    }

    /// Return the branch as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Parameters for repository cloning through the public API.
pub struct CloneRepositoryParams<'a, C: ContainerExecClient> {
    /// Pre-connected container engine client.
    pub client: &'a C,
    /// Target container identifier.
    pub container_id: &'a str,
    /// Repository in validated `owner/name` form.
    pub repository: RepositoryRef,
    /// Required branch to clone and verify.
    pub branch: BranchName,
    /// Exact workspace path inside the container.
    pub workspace_base_dir: &'a str,
    /// Path to the `GIT_ASKPASS` helper inside the container.
    pub askpass_path: &'a str,
    /// Tokio runtime handle for blocking execution.
    pub runtime_handle: &'a tokio::runtime::Handle,
}

/// Clone a repository into the configured sandbox workspace.
///
/// # Errors
///
/// Returns `ContainerError::ExecFailed` when the clone or verification command
/// exits with a non-zero status, and returns validation errors when required
/// values are missing.
pub fn clone_repository_into_workspace<C: ContainerExecClient + Sync>(
    params: &CloneRepositoryParams<'_, C>,
) -> PodbotResult<RepositoryCloneResult> {
    let request = RepositoryCloneRequest {
        container_id: params.container_id,
        repository_owner: params.repository.owner(),
        repository_name: params.repository.name(),
        branch: params.branch.as_str(),
        workspace_base_dir: params.workspace_base_dir,
        askpass_path: params.askpass_path,
    };

    engine_clone_repository(params.runtime_handle, params.client, &request)
}

#[cfg(test)]
mod tests {
    use super::{BranchName, RepositoryRef};
    use crate::error::{ConfigError, PodbotError};
    use rstest::rstest;

    #[rstest]
    #[case("leynos/podbot", "leynos", "podbot")]
    #[case("  leynos/podbot  ", "leynos", "podbot")]
    fn repository_ref_accepts_owner_name(
        #[case] input: &str,
        #[case] owner: &str,
        #[case] name: &str,
    ) {
        let repo = RepositoryRef::parse(input).expect("valid repository should parse");

        assert_eq!(repo.owner(), owner);
        assert_eq!(repo.name(), name);
    }

    #[rstest]
    #[case("")]
    #[case("leynos")]
    #[case("/podbot")]
    #[case("leynos/")]
    #[case("leynos/podbot/extra")]
    fn repository_ref_rejects_malformed_values(#[case] input: &str) {
        let result = RepositoryRef::parse(input);

        assert!(matches!(
            result,
            Err(PodbotError::Config(ConfigError::InvalidValue { .. }))
        ));
    }

    #[rstest]
    #[case("main", "main")]
    #[case(" feature/repo-clone ", "feature/repo-clone")]
    fn branch_name_accepts_non_empty_values(#[case] input: &str, #[case] expected: &str) {
        let branch = BranchName::parse(input).expect("valid branch should parse");

        assert_eq!(branch.as_str(), expected);
    }

    #[rstest]
    #[case("")]
    #[case("   ")]
    fn branch_name_rejects_empty_values(#[case] input: &str) {
        let result = BranchName::parse(input);

        assert!(matches!(
            result,
            Err(PodbotError::Config(ConfigError::MissingRequired { .. }))
        ));
    }
}
