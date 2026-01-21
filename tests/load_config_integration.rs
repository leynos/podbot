//! Integration tests for the `load_config()` public API.
//!
//! These tests validate the end-to-end behaviour of `load_config()` from the
//! `podbot::config` module, testing CLI argument parsing through to final
//! configuration values.

// Test-specific lint exceptions: expect is standard practice in tests
#![expect(clippy::expect_used, reason = "expect is standard practice in tests")]
#![expect(clippy::unwrap_used, reason = "unwrap is acceptable in tests")]

use std::io::Write;

use camino::Utf8PathBuf;
use podbot::config::{Cli, Commands, load_config};
use tempfile::NamedTempFile;

/// Helper: Creates a CLI struct with a config file path.
///
/// Uses the `Ps` subcommand as it requires no additional arguments.
const fn cli_with_config(config_path: Option<Utf8PathBuf>) -> Cli {
    Cli {
        config: config_path,
        engine_socket: None,
        image: None,
        command: Commands::Ps,
    }
}

/// Helper: Creates a temporary config file with the given TOML content.
fn temp_config_file(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("failed to create temp file");
    file.write_all(content.as_bytes())
        .expect("failed to write to temp file");
    file
}

#[test]
fn load_config_returns_defaults_when_no_sources_provided() {
    // CLI with no config file, no CLI overrides.
    let cli = cli_with_config(None);

    // Note: This test assumes no PODBOT_* env vars are set and no config file
    // exists at standard locations. In CI this should be true.
    let result = load_config(&cli);

    // Should succeed with default values.
    assert!(result.is_ok(), "load_config should succeed: {result:?}");
    let config = result.unwrap();

    // Verify key defaults.
    assert!(config.engine_socket.is_none());
    assert!(config.image.is_none());
    assert!(!config.sandbox.privileged);
    assert!(config.sandbox.mount_dev_fuse);
    assert_eq!(config.workspace.base_dir.as_str(), "/work");
}

#[test]
fn load_config_loads_from_config_file() {
    let toml_content = r#"
        engine_socket = "unix:///from/config/file.sock"
        image = "test-image:v1"

        [sandbox]
        privileged = true
    "#;
    let config_file = temp_config_file(toml_content);
    let config_path = Utf8PathBuf::try_from(config_file.path().to_path_buf())
        .expect("path should be valid UTF-8");

    let cli = cli_with_config(Some(config_path));
    let config = load_config(&cli).expect("load_config should succeed");

    assert_eq!(
        config.engine_socket.as_deref(),
        Some("unix:///from/config/file.sock")
    );
    assert_eq!(config.image.as_deref(), Some("test-image:v1"));
    assert!(config.sandbox.privileged);
    // Defaults should still apply for unset fields.
    assert!(config.sandbox.mount_dev_fuse);
}

#[test]
fn load_config_cli_overrides_config_file() {
    let toml_content = r#"
        engine_socket = "unix:///from/config/file.sock"
        image = "file-image:v1"
    "#;
    let config_file = temp_config_file(toml_content);
    let config_path = Utf8PathBuf::try_from(config_file.path().to_path_buf())
        .expect("path should be valid UTF-8");

    // CLI provides engine_socket override.
    let cli = Cli {
        config: Some(config_path),
        engine_socket: Some("unix:///from/cli.sock".to_owned()),
        image: None,
        command: Commands::Ps,
    };
    let config = load_config(&cli).expect("load_config should succeed");

    // CLI wins for engine_socket.
    assert_eq!(
        config.engine_socket.as_deref(),
        Some("unix:///from/cli.sock")
    );
    // File value preserved for image.
    assert_eq!(config.image.as_deref(), Some("file-image:v1"));
}

#[test]
fn load_config_handles_missing_config_file_gracefully() {
    // Point to a non-existent config file.
    let cli = cli_with_config(Some(Utf8PathBuf::from("/nonexistent/config.toml")));

    // Should succeed (missing file is OK, falls back to defaults).
    let result = load_config(&cli);
    assert!(result.is_ok(), "load_config should succeed: {result:?}");

    let config = result.unwrap();
    // All defaults should apply.
    assert!(config.engine_socket.is_none());
}

#[test]
fn load_config_rejects_malformed_config_file() {
    let toml_content = r"
        this is not valid TOML {{{
    ";
    let config_file = temp_config_file(toml_content);
    let config_path = Utf8PathBuf::try_from(config_file.path().to_path_buf())
        .expect("path should be valid UTF-8");

    let cli = cli_with_config(Some(config_path));
    let result = load_config(&cli);

    assert!(
        result.is_err(),
        "load_config should fail for malformed TOML"
    );
}

#[test]
fn load_config_preserves_nested_config_defaults() {
    // Only set top-level fields, nested should get defaults.
    let toml_content = r#"
        engine_socket = "unix:///test.sock"
    "#;
    let config_file = temp_config_file(toml_content);
    let config_path = Utf8PathBuf::try_from(config_file.path().to_path_buf())
        .expect("path should be valid UTF-8");

    let cli = cli_with_config(Some(config_path));
    let config = load_config(&cli).expect("load_config should succeed");

    // Top-level from file.
    assert_eq!(config.engine_socket.as_deref(), Some("unix:///test.sock"));

    // Nested defaults preserved.
    assert!(!config.sandbox.privileged);
    assert!(config.sandbox.mount_dev_fuse);
    assert_eq!(config.workspace.base_dir.as_str(), "/work");
    assert!(config.creds.copy_claude);
    assert!(config.creds.copy_codex);
}
