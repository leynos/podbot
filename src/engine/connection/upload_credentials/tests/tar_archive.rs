//! Unit tests for tar archive structure and permission preservation.

use std::io::Cursor;

use camino::Utf8Path;
use cap_std::{ambient_authority, fs_utf8::Dir};
use rstest::rstest;
use tar::EntryType;

use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TarEntry {
    pub(super) path: String,
    pub(super) mode: u32,
    pub(super) entry_type: EntryType,
}

#[rstest]
#[cfg(unix)]
fn tar_archive_preserves_directory_and_file_permissions() -> std::io::Result<()> {
    let (_tmp, host_home) = host_home_dir();

    let claude_dir = host_home.join(".claude");
    create_dir(&claude_dir);
    set_mode(&claude_dir, 0o700);

    let credentials_path = claude_dir.join("credentials.json");
    write_file(&credentials_path, "{\"token\":\"abc\"}\n");
    set_mode(&credentials_path, 0o600);

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

#[rstest]
#[cfg(unix)]
fn tar_archive_preserves_symlink_entries() -> std::io::Result<()> {
    let (_tmp, host_home) = host_home_dir();
    let host_home_dir = Dir::open_ambient_dir(&host_home, ambient_authority())?;

    let claude_dir = host_home.join(".claude");
    create_dir(&claude_dir);
    write_file(&claude_dir.join("target.toml"), "token = \"abc\"\n");

    let source_dir = host_home_dir.open_dir(".claude")?;
    source_dir.symlink("target.toml", "linked.toml")?;

    let request = CredentialUploadRequest::new("container-symlink", host_home, true, false);
    let (uploader, captured) = successful_uploader();

    runtime()
        .block_on(EngineConnector::upload_credentials_async(
            &uploader, &request,
        ))
        .map_err(|error| io_error(format!("upload should succeed: {error}")))?;

    let captured_call = captured_call(&captured);
    let entries = parse_archive_entries(&captured_call.archive_bytes)?;
    let symlink_entry = entries
        .iter()
        .find(|entry| entry.path == ".claude/linked.toml")
        .ok_or_else(|| io_error("missing symlink archive entry"))?;

    if !symlink_entry.entry_type.is_symlink() {
        return Err(io_error(format!(
            "expected symlink entry type, got {:?}",
            symlink_entry.entry_type
        )));
    }

    Ok(())
}

#[rstest]
fn normalize_archive_path_uses_forward_slashes() {
    let nested_path = Utf8Path::new(r".claude\subdir\credentials.json");
    let archive_path = normalize_archive_path(nested_path);

    assert_eq!(archive_path, ".claude/subdir/credentials.json");
}

pub(super) fn parse_archive_entries(archive_bytes: &[u8]) -> std::io::Result<Vec<TarEntry>> {
    let mut archive = tar::Archive::new(Cursor::new(archive_bytes));
    let mut entries = vec![];

    for entry_result in archive.entries()? {
        let entry = entry_result?;
        let path = entry.path()?.to_string_lossy().into_owned();
        let mode = entry.header().mode()?;
        let entry_type = entry.header().entry_type();

        entries.push(TarEntry {
            path,
            mode,
            entry_type,
        });
    }

    Ok(entries)
}
