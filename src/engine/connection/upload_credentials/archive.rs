//! Tar archive construction helpers for credential upload.

use std::io;

use camino::{Utf8Path, Utf8PathBuf};
use cap_std::fs::Metadata;
use cap_std::fs_utf8::Dir;
use tar::{Builder, EntryType, Header};

const DEFAULT_DIRECTORY_MODE: u32 = 0o755;
const DEFAULT_FILE_MODE: u32 = 0o644;

/// Build an in-memory tar archive containing selected credential directories.
///
/// Directory and file entries preserve source mode bits where available.
pub(super) fn build_tar_archive(
    host_home_dir: &Dir,
    source_directory_names: &[&str],
) -> io::Result<Vec<u8>> {
    let mut builder = Builder::new(vec![]);

    for source_directory_name in source_directory_names {
        let source_path = Utf8PathBuf::from(source_directory_name);
        let metadata = host_home_dir.metadata(source_directory_name)?;
        append_directory_header(&mut builder, &source_path, &metadata)?;

        let source_dir = host_home_dir.open_dir(source_directory_name)?;
        append_directory_contents(&mut builder, &source_dir, &source_path)?;
    }

    builder.finish()?;
    builder.into_inner()
}

fn append_directory_contents(
    builder: &mut Builder<Vec<u8>>,
    current_dir: &Dir,
    current_relative_path: &Utf8Path,
) -> io::Result<()> {
    let entries = sorted_entries(current_dir)?;

    for entry in entries {
        let entry_relative_path = current_relative_path.join(&entry.file_name);

        match entry.entry_kind {
            EntryKind::Directory => {
                let metadata = current_dir.metadata(&entry.file_name)?;
                append_directory_header(builder, &entry_relative_path, &metadata)?;
                let child_dir = current_dir.open_dir(&entry.file_name)?;
                append_directory_contents(builder, &child_dir, &entry_relative_path)?;
            }
            EntryKind::File | EntryKind::Symlink => {
                append_non_directory_entry(builder, current_dir, &entry, &entry_relative_path)?;
            }
            EntryKind::Other => {}
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    Directory,
    File,
    Symlink,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SortedEntry {
    file_name: String,
    entry_kind: EntryKind,
}

fn sorted_entries(directory: &Dir) -> io::Result<Vec<SortedEntry>> {
    let mut entries = vec![];

    for entry_result in directory.entries()? {
        let entry = entry_result?;
        let file_name = entry.file_name()?;
        let file_type = entry.file_type()?;
        let entry_kind = classify_entry_kind(
            file_type.is_dir(),
            file_type.is_file(),
            file_type.is_symlink(),
        );

        entries.push(SortedEntry {
            file_name,
            entry_kind,
        });
    }

    entries.sort_unstable_by(|left, right| left.file_name.cmp(&right.file_name));
    Ok(entries)
}

const fn classify_entry_kind(is_directory: bool, is_file: bool, is_symlink: bool) -> EntryKind {
    if is_directory {
        return EntryKind::Directory;
    }

    if is_file {
        return EntryKind::File;
    }

    if is_symlink {
        return EntryKind::Symlink;
    }

    EntryKind::Other
}

fn append_directory_header(
    builder: &mut Builder<Vec<u8>>,
    relative_path: &Utf8Path,
    metadata: &Metadata,
) -> io::Result<()> {
    let mut header = new_entry_header(
        EntryType::Directory,
        0,
        metadata_mode(metadata, DEFAULT_DIRECTORY_MODE),
    );

    let path = format!("{}/", normalize_archive_path(relative_path));
    builder.append_data(&mut header, path, io::empty())
}

fn append_non_directory_entry(
    builder: &mut Builder<Vec<u8>>,
    parent_dir: &Dir,
    entry: &SortedEntry,
    relative_path: &Utf8Path,
) -> io::Result<()> {
    let path = normalize_archive_path(relative_path);
    match entry.entry_kind {
        EntryKind::File => {
            let metadata = parent_dir.metadata(&entry.file_name)?;
            let mut file = parent_dir.open(&entry.file_name)?;
            let mut header = new_entry_header(
                EntryType::Regular,
                metadata.len(),
                metadata_mode(&metadata, DEFAULT_FILE_MODE),
            );

            builder.append_data(&mut header, path, &mut file)
        }
        EntryKind::Symlink => {
            let metadata = parent_dir.symlink_metadata(&entry.file_name)?;
            let target = parent_dir.read_link_contents(&entry.file_name)?;
            let mut header = new_entry_header(
                EntryType::Symlink,
                0,
                metadata_mode(&metadata, DEFAULT_FILE_MODE),
            );

            let normalized_target = normalize_archive_path(target.as_path());
            builder.append_link(&mut header, path, normalized_target)
        }
        EntryKind::Directory | EntryKind::Other => Err(io::Error::other(
            "non-directory entry helper received invalid entry kind",
        )),
    }
}

fn new_entry_header(entry_type: EntryType, size: u64, mode: u32) -> Header {
    let mut header = Header::new_gnu();
    header.set_entry_type(entry_type);
    header.set_size(size);
    header.set_mode(mode);
    header.set_cksum();
    header
}

/// Normalize archive entry paths to forward-slash separators.
///
/// Tar archives use `/` as the path separator, including on Windows hosts.
pub(super) fn normalize_archive_path(path: &Utf8Path) -> String {
    path.as_str().replace('\\', "/")
}

#[cfg(unix)]
fn metadata_mode(metadata: &Metadata, _fallback: u32) -> u32 {
    use cap_std::fs::PermissionsExt;

    metadata.permissions().mode() & 0o7777
}

#[cfg(not(unix))]
fn metadata_mode(_metadata: &Metadata, fallback: u32) -> u32 {
    fallback
}
