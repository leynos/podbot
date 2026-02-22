//! Unit tests for credential upload control flow.

use cap_std::{ambient_authority, fs_utf8::Dir};
use rstest::rstest;

use super::tar_archive::parse_archive_entries;
use super::*;
use crate::config::{AppConfig, CredsConfig};
use crate::error::{ContainerError, PodbotError};

struct ToggleCase {
    copy_claude: bool,
    copy_codex: bool,
    expected_paths: Vec<&'static str>,
    expected_archive_entry: &'static str,
}

#[rstest]
fn upload_credentials_uploads_selected_sources_and_reports_paths(
    runtime: std::io::Result<tokio::runtime::Runtime>,
    host_home_dir: std::io::Result<(tempfile::TempDir, camino::Utf8PathBuf)>,
) -> std::io::Result<()> {
    let runtime_handle = runtime?;
    let (_tmp, host_home) = host_home_dir?;

    let claude_dir = host_home.join(".claude");
    create_dir(&claude_dir)?;
    write_file(
        &claude_dir.join("settings.json"),
        "{\"api_key\":\"claude\"}",
    )?;

    let codex_dir = host_home.join(".codex");
    create_dir(&codex_dir)?;
    write_file(&codex_dir.join("auth.toml"), "token = \"codex\"")?;

    let config = AppConfig {
        creds: CredsConfig {
            copy_claude: true,
            copy_codex: true,
        },
        ..AppConfig::default()
    };

    let request = CredentialUploadRequest::from_app_config("container-123", host_home, &config);
    let (uploader, captured) = successful_uploader();

    let result = runtime_handle
        .block_on(EngineConnector::upload_credentials_async(
            &uploader, &request,
        ))
        .map_err(|error| io_error(format!("upload should succeed: {error}")))?;

    let expected_paths = [String::from("/root/.claude"), String::from("/root/.codex")];
    ensure(
        result.expected_container_paths() == expected_paths.as_slice(),
        format!(
            "expected uploaded paths {expected_paths:?}, got {:?}",
            result.expected_container_paths()
        ),
    )?;

    let captured_call = captured_call(&captured)?;
    ensure(
        captured_call.call_count == 1,
        format!("expected one upload call, got {}", captured_call.call_count),
    )?;
    ensure(
        captured_call.container_id.as_deref() == Some("container-123"),
        format!(
            "expected container id Some(\"container-123\"), got {:?}",
            captured_call.container_id
        ),
    )?;

    let options = captured_call
        .options
        .ok_or_else(|| io_error("upload options should be captured"))?;
    ensure(
        options.path == "/root",
        format!("expected upload path '/root', got '{}'", options.path),
    )?;

    ensure(
        !captured_call.archive_bytes.is_empty(),
        "expected non-empty archive bytes",
    )?;

    Ok(())
}

#[rstest]
#[case::only_claude(ToggleCase {
    copy_claude: true,
    copy_codex: false,
    expected_paths: vec!["/root/.claude"],
    expected_archive_entry: ".claude/",
})]
#[case::only_codex(ToggleCase {
    copy_claude: false,
    copy_codex: true,
    expected_paths: vec!["/root/.codex"],
    expected_archive_entry: ".codex/",
})]
fn upload_credentials_respects_copy_toggles(
    #[case] case: ToggleCase,
    runtime: std::io::Result<tokio::runtime::Runtime>,
    host_home_dir: std::io::Result<(tempfile::TempDir, camino::Utf8PathBuf)>,
) -> std::io::Result<()> {
    let runtime_handle = runtime?;
    let (_tmp, host_home) = host_home_dir?;

    let claude_dir = host_home.join(".claude");
    create_dir(&claude_dir)?;
    write_file(&claude_dir.join("settings.json"), "{}\n")?;

    let codex_dir = host_home.join(".codex");
    create_dir(&codex_dir)?;
    write_file(&codex_dir.join("auth.toml"), "token = \"x\"\n")?;

    let request = CredentialUploadRequest::new(
        "container-456",
        host_home,
        case.copy_claude,
        case.copy_codex,
    );
    let (uploader, captured) = successful_uploader();

    let result = runtime_handle
        .block_on(EngineConnector::upload_credentials_async(
            &uploader, &request,
        ))
        .map_err(|error| io_error(format!("upload should succeed: {error}")))?;

    let expected_paths_values: Vec<String> =
        case.expected_paths.into_iter().map(String::from).collect();
    ensure(
        result.expected_container_paths() == expected_paths_values.as_slice(),
        format!(
            "expected uploaded paths {:?}, got {:?}",
            expected_paths_values,
            result.expected_container_paths()
        ),
    )?;

    let captured_call = captured_call(&captured)?;
    ensure(
        captured_call.call_count == 1,
        format!("expected one upload call, got {}", captured_call.call_count),
    )?;

    let entries = parse_archive_entries(&captured_call.archive_bytes)?;
    let entry_paths: Vec<String> = entries.into_iter().map(|entry| entry.path).collect();

    ensure(
        entry_paths
            .iter()
            .any(|path| path == case.expected_archive_entry),
        format!(
            "expected archive entry '{}' in {entry_paths:?}",
            case.expected_archive_entry
        ),
    )?;

    if !case.copy_claude {
        ensure(
            !entry_paths.iter().any(|path| path.starts_with(".claude/")),
            format!("did not expect .claude entries when disabled, got {entry_paths:?}"),
        )?;
    }

    if !case.copy_codex {
        ensure(
            !entry_paths.iter().any(|path| path.starts_with(".codex/")),
            format!("did not expect .codex entries when disabled, got {entry_paths:?}"),
        )?;
    }

    Ok(())
}

