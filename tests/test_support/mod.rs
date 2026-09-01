//! Shared helpers for integration tests.
//!
//! Integration tests live in separate crates under `tests/`. This module exists
//! to share small helpers between those crates without requiring process-wide
//! environment mutation (which is forbidden by the project's testing guidance).

use std::collections::HashMap;
use std::io::{self, Write};

use camino::Utf8PathBuf;
use mockable::MockEnv;
use tempfile::NamedTempFile;

/// Helper: Creates a `MockEnv` that returns the provided values.
pub(crate) fn env_with(values: &[(&str, &str)]) -> MockEnv {
    let map: HashMap<String, String> = values
        .iter()
        .map(|(key, value)| (String::from(*key), String::from(*value)))
        .collect();

    let mut env = MockEnv::new();
    env.expect_string()
        .returning(move |key| map.get(key).cloned());
    env
}

/// Helper: Creates a temporary config file with the given TOML content.
fn temp_config_file(content: &str) -> io::Result<NamedTempFile> {
    let mut file = NamedTempFile::new()?;
    file.write_all(content.as_bytes())?;
    Ok(file)
}

/// Writes `content` to a temporary config file and returns it alongside its
/// UTF-8 path.
///
/// The [`NamedTempFile`] is returned so the caller keeps it alive: dropping it
/// deletes the backing file and invalidates the path. A non-UTF-8 temporary
/// path is reported as an [`io::Error`] rather than a panic, so this helper
/// stays usable outwith test bodies.
///
/// # Examples
///
/// ```ignore
/// let (_config_file, config_path) =
///     temp_config_path("image = \"test:v1\"\n").expect("temp config file should be created");
/// assert!(config_path.exists());
/// ```
pub(crate) fn temp_config_path(content: &str) -> io::Result<(NamedTempFile, Utf8PathBuf)> {
    let file = temp_config_file(content)?;
    let path = Utf8PathBuf::try_from(file.path().to_path_buf()).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("temporary config path is not valid UTF-8: {error}"),
        )
    })?;
    Ok((file, path))
}
