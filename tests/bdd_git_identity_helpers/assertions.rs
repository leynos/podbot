//! Then step definitions for Git identity behavioural scenarios.

use podbot::engine::GitIdentityResult;
use rstest_bdd_macros::then;

use super::state::{GitIdentityState, StepResult};

#[then("git identity result is configured")]
fn git_identity_result_is_configured(git_identity_state: &GitIdentityState) -> StepResult<()> {
    let outcome = git_identity_state
        .outcome
        .get()
        .ok_or_else(|| String::from("outcome not set"))?;

    match outcome {
        Ok(GitIdentityResult::Configured { .. }) => Ok(()),
        Ok(other) => Err(format!("Expected Configured, got {other:?}")),
        Err(e) => Err(format!("Expected success, got error: {e}")),
    }
}

#[then("git identity result is partial")]
fn git_identity_result_is_partial(git_identity_state: &GitIdentityState) -> StepResult<()> {
    let outcome = git_identity_state
        .outcome
        .get()
        .ok_or_else(|| String::from("outcome not set"))?;

    match outcome {
        Ok(GitIdentityResult::Partial { .. }) => Ok(()),
        Ok(other) => Err(format!("Expected Partial, got {other:?}")),
        Err(e) => Err(format!("Expected success, got error: {e}")),
    }
}

#[then("git identity result is none configured")]
fn git_identity_result_is_none_configured(git_identity_state: &GitIdentityState) -> StepResult<()> {
    let outcome = git_identity_state
        .outcome
        .get()
        .ok_or_else(|| String::from("outcome not set"))?;

    match outcome {
        Ok(GitIdentityResult::NoneConfigured { .. }) => Ok(()),
        Ok(other) => Err(format!("Expected NoneConfigured, got {other:?}")),
        Err(e) => Err(format!("Expected success, got error: {e}")),
    }
}

#[then("configured name is {name}")]
fn configured_name_is_name(git_identity_state: &GitIdentityState, name: String) -> StepResult<()> {
    let outcome = git_identity_state
        .outcome
        .get()
        .ok_or_else(|| String::from("outcome not set"))?;

    match outcome {
        Ok(GitIdentityResult::Configured {
            name: actual_name, ..
        }) => {
            if actual_name == &name {
                Ok(())
            } else {
                Err(format!("Expected name '{name}', got '{actual_name}'"))
            }
        }
        Ok(GitIdentityResult::Partial {
            name: Some(actual_name),
            ..
        }) => {
            if actual_name == &name {
                Ok(())
            } else {
                Err(format!("Expected name '{name}', got '{actual_name}'"))
            }
        }
        Ok(other) => Err(format!("Cannot check name on result: {other:?}")),
        Err(e) => Err(format!("Expected success, got error: {e}")),
    }
}

#[then("configured email is {email}")]
fn configured_email_is_email(
    git_identity_state: &GitIdentityState,
    email: String,
) -> StepResult<()> {
    let outcome = git_identity_state
        .outcome
        .get()
        .ok_or_else(|| String::from("outcome not set"))?;

    match outcome {
        Ok(GitIdentityResult::Configured {
            email: actual_email,
            ..
        }) => {
            if actual_email == &email {
                Ok(())
            } else {
                Err(format!("Expected email '{email}', got '{actual_email}'"))
            }
        }
        Ok(GitIdentityResult::Partial {
            email: Some(actual_email),
            ..
        }) => {
            if actual_email == &email {
                Ok(())
            } else {
                Err(format!("Expected email '{email}', got '{actual_email}'"))
            }
        }
        Ok(other) => Err(format!("Cannot check email on result: {other:?}")),
        Err(e) => Err(format!("Expected success, got error: {e}")),
    }
}

#[then("configured name is absent")]
fn configured_name_is_absent(git_identity_state: &GitIdentityState) -> StepResult<()> {
    let outcome = git_identity_state
        .outcome
        .get()
        .ok_or_else(|| String::from("outcome not set"))?;

    match outcome {
        Ok(GitIdentityResult::Partial { name: None, .. }) => Ok(()),
        Ok(GitIdentityResult::NoneConfigured { .. }) => Ok(()),
        Ok(other) => Err(format!("Expected absent name, got {other:?}")),
        Err(e) => Err(format!("Expected success, got error: {e}")),
    }
}

#[then("configured email is absent")]
fn configured_email_is_absent(git_identity_state: &GitIdentityState) -> StepResult<()> {
    let outcome = git_identity_state
        .outcome
        .get()
        .ok_or_else(|| String::from("outcome not set"))?;

    match outcome {
        Ok(GitIdentityResult::Partial { email: None, .. }) => Ok(()),
        Ok(GitIdentityResult::NoneConfigured { .. }) => Ok(()),
        Ok(other) => Err(format!("Expected absent email, got {other:?}")),
        Err(e) => Err(format!("Expected success, got error: {e}")),
    }
}

#[then("warnings include {warning}")]
fn warnings_include_warning(
    git_identity_state: &GitIdentityState,
    warning: String,
) -> StepResult<()> {
    let outcome = git_identity_state
        .outcome
        .get()
        .ok_or_else(|| String::from("outcome not set"))?;

    let warnings = match outcome {
        Ok(GitIdentityResult::Partial { warnings, .. }) => warnings,
        Ok(GitIdentityResult::NoneConfigured { warnings }) => warnings,
        Ok(other) => {
            return Err(format!(
                "Expected result with warnings, got {other:?}"
            ))
        }
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
    let outcome = git_identity_state
        .outcome
        .get()
        .ok_or_else(|| String::from("outcome not set"))?;

    match outcome {
        Err(_) => Ok(()),
        Ok(result) => Err(format!("Expected error, got success: {result:?}")),
    }
}
