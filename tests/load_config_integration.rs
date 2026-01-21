//! Integration tests for the `load_config()` public API.
//!
//! These tests validate the end-to-end behaviour of `load_config()` from the
//! `podbot::config` module, testing CLI argument parsing through to final
//! configuration values.

use std::io::Write;

use camino::Utf8PathBuf;
use podbot::config::{Cli, Commands, load_config};
use serial_test::serial;
use tempfile::NamedTempFile;

/// All `PODBOT_*` environment variables that affect configuration loading.
const PODBOT_ENV_VARS: &[&str] = &[
    "PODBOT_CONFIG_PATH",
    "PODBOT_ENGINE_SOCKET",
    "PODBOT_IMAGE",
    "PODBOT_GITHUB_APP_ID",
    "PODBOT_GITHUB_INSTALLATION_ID",
    "PODBOT_GITHUB_PRIVATE_KEY_PATH",
    "PODBOT_SANDBOX_PRIVILEGED",
    "PODBOT_SANDBOX_MOUNT_DEV_FUSE",
    "PODBOT_AGENT_KIND",
    "PODBOT_AGENT_MODE",
    "PODBOT_WORKSPACE_BASE_DIR",
    "PODBOT_CREDS_COPY_CLAUDE",
    "PODBOT_CREDS_COPY_CODEX",
];

/// Clears all `PODBOT_*` environment variables to ensure test isolation.
///
/// # Safety
///
/// This function uses `std::env::remove_var` which is unsafe in Rust 2024.
/// It is safe to call in the context of these tests because:
/// - All tests that modify environment state are marked `#[serial]`
/// - No concurrent access to these environment variables is occurring
fn clear_podbot_env() {
    for var in PODBOT_ENV_VARS {
        // SAFETY: Tests are run serially via `#[serial]` attribute,
        // preventing concurrent access to environment variables.
        unsafe {
            std::env::remove_var(var);
        }
    }
}

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
///
/// # Errors
///
/// Returns an error if the temporary file cannot be created or written to.
fn temp_config_file(content: &str) -> std::io::Result<NamedTempFile> {
    let mut file = NamedTempFile::new()?;
    file.write_all(content.as_bytes())?;
    Ok(file)
}

#[test]
#[serial]
fn load_config_returns_defaults_when_no_sources_provided() {
    clear_podbot_env();

    // CLI with no config file, no CLI overrides.
    let cli = cli_with_config(None);

    let config = load_config(&cli).expect("load_config should succeed with defaults");

    // Verify key defaults.
    assert!(config.engine_socket.is_none());
    assert!(config.image.is_none());
    assert!(!config.sandbox.privileged);
    assert!(config.sandbox.mount_dev_fuse);
    assert_eq!(config.workspace.base_dir.as_str(), "/work");
}

#[test]
#[serial]
fn load_config_loads_from_config_file() {
    clear_podbot_env();

    let toml_content = r#"
        engine_socket = "unix:///from/config/file.sock"
        image = "test-image:v1"

        [sandbox]
        privileged = true
    "#;
    let config_file = temp_config_file(toml_content).expect("failed to create temp config");
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
#[serial]
fn load_config_cli_overrides_config_file() {
    clear_podbot_env();

    let toml_content = r#"
        engine_socket = "unix:///from/config/file.sock"
        image = "file-image:v1"
    "#;
    let config_file = temp_config_file(toml_content).expect("failed to create temp config");
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
#[serial]
fn load_config_handles_missing_config_file_gracefully() {
    clear_podbot_env();

    // Point to a non-existent config file.
    let cli = cli_with_config(Some(Utf8PathBuf::from("/nonexistent/config.toml")));

    // Should succeed (missing file is OK, falls back to defaults).
    let config = load_config(&cli).expect("load_config should succeed for missing file");

    // All defaults should apply.
    assert!(config.engine_socket.is_none());
}

#[test]
#[serial]
fn load_config_rejects_malformed_config_file() {
    clear_podbot_env();

    let toml_content = r"
        this is not valid TOML {{{
    ";
    let config_file = temp_config_file(toml_content).expect("failed to create temp config");
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
#[serial]
fn load_config_preserves_nested_config_defaults() {
    clear_podbot_env();

    // Only set top-level fields, nested should get defaults.
    let toml_content = r#"
        engine_socket = "unix:///test.sock"
    "#;
    let config_file = temp_config_file(toml_content).expect("failed to create temp config");
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

#[test]
#[serial]
fn load_config_fails_on_invalid_bool_env_var() {
    clear_podbot_env();

    // SAFETY: Tests are run serially via `#[serial]` attribute.
    unsafe {
        std::env::set_var("PODBOT_SANDBOX_PRIVILEGED", "maybe");
    }

    let cli = cli_with_config(None);
    let result = load_config(&cli);

    let err = result.expect_err("load_config should fail for invalid bool");
    let err_str = err.to_string();
    assert!(
        err_str.contains("PODBOT_SANDBOX_PRIVILEGED"),
        "error should mention the env var: {err_str}"
    );
    assert!(
        err_str.contains("expected bool"),
        "error should explain expected type: {err_str}"
    );
}

#[test]
#[serial]
fn load_config_fails_on_invalid_u64_env_var() {
    clear_podbot_env();

    // SAFETY: Tests are run serially via `#[serial]` attribute.
    unsafe {
        std::env::set_var("PODBOT_GITHUB_APP_ID", "not-a-number");
    }

    let cli = cli_with_config(None);
    let result = load_config(&cli);

    let err = result.expect_err("load_config should fail for invalid integer");
    let err_str = err.to_string();
    assert!(
        err_str.contains("PODBOT_GITHUB_APP_ID"),
        "error should mention the env var: {err_str}"
    );
    assert!(
        err_str.contains("expected unsigned integer"),
        "error should explain expected type: {err_str}"
    );
}

#[test]
#[serial]
fn load_config_accepts_valid_bool_env_var() {
    clear_podbot_env();

    // SAFETY: Tests are run serially via `#[serial]` attribute.
    unsafe {
        std::env::set_var("PODBOT_SANDBOX_PRIVILEGED", "true");
    }

    let cli = cli_with_config(None);
    let config = load_config(&cli).expect("load_config should succeed for valid bool");

    assert!(config.sandbox.privileged);
}

#[test]
#[serial]
fn load_config_accepts_valid_u64_env_var() {
    clear_podbot_env();

    // SAFETY: Tests are run serially via `#[serial]` attribute.
    unsafe {
        std::env::set_var("PODBOT_GITHUB_APP_ID", "12345");
    }

    let cli = cli_with_config(None);
    let config = load_config(&cli).expect("load_config should succeed for valid u64");

    assert_eq!(config.github.app_id, Some(12345));
}
