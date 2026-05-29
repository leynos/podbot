//! Repository-cloning API boundary.
//!
//! Validates user-supplied repository, branch, and workspace values before
//! internal engine code performs the clone inside the sandbox.

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

        if has_invalid_segments(owner, name) {
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

fn has_invalid_segments(owner: &str, name: &str) -> bool {
    owner.is_empty()
        || name.is_empty()
        || name.contains('/')
        || owner != owner.trim()
        || name != name.trim()
}

/// Trim `value` and reject it if empty, returning the trimmed string.
///
/// `field` is used verbatim in the `ConfigError::MissingRequired` payload.
fn parse_required_trimmed(value: impl AsRef<str>, field: &'static str) -> PodbotResult<String> {
    let trimmed = value.as_ref().trim();
    if trimmed.is_empty() {
        return Err(ConfigError::MissingRequired {
            field: String::from(field),
        }
        .into());
    }
    Ok(String::from(trimmed))
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
        parse_required_trimmed(value, "branch").map(Self)
    }

    /// Return the branch as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Absolute workspace path inside the sandbox container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePath(String);

impl WorkspacePath {
    /// Validate and construct an absolute workspace path.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::InvalidValue` when the path is empty or relative.
    ///
    /// # Examples
    ///
    /// ```
    /// use podbot::api::WorkspacePath;
    ///
    /// let workspace = WorkspacePath::parse("/work")?;
    /// assert_eq!(workspace.as_str(), "/work");
    /// # Ok::<(), podbot::error::PodbotError>(())
    /// ```
    pub fn parse(value: impl AsRef<str>) -> PodbotResult<Self> {
        let trimmed = value.as_ref().trim();

        if trimmed.is_empty() || !trimmed.starts_with('/') {
            return Err(ConfigError::InvalidValue {
                field: String::from("workspace.base_dir"),
                reason: String::from("expected an absolute container path"),
            }
            .into());
        }

        Ok(Self(String::from(trimmed)))
    }

    /// Return the workspace path as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// In-container path to the `GIT_ASKPASS` helper.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(any(feature = "internal", test))]
pub struct AskpassPath(String);

#[cfg(any(feature = "internal", test))]
impl AskpassPath {
    /// Validate and construct an askpass helper path.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::MissingRequired` when the path is empty after
    /// trimming whitespace.
    pub fn parse(value: impl AsRef<str>) -> PodbotResult<Self> {
        parse_required_trimmed(value, "git.askpass_path").map(Self)
    }

    /// Return the path as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{AskpassPath, BranchName, RepositoryRef, WorkspacePath};
    use crate::error::{ConfigError, PodbotError};
    use proptest::prelude::*;
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
    #[case("leynos /podbot")]
    #[case("leynos/ podbot")]
    #[case("leynos\t/podbot")]
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

    #[rstest]
    #[case("/work", "/work")]
    #[case(" /workspace/project ", "/workspace/project")]
    fn workspace_path_accepts_absolute_values(#[case] input: &str, #[case] expected: &str) {
        let workspace = WorkspacePath::parse(input).expect("absolute workspace should parse");

        assert_eq!(workspace.as_str(), expected);
    }

    #[rstest]
    #[case("/usr/local/bin/git-askpass", "/usr/local/bin/git-askpass")]
    #[case("  /helper  ", "/helper")]
    fn askpass_path_accepts_non_empty_values(#[case] input: &str, #[case] expected: &str) {
        let askpass = AskpassPath::parse(input).expect("valid askpass path should parse");

        assert_eq!(askpass.as_str(), expected);
    }

    #[rstest]
    #[case("")]
    #[case("   ")]
    fn askpass_path_rejects_empty_values(#[case] input: &str) {
        let result = AskpassPath::parse(input);

        assert!(matches!(
            result,
            Err(PodbotError::Config(ConfigError::MissingRequired { .. }))
        ));
    }

    #[rstest]
    #[case("")]
    #[case("   ")]
    #[case("work")]
    #[case("./work")]
    fn workspace_path_rejects_relative_values(#[case] input: &str) {
        let result = WorkspacePath::parse(input);

        assert!(matches!(
            result,
            Err(PodbotError::Config(ConfigError::InvalidValue { .. }))
        ));
    }

    proptest! {
        #[test]
        fn repository_ref_property_matches_segment_invariants(input in "\\PC*") {
            let trimmed = input.trim();
            let parsed = RepositoryRef::parse(&input);
            let expected = trimmed
                .split_once('/')
                .filter(|(owner, name)| {
                    !owner.is_empty()
                        && !name.is_empty()
                        && !name.contains('/')
                        && *owner == owner.trim()
                        && *name == name.trim()
                });

            match (parsed, expected) {
                (Ok(repo), Some((owner, name))) => {
                    prop_assert_eq!(repo.owner(), owner);
                    prop_assert_eq!(repo.name(), name);
                }
                (Err(PodbotError::Config(ConfigError::InvalidValue { .. })), None) => {}
                (other, expected_segments) => {
                    prop_assert!(
                        false,
                        "unexpected repository parse result {other:?} for expected {expected_segments:?}"
                    );
                }
            }
        }

        #[test]
        fn branch_name_property_rejects_only_trimmed_empty(input in "\\PC*") {
            let parsed = BranchName::parse(&input);
            let trimmed = input.trim();

            if trimmed.is_empty() {
                match parsed {
                    Err(PodbotError::Config(ConfigError::MissingRequired { .. })) => {}
                    other => prop_assert!(
                        false,
                        "unexpected branch parse result {other:?} for empty input"
                    ),
                }
            } else {
                match parsed {
                    Ok(branch) => prop_assert_eq!(branch.as_str(), trimmed),
                    other => prop_assert!(
                        false,
                        "unexpected branch parse result {other:?} for non-empty input"
                    ),
                }
            }
        }

        #[test]
        fn askpass_path_property_rejects_only_trimmed_empty(input in "\\PC*") {
            let parsed = AskpassPath::parse(&input);
            let trimmed = input.trim();

            if trimmed.is_empty() {
                match parsed {
                    Err(PodbotError::Config(ConfigError::MissingRequired { .. })) => {}
                    other => prop_assert!(
                        false,
                        "unexpected askpass parse result {other:?} for empty input"
                    ),
                }
            } else {
                match parsed {
                    Ok(askpass) => prop_assert_eq!(askpass.as_str(), trimmed),
                    other => prop_assert!(
                        false,
                        "unexpected askpass parse result {other:?} for non-empty input"
                    ),
                }
            }
        }

        #[test]
        fn workspace_path_property_accepts_only_absolute_non_empty(s in "\\PC*") {
            let trimmed = s.trim();
            let result = WorkspacePath::parse(&s);
            if trimmed.is_empty() || !trimmed.starts_with('/') {
                prop_assert!(
                    result.is_err(),
                    "expected WorkspacePath::parse to reject {:?} but it succeeded",
                    s
                );
            } else {
                match result {
                    Ok(_) => {}
                    Err(error) => prop_assert!(
                        false,
                        "expected WorkspacePath::parse to accept {:?} but it failed: {:?}",
                        s,
                        error
                    ),
                }
            }
        }
    }
}
