//! Unit tests for credential upload control flow.

use rstest::rstest;

use super::tar_archive::parse_archive_entries;
use super::*;
use crate::config::{AppConfig, CredsConfig};
use crate::error::{ContainerError, PodbotError};

#[rstest]
fn upload_credentials_uploads_selected_sources_and_reports_paths() {
    let (_tmp, host_home) = host_home_dir();

    let claude_dir = host_home.join(".claude");
    create_dir(&claude_dir);
    write_file(
        &claude_dir.join("settings.json"),
        "{\"api_key\":\"claude\"}",
    );

    let codex_dir = host_home.join(".codex");
    create_dir(&codex_dir);
    write_file(&codex_dir.join("auth.toml"), "token = \"codex\"");

    let config = AppConfig {
        creds: CredsConfig {
            copy_claude: true,
            copy_codex: true,
        },
        ..AppConfig::default()
    };

    let request = CredentialUploadRequest::from_app_config("container-123", host_home, &config);
    let (uploader, captured) = successful_uploader();

    let result = runtime()
        .block_on(EngineConnector::upload_credentials_async(
            &uploader, &request,
        ))
        .expect("upload should succeed");

    assert_eq!(
        result.expected_container_paths(),
        &[String::from("/root/.claude"), String::from("/root/.codex")]
    );

    let captured_call = captured_call(&captured);
    assert_eq!(captured_call.call_count, 1);
    assert_eq!(captured_call.container_id.as_deref(), Some("container-123"));

    let options = captured_call
        .options
        .expect("upload options should be captured");
    assert_eq!(options.path, "/root");

    assert!(!captured_call.archive_bytes.is_empty());
}

#[rstest]
#[case::only_claude(true, false, vec!["/root/.claude"], ".claude/")]
#[case::only_codex(false, true, vec!["/root/.codex"], ".codex/")]
fn upload_credentials_respects_copy_toggles(
    #[case] copy_claude: bool,
    #[case] copy_codex: bool,
    #[case] expected_paths: Vec<&str>,
    #[case] expected_archive_entry: &str,
) {
    let (_tmp, host_home) = host_home_dir();

    let claude_dir = host_home.join(".claude");
    create_dir(&claude_dir);
    write_file(&claude_dir.join("settings.json"), "{}\n");

    let codex_dir = host_home.join(".codex");
    create_dir(&codex_dir);
    write_file(&codex_dir.join("auth.toml"), "token = \"x\"\n");

    let request = CredentialUploadRequest::new("container-456", host_home, copy_claude, copy_codex);
    let (uploader, captured) = successful_uploader();

    let result = runtime()
        .block_on(EngineConnector::upload_credentials_async(
            &uploader, &request,
        ))
        .expect("upload should succeed");

    let expected_paths_values: Vec<String> = expected_paths.into_iter().map(String::from).collect();
    assert_eq!(
        result.expected_container_paths(),
        expected_paths_values.as_slice()
    );

    let captured_call = captured_call(&captured);
    assert_eq!(captured_call.call_count, 1);

    let entries = parse_archive_entries(&captured_call.archive_bytes)
        .expect("archive parsing should succeed");
    let entry_paths: Vec<String> = entries.into_iter().map(|entry| entry.path).collect();

    assert!(
        entry_paths
            .iter()
            .any(|path| path == expected_archive_entry)
    );

    if !copy_claude {
        assert!(!entry_paths.iter().any(|path| path.starts_with(".claude/")));
    }

    if !copy_codex {
        assert!(!entry_paths.iter().any(|path| path.starts_with(".codex/")));
    }
}

#[rstest]
fn upload_credentials_skips_missing_sources_without_error() {
    let (_tmp, host_home) = host_home_dir();

    let codex_dir = host_home.join(".codex");
    create_dir(&codex_dir);
    write_file(&codex_dir.join("auth.toml"), "token = \"codex\"");

    let request = CredentialUploadRequest::new("container-789", host_home, true, true);
    let (uploader, captured) = successful_uploader();

    let result = runtime()
        .block_on(EngineConnector::upload_credentials_async(
            &uploader, &request,
        ))
        .expect("upload should succeed when one source is missing");

    assert_eq!(
        result.expected_container_paths(),
        &[String::from("/root/.codex")]
    );
    assert_eq!(captured_call(&captured).call_count, 1);
}

#[rstest]
fn upload_credentials_is_noop_when_all_sources_missing_or_disabled() {
    let (_tmp, host_home) = host_home_dir();

    let request = CredentialUploadRequest::new("container-empty", host_home, true, true);
    let (uploader, captured) = successful_uploader();

    let result = runtime()
        .block_on(EngineConnector::upload_credentials_async(
            &uploader, &request,
        ))
        .expect("upload should succeed as a no-op");

    assert!(result.expected_container_paths().is_empty());
    assert_eq!(captured_call(&captured).call_count, 0);
}

#[rstest]
fn upload_credentials_maps_engine_failures_to_upload_failed() {
    let (_tmp, host_home) = host_home_dir();

    let claude_dir = host_home.join(".claude");
    create_dir(&claude_dir);
    write_file(&claude_dir.join("settings.json"), "{}\n");

    let request = CredentialUploadRequest::new("container-failed", host_home, true, false);
    let (uploader, _) = failing_uploader(bollard::errors::Error::RequestTimeoutError);

    let error = runtime()
        .block_on(EngineConnector::upload_credentials_async(
            &uploader, &request,
        ))
        .expect_err("upload should fail when engine upload fails");

    assert!(
        matches!(
            error,
            PodbotError::Container(ContainerError::UploadFailed {
                ref container_id,
                ref message,
            }) if container_id == "container-failed" && message.contains("Timeout error")
        ),
        "expected upload-failed mapping, got: {error:?}"
    );
}

#[rstest]
fn upload_credentials_errors_when_selected_source_is_not_directory() {
    let (_tmp, host_home) = host_home_dir();

    write_file(&host_home.join(".claude"), "not-a-directory");

    let request = CredentialUploadRequest::new("container-invalid", host_home, true, false);
    let (uploader, captured) = successful_uploader();

    let error = runtime()
        .block_on(EngineConnector::upload_credentials_async(
            &uploader, &request,
        ))
        .expect_err("upload should fail when selected source is not a directory");

    assert!(
        matches!(
            error,
            PodbotError::Container(ContainerError::UploadFailed {
                ref container_id,
                ref message,
            }) if container_id == "container-invalid"
                && message.contains("exists but is not a directory")
        ),
        "expected upload-failed invalid-source mapping, got: {error:?}"
    );
    assert_eq!(captured_call(&captured).call_count, 0);
}

#[rstest]
fn upload_credentials_errors_when_host_home_directory_cannot_be_opened() {
    let (_tmp, host_home) = host_home_dir();
    let missing_home = host_home.join("missing-home-directory");

    let request = CredentialUploadRequest::new("container-missing-home", missing_home, true, true);
    let (uploader, captured) = successful_uploader();

    let error = runtime()
        .block_on(EngineConnector::upload_credentials_async(
            &uploader, &request,
        ))
        .expect_err("upload should fail when host home cannot be opened");

    assert!(
        matches!(
            error,
            PodbotError::Container(ContainerError::UploadFailed {
                ref container_id,
                ref message,
            }) if container_id == "container-missing-home"
                && message.contains("failed to open host home directory")
        ),
        "expected upload-failed host-home mapping, got: {error:?}"
    );
    assert_eq!(captured_call(&captured).call_count, 0);
}
