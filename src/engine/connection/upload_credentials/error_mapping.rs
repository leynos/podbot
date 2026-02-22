//! Filesystem error mapping helpers for credential upload operations.

use std::io;

use camino::{Utf8Path, Utf8PathBuf};

use super::{CLAUDE_CREDENTIAL_DIR, CODEX_CREDENTIAL_DIR};
use crate::error::{FilesystemError, PodbotError};

#[derive(Debug)]
pub(crate) struct LocalUploadError {
    pub(crate) path: Utf8PathBuf,
    pub(crate) error: io::Error,
}

pub(crate) fn map_local_upload_error(local_error: LocalUploadError) -> PodbotError {
    let LocalUploadError { path, error } = local_error;
    PodbotError::from(FilesystemError::IoError {
        path: path.as_std_path().to_path_buf(),
        message: error.to_string(),
    })
}

pub(crate) fn select_error_path(error: &impl ToString, host_home_dir: &Utf8Path) -> Utf8PathBuf {
    let error_message = error.to_string();
    if error_message.contains(CLAUDE_CREDENTIAL_DIR) {
        return host_home_dir.join(CLAUDE_CREDENTIAL_DIR);
    }
    if error_message.contains(CODEX_CREDENTIAL_DIR) {
        return host_home_dir.join(CODEX_CREDENTIAL_DIR);
    }

    host_home_dir.to_path_buf()
}
