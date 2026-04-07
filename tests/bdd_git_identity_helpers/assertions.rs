//! Then step implementations for Git identity scenarios.

use rstest_bdd_macros::then;

use super::state::{GitIdentityState, StepResult};

// =============================================================================
// Helper functions
// =============================================================================

/// Retrieve the outcome from state or return an error.
fn get_outcome(state: &GitIdentityState) -> StepResult<podbot::engine::GitIdentityResult> {
    state
        .outcome
        .get()
        .and_then(|opt| opt.clone())
        .ok_or_else(|| String::from("outcome not captured"))
}

// =============================================================================
// Then steps
// =============================================================================

#[then("Git identity configuration succeeds")]
fn git_identity_configuration_succeeds(
    git_identity_state: &GitIdentityState,
) -> StepResult<()> {
    let _outcome = get_outcome(git_identity_state)?;
    // If we have an outcome, the operation succeeded (errors return Err in When).
    Ok(())
}

#[then("user.name is applied to the container")]
fn user_name_is_applied(
    git_identity_state: &GitIdentityState,
) -> StepResult<()> {
    let outcome = get_outcome(git_identity_state)?;
    if outcome.name_applied() {
        Ok(())
    } else {
        Err(String::from("expected user.name to be applied"))
    }
}

#[then("user.name is not applied to the container")]
fn user_name_is_not_applied(
    git_identity_state: &GitIdentityState,
) -> StepResult<()> {
    let outcome = get_outcome(git_identity_state)?;
    if !outcome.name_applied() {
        Ok(())
    } else {
        Err(String::from("expected user.name to not be applied"))
    }
}

#[then("user.email is applied to the container")]
fn user_email_is_applied(
    git_identity_state: &GitIdentityState,
) -> StepResult<()> {
    let outcome = get_outcome(git_identity_state)?;
    if outcome.email_applied() {
        Ok(())
    } else {
        Err(String::from("expected user.email to be applied"))
    }
}

#[then("user.email is not applied to the container")]
fn user_email_is_not_applied(
    git_identity_state: &GitIdentityState,
) -> StepResult<()> {
    let outcome = get_outcome(git_identity_state)?;
    if !outcome.email_applied() {
        Ok(())
    } else {
        Err(String::from("expected user.email to not be applied"))
    }
}

#[then("no warnings are emitted")]
fn no_warnings_are_emitted(
    git_identity_state: &GitIdentityState,
) -> StepResult<()> {
    let outcome = get_outcome(git_identity_state)?;
    if outcome.warnings().is_empty() {
        Ok(())
    } else {
        Err(format!(
            "expected no warnings, got: {:?}",
            outcome.warnings()
        ))
    }
}

#[then("a warning mentions {text}")]
fn a_warning_mentions(
    git_identity_state: &GitIdentityState,
    text: String,
) -> StepResult<()> {
    let outcome = get_outcome(git_identity_state)?;
    let found = outcome
        .warnings()
        .iter()
        .any(|w| w.contains(&text));

    if found {
        Ok(())
    } else {
        Err(format!(
            "expected a warning containing '{text}', got: {:?}",
            outcome.warnings()
        ))
    }
}
