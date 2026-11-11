//! Host-side storage for GitHub App installation tokens.
//!
//! The refresh daemon writes each short-lived token to a per-session directory
//! under `$XDG_RUNTIME_DIR/podbot/`. The directory is created with owner-only
//! permissions (`0700`), the token file with `0600`, and every write is staged
//! through a temporary file plus atomic rename so a container never observes a
//! partial token.
//!
//! The token is never logged. Callers must remove the store when the sandbox
//! exits; [`TokenStore::cleanup`] removes the session directory recursively.

use std::io::{ErrorKind, Write};

use camino::{Utf8Path, Utf8PathBuf};
use cap_std::ambient_authority;
use cap_std::fs::{DirBuilder, OpenOptions, Permissions};
use cap_std::fs_utf8::Dir;
#[cfg(unix)]
use cap_std::fs_utf8::{DirBuilderExt, OpenOptionsExt, PermissionsExt};

use crate::error::{ConfigError, GitHubError, PodbotError};

/// Name of the token file inside the session directory.
pub const TOKEN_FILE_NAME: &str = "ghapp_token";

/// Container directory where the host token directory is mounted read-only.
pub const CONTAINER_TOKEN_DIR: &str = "/run/secrets";

/// Environment variable naming the per-user runtime directory.
pub const XDG_RUNTIME_DIR: &str = "XDG_RUNTIME_DIR";

/// Owner-only directory permission bits.
#[cfg(unix)]
const DIRECTORY_MODE: u32 = 0o700;

/// Owner-only file permission bits.
#[cfg(unix)]
const FILE_MODE: u32 = 0o600;

/// A per-session token directory on the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenStore {
    session_dir: Utf8PathBuf,
    token_path: Utf8PathBuf,
}

impl TokenStore {
    /// Create a token store rooted at `$XDG_RUNTIME_DIR/podbot/<session_id>`.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::MissingRequired` when `XDG_RUNTIME_DIR` is absent
    /// or blank, and `ConfigError::InvalidValue` when `session_id` is empty or
    /// contains characters that could escape the runtime directory.
    pub fn from_env(env: &dyn mockable::Env, session_id: &str) -> Result<Self, PodbotError> {
        let runtime_dir = env
            .string(XDG_RUNTIME_DIR)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| ConfigError::MissingRequired {
                field: String::from(XDG_RUNTIME_DIR),
            })?;

        Self::new(Utf8Path::new(runtime_dir.trim()), session_id)
    }

    /// Create a token store under an explicit runtime directory.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::InvalidValue` when `session_id` is not a safe
    /// single path segment.
    pub fn new(runtime_dir: &Utf8Path, session_id: &str) -> Result<Self, PodbotError> {
        validate_session_id(session_id)?;
        let session_dir = runtime_dir.join("podbot").join(session_id);

        Ok(Self {
            token_path: session_dir.join(TOKEN_FILE_NAME),
            session_dir,
        })
    }

    /// Return the absolute path of the token file.
    #[must_use]
    pub fn token_path(&self) -> &Utf8Path {
        &self.token_path
    }

    /// Return the absolute path of the session directory.
    #[must_use]
    pub fn session_dir(&self) -> &Utf8Path {
        &self.session_dir
    }

    /// Write the token atomically with owner-only permissions.
    ///
    /// # Errors
    ///
    /// Returns `GitHubError::TokenStoreWriteFailed` when the runtime directory
    /// is missing or the token cannot be staged and renamed.
    pub fn write_token(&self, token: &str) -> Result<(), PodbotError> {
        let session_dir = create_session_dir(&self.session_dir)?;
        let temp_name = temp_file_name();
        write_private_file(&session_dir, &temp_name, token, &self.session_dir)?;

        session_dir
            .rename(&temp_name, &session_dir, TOKEN_FILE_NAME)
            .map_err(|error| token_store_write_failed(&self.token_path, &error))
    }

    /// Remove the session directory and everything inside it.
    ///
    /// A missing session directory is not an error: cleanup is idempotent so
    /// teardown can run after partial startup.
    ///
    /// # Errors
    ///
    /// Returns `GitHubError::TokenStoreCleanupFailed` when the directory
    /// exists but cannot be removed.
    pub fn cleanup(&self) -> Result<(), PodbotError> {
        let Some(podbot_dir_path) = self.session_dir.parent() else {
            return Ok(());
        };
        let Some(runtime_dir_path) = podbot_dir_path.parent() else {
            return Ok(());
        };
        let Some(session_name) = self.session_dir.file_name() else {
            return Ok(());
        };

        let runtime_dir = match Dir::open_ambient_dir(runtime_dir_path, ambient_authority()) {
            Ok(dir) => dir,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(token_store_cleanup_failed(&self.session_dir, &error)),
        };
        let podbot_dir = match runtime_dir.open_dir("podbot") {
            Ok(dir) => dir,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(token_store_cleanup_failed(&self.session_dir, &error)),
        };

        match podbot_dir.remove_dir_all(session_name) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(token_store_cleanup_failed(&self.session_dir, &error)),
        }
    }
}

