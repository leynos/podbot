//! Then-step assertions for credential-injection behavioural scenarios.

use rstest_bdd_macros::then;

use super::state::{CredentialInjectionState, FailureKind, InjectionOutcome, StepResult};

#[then("credential injection succeeds")]
fn credential_injection_succeeds(
    credential_injection_state: &CredentialInjectionState,
) -> StepResult<()> {
    let outcome = credential_injection_state
        .outcome
        .get()
        .ok_or_else(|| String::from("credential injection outcome should be set"))?;

    match outcome {
        InjectionOutcome::Success(_) => Ok(()),
        InjectionOutcome::Failed { message, .. } => Err(format!(
            "expected successful credential injection, got: {message}"
        )),
    }
}

#[then("expected container credential paths are {paths}")]
fn expected_container_credential_paths_are(
    credential_injection_state: &CredentialInjectionState,
    paths: String,
) -> StepResult<()> {
    let expected_paths = parse_expected_paths(&paths);
    assert_paths(credential_injection_state, expected_paths.as_slice())
}

#[then("credential upload is attempted once")]
fn credential_upload_is_attempted_once(
    credential_injection_state: &CredentialInjectionState,
) -> StepResult<()> {
    assert_upload_call_count(credential_injection_state, 1)
}

#[then("credential upload is not attempted")]
fn credential_upload_is_not_attempted(
    credential_injection_state: &CredentialInjectionState,
) -> StepResult<()> {
    assert_upload_call_count(credential_injection_state, 0)
}

#[then("credential injection fails with UploadFailed for container {container_id}")]
fn credential_injection_fails_with_upload_failed_for_container(
    credential_injection_state: &CredentialInjectionState,
    container_id: String,
) -> StepResult<()> {
    let outcome = credential_injection_state
        .outcome
        .get()
        .ok_or_else(|| String::from("credential injection outcome should be set"))?;

    match outcome {
        InjectionOutcome::Success(paths) => Err(format!(
            "expected upload failure for container {container_id}, got success with paths: {paths:?}"
        )),
        InjectionOutcome::Failed {
            kind,
            message,
            container_id: observed_container_id,
        } => {
            if kind != FailureKind::UploadFailed {
                return Err(format!(
                    "expected failure kind UploadFailed, got {kind:?}: {message}"
                ));
            }

            if observed_container_id.as_deref() != Some(container_id.as_str()) {
                return Err(format!(
                    "expected failure container id {container_id}, got {observed_container_id:?}"
                ));
            }

            Ok(())
        }
    }
}

fn parse_expected_paths(paths: &str) -> Vec<String> {
    if paths.trim() == "empty" {
        return vec![];
    }

    paths
        .split(',')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(String::from)
        .collect()
}

fn assert_paths(state: &CredentialInjectionState, expected_paths: &[String]) -> StepResult<()> {
    let outcome = state
        .outcome
        .get()
        .ok_or_else(|| String::from("credential injection outcome should be set"))?;

    match outcome {
        InjectionOutcome::Success(observed_paths) => {
            if observed_paths == expected_paths {
                return Ok(());
            }

            Err(format!(
                "expected container credential paths {expected_paths:?}, got {observed_paths:?}"
            ))
        }
        InjectionOutcome::Failed { message, .. } => Err(format!(
            "expected successful credential injection, got: {message}"
        )),
    }
}

fn assert_upload_call_count(state: &CredentialInjectionState, expected: usize) -> StepResult<()> {
    let call_count = state
        .upload_call_count
        .get()
        .ok_or_else(|| String::from("upload call count should be captured"))?;

    if call_count == expected {
        return Ok(());
    }

    Err(format!(
        "expected credential upload call count {expected}, got {call_count}"
    ))
}