#[rstest]
fn upload_credentials_skips_missing_sources_without_error(
    runtime: std::io::Result<tokio::runtime::Runtime>,
    host_home_dir: std::io::Result<(tempfile::TempDir, camino::Utf8PathBuf)>,
) -> std::io::Result<()> {
    let runtime_handle = runtime?;
    let (_tmp, host_home) = host_home_dir?;

    let codex_dir = host_home.join(".codex");
    create_dir(&codex_dir)?;
    write_file(&codex_dir.join("auth.toml"), "token = \"codex\"")?;

    let request = CredentialUploadRequest::new("container-789", host_home, true, true);
    let (uploader, captured) = successful_uploader();

    let result = runtime_handle
        .block_on(EngineConnector::upload_credentials_async(
            &uploader, &request,
        ))
        .map_err(|error| {
            io_error(format!(
                "upload should succeed when one source is missing: {error}"
            ))
        })?;

    let expected_paths = [String::from("/root/.codex")];
    ensure(
        result.expected_container_paths() == expected_paths.as_slice(),
        format!(
            "expected uploaded paths {expected_paths:?}, got {:?}",
            result.expected_container_paths()
        ),
    )?;
    ensure(
        captured_call(&captured)?.call_count == 1,
        "expected one upload call when codex source exists",
    )?;

    Ok(())
}

#[rstest]
fn upload_credentials_is_noop_when_all_sources_missing_or_disabled(
    runtime: std::io::Result<tokio::runtime::Runtime>,
    host_home_dir: std::io::Result<(tempfile::TempDir, camino::Utf8PathBuf)>,
) -> std::io::Result<()> {
    let runtime_handle = runtime?;
    let (_tmp, host_home) = host_home_dir?;

    let request = CredentialUploadRequest::new("container-empty", host_home, true, true);
    let (uploader, captured) = successful_uploader();

    let result = runtime_handle
        .block_on(EngineConnector::upload_credentials_async(
            &uploader, &request,
        ))
        .map_err(|error| io_error(format!("upload should succeed as a no-op: {error}")))?;

    ensure(
        result.expected_container_paths().is_empty(),
        format!(
            "expected no uploaded paths, got {:?}",
            result.expected_container_paths()
        ),
    )?;
    ensure(
        captured_call(&captured)?.call_count == 0,
        "expected no upload calls for noop path",
    )?;

    Ok(())
}

#[rstest]
fn upload_credentials_maps_engine_failures_to_upload_failed(
    runtime: std::io::Result<tokio::runtime::Runtime>,
    host_home_dir: std::io::Result<(tempfile::TempDir, camino::Utf8PathBuf)>,
) -> std::io::Result<()> {
    let runtime_handle = runtime?;
    let (_tmp, host_home) = host_home_dir?;

    let claude_dir = host_home.join(".claude");
    create_dir(&claude_dir)?;
    write_file(&claude_dir.join("settings.json"), "{}\n")?;

    let request = CredentialUploadRequest::new("container-failed", host_home, true, false);
    let (uploader, _) = failing_uploader(bollard::errors::Error::RequestTimeoutError);

    let result = runtime_handle.block_on(EngineConnector::upload_credentials_async(
        &uploader, &request,
    ));
    let error = match result {
        Ok(success) => {
            return Err(io_error(format!(
                "expected upload failure, got success: {success:?}"
            )));
        }
        Err(error) => error,
    };

    ensure(
        matches!(
            error,
            PodbotError::Container(ContainerError::UploadFailed {
                ref container_id,
                ref message,
            }) if container_id == "container-failed" && message.contains("Timeout error")
        ),
        format!("expected upload-failed mapping, got: {error:?}"),
    )?;

    Ok(())
}

#[rstest]
fn upload_credentials_with_host_home_dir_uses_provided_capability(
    runtime: std::io::Result<tokio::runtime::Runtime>,
    host_home_dir: std::io::Result<(tempfile::TempDir, camino::Utf8PathBuf)>,
) -> std::io::Result<()> {
    let runtime_handle = runtime?;
    let (_tmp, host_home) = host_home_dir?;

    let claude_dir = host_home.join(".claude");
    create_dir(&claude_dir)?;
    write_file(&claude_dir.join("settings.json"), "{}\n")?;

    let host_home_capability = Dir::open_ambient_dir(&host_home, ambient_authority())?;
    let unreachable_path = host_home.join("missing-home-directory");
    let request =
        CredentialUploadRequest::new("container-capability", unreachable_path, true, false);
    let (uploader, captured) = successful_uploader();

    let result = runtime_handle
        .block_on(
            EngineConnector::upload_credentials_with_host_home_dir_async(
                &uploader,
                &request,
                &host_home_capability,
            ),
        )
        .map_err(|error| {
            io_error(format!(
                "upload with pre-opened home should succeed: {error}"
            ))
        })?;

    let expected_paths = [String::from("/root/.claude")];
    ensure(
        result.expected_container_paths() == expected_paths.as_slice(),
        format!(
            "expected uploaded paths {expected_paths:?}, got {:?}",
            result.expected_container_paths()
        ),
    )?;
    ensure(
        captured_call(&captured)?.call_count == 1,
        "expected one upload call when using pre-opened host dir",
    )?;

    Ok(())
}
