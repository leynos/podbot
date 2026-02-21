//! Unit tests for tar archive structure and permission preservation.

use std::io::Cursor;

use rstest::rstest;

use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TarEntry {
    pub(super) path: String,
    pub(super) mode: u32,
}

#[rstest]
#[cfg(unix)]
fn tar_archive_preserves_directory_and_file_permissions() -> std::io::Result<()> {
    let (_tmp, host_home) = host_home_dir();

    let claude_dir = host_home.join(".claude");
    create_dir(claude_dir.as_std_path());
    set_mode(claude_dir.as_std_path(), 0o700);

    let credentials_path = claude_dir.join("credentials.json");
    write_file(credentials_path.as_std_path(), "{\"token\":\"abc\"}\n");
    set_mode(credentials_path.as_std_path(), 0o600);

    let request = CredentialUploadRequest::new("container-perms", host_home, true, false);
    let (uploader, captured) = successful_uploader();

    runtime()
        .block_on(EngineConnector::upload_credentials_async(
            &uploader, &request,
        ))
        .map_err(|error| io_error(format!("upload should succeed: {error}")))?;

    let captured_call = captured_call(&captured);
    let entries = parse_archive_entries(&captured_call.archive_bytes)?;

    let claude_dir_entry = entries
        .iter()
        .find(|entry| entry.path == ".claude/")
        .ok_or_else(|| io_error("missing .claude/ directory entry"))?;
    let credentials_entry = entries
        .iter()
        .find(|entry| entry.path == ".claude/credentials.json")
        .ok_or_else(|| io_error("missing credentials file entry"))?;

    if claude_dir_entry.mode != 0o700 {
        return Err(io_error(format!(
            "expected mode 0o700 for .claude/, got {:o}",
            claude_dir_entry.mode
        )));
    }

    if credentials_entry.mode != 0o600 {
        return Err(io_error(format!(
            "expected mode 0o600 for credentials file, got {:o}",
            credentials_entry.mode
        )));
    }

    Ok(())
}

pub(super) fn parse_archive_entries(archive_bytes: &[u8]) -> std::io::Result<Vec<TarEntry>> {
    let mut archive = tar::Archive::new(Cursor::new(archive_bytes));
    let mut entries = vec![];

    for entry_result in archive.entries()? {
        let entry = entry_result?;
        let path = entry.path()?.to_string_lossy().into_owned();
        let mode = entry.header().mode()?;

        entries.push(TarEntry { path, mode });
    }

    Ok(entries)
}
