//! Then step definitions for Git identity behavioural scenarios.

use podbot::engine::GitIdentityResult;
use rstest_bdd_macros::then;

use super::state::{GitIdentityState, StepResult};

fn assert_ok_variant(
    outcome: &Result<GitIdentityResult, String>,
    check: impl Fn(&GitIdentityResult) -> bool,
    expected: &str,
) -> StepResult<()> {
    match outcome {
        Ok(r) if check(r) => Ok(()),
        Ok(other) => Err(format!("Expected {expected}, got {other:?}")),
        Err(e) => Err(format!("Expected success, got error: {e}")),
    }
}

fn assert_field_value<'a>(
    outcome: &'a Result<GitIdentityResult, String>,
    extractor: impl Fn(&'a GitIdentityResult) -> Option<&'a str>,
    expected: &str,
    field_name: &str,
) -> StepResult<()> {
    match outcome {
        Ok(result) => match extractor(result) {
            Some(actual) if actual == expected => Ok(()),
            Some(actual) => Err(format!(
                "Expected {field_name} '{expected}', got '{actual}'"
            )),
            None => Err(format!("Cannot check {field_name} on result: {result:?}")),
        },
        Err(e) => Err(format!("Expected success, got error: {e}")),
    }
}

fn assert_field_absent(
    outcome: &Result<GitIdentityResult, String>,
    field_present: impl Fn(&GitIdentityResult) -> bool,
    field_name: &str,
) -> StepResult<()> {
    match outcome {
        Ok(r) if !field_present(r) => Ok(()),
        Ok(other) => Err(format!("Expected absent {field_name}, got {other:?}")),
        Err(e) => Err(format!("Expected success, got error: {e}")),
    }
}

#[then("git identity result is configured")]
fn git_identity_result_is_configured(git_identity_state: &GitIdentityState) -> StepResult<()> {
    git_identity_state
        .outcome
        .with_ref(|outcome| {
            let converted = match outcome {
                Ok(r) => Ok(r.clone()),
                Err(e) => Err(format!("{e}")),
            };
            assert_ok_variant(
                &converted,
                |r| matches!(r, GitIdentityResult::Configured { .. }),
                "Configured",
            )
        })
        .ok_or_else(|| String::from("outcome not set"))?
}

#[then("git identity result is partial")]
fn git_identity_result_is_partial(git_identity_state: &GitIdentityState) -> StepResult<()> {
    git_identity_state
        .outcome
        .with_ref(|outcome| {
            let converted = match outcome {
                Ok(r) => Ok(r.clone()),
                Err(e) => Err(format!("{e}")),
            };
            assert_ok_variant(
                &converted,
                |r| matches!(r, GitIdentityResult::Partial { .. }),
                "Partial",
            )
        })
        .ok_or_else(|| String::from("outcome not set"))?
}

#[then("git identity result is none configured")]
fn git_identity_result_is_none_configured(git_identity_state: &GitIdentityState) -> StepResult<()> {
    git_identity_state
        .outcome
        .with_ref(|outcome| {
            let converted = match outcome {
                Ok(r) => Ok(r.clone()),
                Err(e) => Err(format!("{e}")),
            };
            assert_ok_variant(
                &converted,
                |r| matches!(r, GitIdentityResult::NoneConfigured { .. }),
                "NoneConfigured",
            )
        })
        .ok_or_else(|| String::from("outcome not set"))?
}

/// Checks configured name matches expected value.
/// "absent" is treated specially to check for None.
#[then("configured name is {word}")]
fn configured_name_is(git_identity_state: &GitIdentityState, word: String) -> StepResult<()> {
    git_identity_state
        .outcome
        .with_ref(|outcome| {
            let converted = match outcome {
                Ok(r) => Ok(r.clone()),
                Err(e) => Err(format!("{e}")),
            };

            if word == "absent" {
                assert_field_absent(
                    &converted,
                    |r| {
                        matches!(
                            r,
                            GitIdentityResult::Configured { .. }
                                | GitIdentityResult::Partial { name: Some(_), .. }
                        )
                    },
                    "name",
                )
            } else {
                assert_field_value(
                    &converted,
                    |r| match r {
                        GitIdentityResult::Configured { name, .. } => Some(name.as_str()),
                        GitIdentityResult::Partial { name: Some(n), .. } => Some(n.as_str()),
                        _ => None,
                    },
                    &word,
                    "name",
                )
            }
        })
        .ok_or_else(|| String::from("outcome not set"))?
}

/// Checks configured email matches expected value.
/// "absent" is treated specially to check for None.
#[then("configured email is {word}")]
fn configured_email_is(git_identity_state: &GitIdentityState, word: String) -> StepResult<()> {
    git_identity_state
        .outcome
        .with_ref(|outcome| {
            let converted = match outcome {
                Ok(r) => Ok(r.clone()),
                Err(e) => Err(format!("{e}")),
            };

            if word == "absent" {
                assert_field_absent(
                    &converted,
                    |r| {
                        matches!(
                            r,
                            GitIdentityResult::Configured { .. }
                                | GitIdentityResult::Partial { email: Some(_), .. }
                        )
                    },
                    "email",
                )
            } else {
                assert_field_value(
                    &converted,
                    |r| match r {
                        GitIdentityResult::Configured { email, .. } => Some(email.as_str()),
                        GitIdentityResult::Partial { email: Some(e), .. } => Some(e.as_str()),
                        _ => None,
                    },
                    &word,
                    "email",
                )
            }
        })
        .ok_or_else(|| String::from("outcome not set"))?
}

#[then("warnings include {word}")]
fn warnings_include_warning(git_identity_state: &GitIdentityState, word: String) -> StepResult<()> {
    git_identity_state
        .outcome
        .with_ref(|outcome| {
            let converted = match outcome {
                Ok(r) => Ok(r.clone()),
                Err(e) => Err(format!("{e}")),
            };
            let warnings = match &converted {
                Ok(
                    GitIdentityResult::Partial { warnings, .. }
                    | GitIdentityResult::NoneConfigured { warnings },
                ) => warnings,
                Ok(other) => return Err(format!("Expected result with warnings, got {other:?}")),
                Err(e) => return Err(format!("Expected success, got error: {e}")),
            };

            if warnings.contains(&word) {
                Ok(())
            } else {
                Err(format!(
                    "Expected warning '{word}' not found in {warnings:?}"
                ))
            }
        })
        .ok_or_else(|| String::from("outcome not set"))?
}

#[then("git identity configuration fails with an exec error")]
fn git_identity_configuration_fails_with_exec_error(
    git_identity_state: &GitIdentityState,
) -> StepResult<()> {
    git_identity_state
        .outcome
        .with_ref(|outcome| {
            outcome.as_ref().map_or(Ok(()), |result| {
                Err(format!("Expected error, got success: {result:?}"))
            })
        })
        .ok_or_else(|| String::from("outcome not set"))?
}
