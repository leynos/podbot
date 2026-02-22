//! Behavioural tests for credential injection into sandbox containers.

mod bdd_credential_injection_helpers;

pub use bdd_credential_injection_helpers::{CredentialInjectionState, credential_injection_state};
use rstest_bdd_macros::scenario;

#[scenario(
    path = "tests/features/credential_injection.feature",
    name = "Upload selected credentials when both toggles are enabled"
)]
fn upload_selected_credentials_when_both_toggles_are_enabled(
    credential_injection_state: CredentialInjectionState,
) {
    let _ = credential_injection_state;
}

#[scenario(
    path = "tests/features/credential_injection.feature",
    name = "Upload only Claude credentials when Codex toggle is disabled"
)]
fn upload_only_claude_credentials_when_codex_toggle_is_disabled(
    credential_injection_state: CredentialInjectionState,
) {
    let _ = credential_injection_state;
}

#[scenario(
    path = "tests/features/credential_injection.feature",
    name = "Upload only Codex credentials when Claude toggle is disabled"
)]
fn upload_only_codex_credentials_when_claude_toggle_is_disabled(
    credential_injection_state: CredentialInjectionState,
) {
    let _ = credential_injection_state;
}

#[scenario(
    path = "tests/features/credential_injection.feature",
    name = "Missing source directory is skipped"
)]
fn missing_source_directory_is_skipped(credential_injection_state: CredentialInjectionState) {
    let _ = credential_injection_state;
}

#[scenario(
    path = "tests/features/credential_injection.feature",
    name = "No upload occurs when both toggles are disabled"
)]
fn no_upload_occurs_when_both_toggles_are_disabled(
    credential_injection_state: CredentialInjectionState,
) {
    let _ = credential_injection_state;
}

#[scenario(
    path = "tests/features/credential_injection.feature",
    name = "Upload failures map to UploadFailed"
)]
fn upload_failures_map_to_upload_failed(credential_injection_state: CredentialInjectionState) {
    let _ = credential_injection_state;
}
