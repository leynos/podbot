//! Then step definitions for Git identity behavioural scenarios.

use podbot::engine::GitIdentityResult;
use podbot::error::PodbotError;
use rstest_bdd_macros::then;

use super::state::{GitIdentityState, StepResult};

fn get_outcome(state: &GitIdentityState) -> StepResult<Result<GitIdentityResult, String>> {
    let result = state
        .outcome
        .get()
        .ok_or_else(|| String::from("outcome not set"))?;

    match result {
        Ok(identity_result) => Ok(Ok(identity_result)),
        Err(e) => Ok(Err(format!("{e}"))),
    }
}

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
    assert_ok_variant(
        get_outcome(git_identity_state)?,
        |r| matches!(r, GitIdentityResult::Configured { .. }),
        "Configured",
    )
}

#[then("git identity result is partial")]
fn git_identity_result_is_partial(git_identity_state: &GitIdentityState) -> StepResult<()> {
    assert_ok_variant(
        get_outcome(git_identity_state)?,
        |r| matches!(r, GitIdentityResult::Partial { .. }),
        "Partial",
    )
}

#[then("git identity result is none configured")]
fn git_identity_result_is_none_configured(git_identity_state: &GitIdentityState) -> StepResult<()> {
    assert_ok_variant(
        get_outcome(git_identity_state)?,
        |r| matches!(r, GitIdentityResult::NoneConfigured { .. }),
        "NoneConfigured",
    )
}

#[then("configured name is {name}")]
fn configured_name_is_name(git_identity_state: &GitIdentityState, name: String) -> StepResult<()> {
    assert_field_value(
        get_outcome(git_identity_state)?,
        |r| match r {
            GitIdentityResult::Configured { name, .. } => Some(name.as_str()),
            GitIdentityResult::Partial { name: Some(n), .. } => Some(n.as_str()),
            _ => None,
        },
        &name,
        "name",
    )
}

#[then("configured email is {email}")]
fn configured_email_is_email(
    git_identity_state: &GitIdentityState,
    email: String,
) -> StepResult<()> {
    assert_field_value(
        get_outcome(git_identity_state)?,
        |r| match r {
            GitIdentityResult::Configured { email, .. } => Some(email.as_str()),
            GitIdentityResult::Partial { email: Some(e), .. } => Some(e.as_str()),
            _ => None,
        },
        &email,
        "email",
    )
}

#[then("configured name is absent")]
fn configured_name_is_absent(git_identity_state: &GitIdentityState) -> StepResult<()> {
    assert_field_absent(
        get_outcome(git_identity_state)?,
        |r| {
            matches!(
                r,
                GitIdentityResult::Configured { .. }
                    | GitIdentityResult::Partial { name: Some(_), .. }
            )
        },
        "name",
    )
}

#[then("configured email is absent")]
fn configured_email_is_absent(git_identity_state: &GitIdentityState) -> StepResult<()> {
    assert_field_absent(
        get_outcome(git_identity_state)?,
        |r| {
            matches!(
                r,
                GitIdentityResult::Configured { .. }
                    | GitIdentityResult::Partial { email: Some(_), .. }
            )
        },
        "email",
    )
}

#[then("warnings include {warning}")]
fn warnings_include_warning(
    git_identity_state: &GitIdentityState,
    warning: String,
) -> StepResult<()> {
    let warnings = match get_outcome(git_identity_state)? {
        Ok(GitIdentityResult::Partial { warnings, .. }) => warnings,
        Ok(GitIdentityResult::NoneConfigured { warnings }) => warnings,
        Ok(other) => return Err(format!("Expected result with warnings, got {other:?}")),
        Err(e) => return Err(format!("Expected success, got error: {e}")),
    };

    if warnings.contains(&warning) {
        Ok(())
    } else {
        Err(format!(
            "Expected warning '{warning}' not found in {warnings:?}"
        ))
    }
}

#[then("git identity configuration fails with an exec error")]
fn git_identity_configuration_fails_with_exec_error(
    git_identity_state: &GitIdentityState,
) -> StepResult<()> {
    match get_outcome(git_identity_state)? {
        Err(_) => Ok(()),
        Ok(result) => Err(format!("Expected error, got success: {result:?}")),
    }
}
