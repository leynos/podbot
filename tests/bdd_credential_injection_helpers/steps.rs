//! Given/when step definitions for credential-injection behavioural scenarios.

use std::sync::{Arc, Mutex};

use bollard::query_parameters::UploadToContainerOptions;
use mockall::mock;
use podbot::engine::{
    ContainerUploader, CredentialUploadRequest, EngineConnector, UploadToContainerFuture,
};
use podbot::error::{ContainerError, PodbotError};
use rstest_bdd_macros::{given, when};

use super::state::{CredentialInjectionState, FailureKind, HostHome, InjectionOutcome, StepResult};

mock! {
    #[derive(Debug)]
    Uploader {}

    impl ContainerUploader for Uploader {
        fn upload_to_container(
            &self,
            container_id: &str,
            options: Option<UploadToContainerOptions>,
            archive_bytes: Vec<u8>,
        ) -> UploadToContainerFuture<'_>;
    }
}

/// Captures transport invocation metadata during a scenario run.
struct MockCaptureState {
    call_count: Arc<Mutex<usize>>,
}

impl MockCaptureState {
    fn new() -> Self {
        Self {
            call_count: Arc::new(Mutex::new(0_usize)),
        }
    }
}

macro_rules! credential_step {
    ($fn_name:ident, $desc:literal, $dir:literal, $file:literal, $content:literal) => {
        #[given($desc)]
        fn $fn_name(credential_injection_state: &CredentialInjectionState) -> StepResult<()> {
            write_credential_file(credential_injection_state, $dir, $file, $content)
        }
    };
}

credential_step!(
    host_has_claude_credentials,
    "host has Claude credentials",
    ".claude",
    "settings.json",
    "{\"api_key\":\"claude\"}\n"
);

credential_step!(
    host_has_codex_credentials,
    "host has Codex credentials",
    ".codex",
    "auth.toml",
    "token = \"codex\"\n"
);

#[given("copy_claude toggle is enabled")]
fn copy_claude_toggle_is_enabled(credential_injection_state: &CredentialInjectionState) {
    credential_injection_state.copy_claude.set(true);
}

#[given("copy_claude toggle is disabled")]
fn copy_claude_toggle_is_disabled(credential_injection_state: &CredentialInjectionState) {
    credential_injection_state.copy_claude.set(false);
}

#[given("copy_codex toggle is enabled")]
fn copy_codex_toggle_is_enabled(credential_injection_state: &CredentialInjectionState) {
    credential_injection_state.copy_codex.set(true);
}

#[given("copy_codex toggle is disabled")]
fn copy_codex_toggle_is_disabled(credential_injection_state: &CredentialInjectionState) {
    credential_injection_state.copy_codex.set(false);
}

#[given("the credential upload operation fails")]
fn credential_upload_operation_fails(credential_injection_state: &CredentialInjectionState) {
    credential_injection_state.should_fail_upload.set(true);
}

#[when("credential injection is requested")]
fn credential_injection_is_requested(
    credential_injection_state: &CredentialInjectionState,
) -> StepResult<()> {
    let host_home = ensure_host_home(credential_injection_state)?;
    let capture_state = MockCaptureState::new();
    let should_fail_upload = credential_injection_state
        .should_fail_upload
        .get()
        .unwrap_or(false);
    let uploader = setup_mock_uploader(should_fail_upload, &capture_state);

    let container_id = credential_injection_state
        .container_id
        .get()
        .unwrap_or_else(|| String::from("sandbox-credential-test"));
    let copy_claude = credential_injection_state.copy_claude.get().unwrap_or(true);
    let copy_codex = credential_injection_state.copy_codex.get().unwrap_or(true);
    let request =
        CredentialUploadRequest::new(container_id, host_home.path, copy_claude, copy_codex);

    let runtime = tokio::runtime::Runtime::new()
        .map_err(|error| format!("failed to create tokio runtime for scenario: {error}"))?;
    let result = runtime.block_on(EngineConnector::upload_credentials_async(
        &uploader, &request,
    ));

    capture_mock_state(credential_injection_state, &capture_state)?;

    match result {
        Ok(upload_result) => credential_injection_state
            .outcome
            .set(InjectionOutcome::Success(
                upload_result.expected_container_paths().to_vec(),
            )),
        Err(error) => record_failure(credential_injection_state, &error),
    }

    Ok(())
}

fn ensure_host_home(state: &CredentialInjectionState) -> StepResult<HostHome> {
    if let Some(host_home) = state.host_home.get() {
        return Ok(host_home);
    }

    let host_home = HostHome::new()?;
    state.host_home.set(host_home.clone());
    Ok(host_home)
}

fn write_credential_file(
    state: &CredentialInjectionState,
    source_directory: &str,
    filename: &str,
    contents: &str,
) -> StepResult<()> {
    let host_home = ensure_host_home(state)?;
    let source_dir_path = host_home.path.join(source_directory);
    std::fs::create_dir_all(source_dir_path.as_std_path()).map_err(|error| {
        format!("failed to create credential source directory '{source_dir_path}': {error}")
    })?;

    let file_path = source_dir_path.join(filename);
    std::fs::write(file_path.as_std_path(), contents)
        .map_err(|error| format!("failed to write credential file '{file_path}': {error}"))?;

    Ok(())
}

fn setup_mock_uploader(should_fail_upload: bool, capture_state: &MockCaptureState) -> MockUploader {
    let mut uploader = MockUploader::new();
    let call_count_for_closure = Arc::clone(&capture_state.call_count);

    uploader.expect_upload_to_container().returning(
        move |_container_id, _options, _archive_bytes| {
            if let Ok(mut locked) = call_count_for_closure.lock() {
                *locked += 1;
            }

            if should_fail_upload {
                return Box::pin(async { Err(bollard::errors::Error::RequestTimeoutError) });
            }

            Box::pin(async { Ok(()) })
        },
    );

    uploader
}

fn capture_mock_state(
    state: &CredentialInjectionState,
    capture_state: &MockCaptureState,
) -> StepResult<()> {
    let call_count_value = *capture_state
        .call_count
        .lock()
        .map_err(|_| String::from("upload call count mutex is poisoned"))?;

    state.upload_call_count.set(call_count_value);
    Ok(())
}

fn record_failure(state: &CredentialInjectionState, error: &PodbotError) {
    let (kind, container_id) = classify_failure_kind(error);
    state.outcome.set(InjectionOutcome::Failed {
        kind,
        message: error.to_string(),
        container_id,
    });
}

fn classify_failure_kind(error: &PodbotError) -> (FailureKind, Option<String>) {
    match error {
        PodbotError::Container(ContainerError::UploadFailed { container_id, .. }) => {
            (FailureKind::UploadFailed, Some(container_id.clone()))
        }
        _ => (FailureKind::Other, None),
    }
}
