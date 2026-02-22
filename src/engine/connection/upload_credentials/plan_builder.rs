//! Plan construction helpers for credential upload operations.

use std::io;

use cap_std::fs_utf8::Dir;

use super::archive::build_tar_archive;
use super::{
    CLAUDE_CREDENTIAL_DIR, CODEX_CREDENTIAL_DIR, CONTAINER_HOME_DIR, CredentialUploadPlan,
    CredentialUploadRequest,
};

#[derive(Debug, Default)]
struct SelectedSources {
    source_directory_names: Vec<&'static str>,
    expected_container_paths: Vec<String>,
}

/// Try to include a credential source in the upload plan.
///
/// Returns `Ok(Some((dir_name, container_path)))` when the source is enabled,
/// present, and valid. Returns `Ok(None)` when disabled or missing. Returns
/// `Err` when the source exists but is invalid.
fn include_credential_source(
    host_home_dir: &Dir,
    is_enabled: bool,
    directory_name: &'static str,
) -> io::Result<Option<(&'static str, String)>> {
    if !is_enabled {
        return Ok(None);
    }

    match host_home_dir.metadata(directory_name) {
        Ok(metadata) if metadata.is_dir() => {
            let container_path = format!("{CONTAINER_HOME_DIR}/{directory_name}");
            Ok(Some((directory_name, container_path)))
        }
        Ok(_) => Err(io::Error::other(format!(
            "credential source '{directory_name}' exists but is not a directory"
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io::Error::other(format!(
            "failed to inspect credential source '{directory_name}': {error}"
        ))),
    }
}

pub(crate) fn build_upload_plan(
    host_home_dir: &Dir,
    request: &CredentialUploadRequest,
) -> io::Result<CredentialUploadPlan> {
    let selected_source_pairs = [
        (request.copy_claude, CLAUDE_CREDENTIAL_DIR),
        (request.copy_codex, CODEX_CREDENTIAL_DIR),
    ]
    .into_iter()
    .map(|(is_enabled, directory_name)| {
        include_credential_source(host_home_dir, is_enabled, directory_name)
    })
    .collect::<io::Result<Vec<_>>>()?
    .into_iter()
    .flatten();

    let (source_directory_names, expected_container_paths) = selected_source_pairs.unzip();
    let selected_sources = SelectedSources {
        source_directory_names,
        expected_container_paths,
    };

    build_plan_from_selected_sources(host_home_dir, selected_sources)
}

fn build_plan_from_selected_sources(
    host_home_dir: &Dir,
    selected_sources: SelectedSources,
) -> io::Result<CredentialUploadPlan> {
    if selected_sources.source_directory_names.is_empty() {
        return Ok(CredentialUploadPlan {
            archive_bytes: vec![],
            expected_container_paths: vec![],
        });
    }

    let archive_bytes = build_tar_archive(host_home_dir, &selected_sources.source_directory_names)?;

    Ok(CredentialUploadPlan {
        archive_bytes,
        expected_container_paths: selected_sources.expected_container_paths,
    })
}
