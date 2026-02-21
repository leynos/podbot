//! Tar archive construction helpers for credential upload.

use std::io;

use camino::{Utf8Path, Utf8PathBuf};
use cap_std::fs::Metadata;
use cap_std::fs_utf8::Dir;
use tar::{Builder, EntryType, Header};

const DEFAULT_DIRECTORY_MODE: u32 = 0o755;
const DEFAULT_FILE_MODE: u32 = 0o644;

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
            EntryKind::File => {
                append_file_header(builder, current_dir, &entry.file_name, &entry_relative_path)?;
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

        let entry_kind = if file_type.is_dir() {
            EntryKind::Directory
        } else if file_type.is_file() {
            EntryKind::File
        } else {
            EntryKind::Other
        };

        entries.push(SortedEntry {
            file_name,
            entry_kind,
        });
    }

    entries.sort_unstable_by(|left, right| left.file_name.cmp(&right.file_name));
    Ok(entries)
}

fn append_directory_header(
    builder: &mut Builder<Vec<u8>>,
    relative_path: &Utf8Path,
    metadata: &Metadata,
) -> io::Result<()> {
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Directory);
    header.set_size(0);
    header.set_mode(metadata_mode(metadata, DEFAULT_DIRECTORY_MODE));
    header.set_cksum();

    let path = format!("{}/", normalize_archive_path(relative_path));
    builder.append_data(&mut header, path, io::empty())
}

fn append_file_header(
    builder: &mut Builder<Vec<u8>>,
    parent_dir: &Dir,
    file_name: &str,
    relative_path: &Utf8Path,
) -> io::Result<()> {
    let metadata = parent_dir.metadata(file_name)?;
    let mut file = parent_dir.open(file_name)?;

    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Regular);
    header.set_size(metadata.len());
    header.set_mode(metadata_mode(&metadata, DEFAULT_FILE_MODE));
    header.set_cksum();

    let path = normalize_archive_path(relative_path);
    builder.append_data(&mut header, path, &mut file)
}

fn normalize_archive_path(path: &Utf8Path) -> String {
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
