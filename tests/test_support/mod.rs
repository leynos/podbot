//! Shared helpers for integration tests.
//!
//! Integration tests live in separate crates under `tests/`. This module exists
//! to share small helpers between those crates without requiring process-wide
//! environment mutation (which is forbidden by the project's testing guidance).

use std::collections::HashMap;
use std::io::Write;

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
pub(crate) fn temp_config_file(content: &str) -> std::io::Result<NamedTempFile> {
    let mut file = NamedTempFile::new()?;
    file.write_all(content.as_bytes())?;
    Ok(file)
}