/// Reject session identifiers that could escape the runtime directory.
fn validate_session_id(session_id: &str) -> Result<(), PodbotError> {
    let is_safe_segment = !session_id.is_empty()
        && session_id != "."
        && session_id != ".."
        && session_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character));

    if is_safe_segment {
        return Ok(());
    }

    Err(ConfigError::InvalidValue {
        field: String::from("session_id"),
        reason: String::from(
            "session_id must be a single path segment of ASCII letters, digits, '-', '_' or '.'",
        ),
    }
    .into())
}

/// Open or create the session directory with owner-only permissions.
fn create_session_dir(session_dir: &Utf8Path) -> Result<Dir, PodbotError> {
    let runtime_dir_path = session_dir
        .parent()
        .and_then(Utf8Path::parent)
        .ok_or_else(|| GitHubError::TokenStoreWriteFailed {
            path: std::path::PathBuf::from(session_dir.as_std_path()),
            message: String::from("token store path has no runtime directory"),
        })?;
    let session_name =
        session_dir
            .file_name()
            .ok_or_else(|| GitHubError::TokenStoreWriteFailed {
                path: std::path::PathBuf::from(session_dir.as_std_path()),
                message: String::from("token store path has no session directory name"),
            })?;

    let runtime_dir =
        Dir::open_ambient_dir(runtime_dir_path, ambient_authority()).map_err(|error| {
            GitHubError::TokenStoreWriteFailed {
                path: std::path::PathBuf::from(runtime_dir_path.as_std_path()),
                message: format!("failed to open runtime directory: {error}"),
            }
        })?;
    let podbot_dir_path = runtime_dir_path.join("podbot");
    let podbot_dir = open_or_create(&runtime_dir, "podbot", &podbot_dir_path)?;

    open_or_create(&podbot_dir, session_name, session_dir)
}

/// Open a child directory, creating it owner-only when absent.
fn open_or_create(parent: &Dir, name: &str, display: &Utf8Path) -> Result<Dir, PodbotError> {
    match create_private_dir(parent, name) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
        Err(error) => return Err(token_store_write_failed(display, &error)),
    }

    enforce_private_dir(parent, name, display)?;
    parent
        .open_dir(name)
        .map_err(|error| token_store_write_failed(display, &error))
}

/// Create a directory with owner-only permissions where the platform supports
/// permission bits; other platforms rely on the runtime directory's policy.
fn create_private_dir(parent: &Dir, name: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let mut builder = DirBuilder::new();
        builder.mode(DIRECTORY_MODE);
        parent.create_dir_with(name, &builder)
    }

    #[cfg(not(unix))]
    {
        parent.create_dir(name)
    }
}

/// Enforce owner-only permissions on an existing directory.
fn enforce_private_dir(parent: &Dir, name: &str, display: &Utf8Path) -> Result<(), PodbotError> {
    #[cfg(unix)]
    {
        parent
            .set_permissions(name, Permissions::from_mode(DIRECTORY_MODE))
            .map_err(|error| token_store_write_failed(display, &error))
    }

    #[cfg(not(unix))]
    {
        let _ = (parent, name);
        let _ = display;
        Ok(())
    }
}

/// Write a staged token file with owner-only permissions.
fn write_private_file(
    dir: &Dir,
    name: &str,
    token: &str,
    display: &Utf8Path,
) -> Result<(), PodbotError> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(FILE_MODE);

    let path = display.join(name);
    let mut file = dir
        .open_with(name, &options)
        .map_err(|error| token_store_write_failed(&path, &error))?;
    file.write_all(token.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|error| token_store_write_failed(&path, &error))
}

/// Unique temporary name for the staged token file.
fn temp_file_name() -> String {
    format!("{TOKEN_FILE_NAME}.tmp.{}", std::process::id())
}

fn token_store_write_failed(path: &Utf8Path, error: &std::io::Error) -> PodbotError {
    GitHubError::TokenStoreWriteFailed {
        path: std::path::PathBuf::from(path.as_std_path()),
        message: error.to_string(),
    }
    .into()
}

fn token_store_cleanup_failed(path: &Utf8Path, error: &std::io::Error) -> PodbotError {
    GitHubError::TokenStoreCleanupFailed {
        path: std::path::PathBuf::from(path.as_std_path()),
        message: error.to_string(),
    }
    .into()
}

#[cfg(test)]
mod tests;
