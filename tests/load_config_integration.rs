//! Integration tests for the `podbot::config::load_config_*` public APIs.
//!
//! These tests validate the end-to-end behaviour of the library-facing config
//! loader, using explicit load options and injected environment values. The
//! tests deliberately avoid Clap parse types so they exercise the embedding
//! surface rather than the CLI adapter.

mod test_support;

use std::io::Write;

use crate::test_support::env_with;
use camino::Utf8PathBuf;
use podbot::config::{ConfigLoadOptions, ConfigOverrides, SelinuxLabelMode, load_config_with_env};
use rstest::rstest;
use tempfile::NamedTempFile;

/// Helper: Creates a temporary config file with the given TOML content.
fn temp_config_file(content: &str) -> std::io::Result<NamedTempFile> {
    let mut file = NamedTempFile::new()?;
    file.write_all(content.as_bytes())?;
    Ok(file)
}

#[rstest]
fn load_config_returns_defaults_when_no_sources_provided() {
    let env = env_with(&[]);
    let options = ConfigLoadOptions {
        discover_config: false,
        ..ConfigLoadOptions::default()
    };

    let config =
        load_config_with_env(&env, &options).expect("load_config should succeed with defaults");

    // Verify key defaults.
    assert!(config.engine_socket.is_none());
    assert!(config.image.is_none());
    assert!(!config.sandbox.privileged);
    assert!(config.sandbox.mount_dev_fuse);
    assert_eq!(config.workspace.base_dir.as_str(), "/work");
}

#[rstest]
fn load_config_loads_from_config_file() {
    let toml_content = r#"
        engine_socket = "unix:///from/config/file.sock"
        image = "test-image:v1"

        [sandbox]
        privileged = true
    "#;
    let config_file =
        temp_config_file(toml_content).expect("temp config file creation should succeed");
    let config_path = Utf8PathBuf::try_from(config_file.path().to_path_buf())
        .expect("path should be valid UTF-8");

    let env = env_with(&[]);
    let options = ConfigLoadOptions {
        config_path_hint: Some(config_path),
        discover_config: false,
        ..ConfigLoadOptions::default()
    };
    let config = load_config_with_env(&env, &options).expect("load_config should succeed");

    assert_eq!(
        config.engine_socket.as_deref(),
        Some("unix:///from/config/file.sock")
    );
    assert_eq!(config.image.as_deref(), Some("test-image:v1"));
    assert!(config.sandbox.privileged);
    // Defaults should still apply for unset fields.
    assert!(config.sandbox.mount_dev_fuse);
}

#[rstest]
#[case(
    "unix:///from/podbot_config_path.sock",
    None,
    true,
    Some("unix:///from/podbot_config_path.sock")
)]
#[case(
    "unix:///from/podbot_config_path_no_discovery.sock",
    None,
    false,
    Some("unix:///from/podbot_config_path_no_discovery.sock")
)]
#[case(
    "unix:///should_not_be_used.sock",
    Some("/nonexistent/config.toml"),
    false,
    None
)]
fn load_config_podbot_config_path_behaviour(
    #[case] socket_in_env_config: &str,
    #[case] hint: Option<&str>,
    #[case] discover_config: bool,
    #[case] expected_socket: Option<&str>,
) {
    let toml_content = format!(r#"engine_socket = "{socket_in_env_config}""#);
    let config_file =
        temp_config_file(&toml_content).expect("temp config file creation should succeed");
    let config_path = Utf8PathBuf::try_from(config_file.path().to_path_buf())
        .expect("path should be valid UTF-8");
    let env_values = [("PODBOT_CONFIG_PATH", config_path.as_str())];
    let env = env_with(&env_values);

    let options = ConfigLoadOptions {
        config_path_hint: hint.map(Utf8PathBuf::from),
        discover_config,
        ..ConfigLoadOptions::default()
    };
    let config = load_config_with_env(&env, &options).expect("load_config should succeed");

    assert_eq!(
        config.engine_socket.as_deref(),
        expected_socket,
        "unexpected engine_socket for hint={hint:?}, discover_config={discover_config}"
    );
}

#[rstest]
fn load_config_overrides_take_precedence_over_config_file() {
    let toml_content = r#"
        engine_socket = "unix:///from/config/file.sock"
        image = "file-image:v1"
    "#;
    let config_file =
        temp_config_file(toml_content).expect("temp config file creation should succeed");
    let config_path = Utf8PathBuf::try_from(config_file.path().to_path_buf())
        .expect("path should be valid UTF-8");

    let env = env_with(&[]);
    let options = ConfigLoadOptions {
        config_path_hint: Some(config_path),
        discover_config: false,
        overrides: ConfigOverrides {
            engine_socket: Some(String::from("unix:///from/overrides.sock")),
            image: None,
        },
    };
    let config = load_config_with_env(&env, &options).expect("load_config should succeed");

    // Overrides win for engine_socket.
    assert_eq!(
        config.engine_socket.as_deref(),
        Some("unix:///from/overrides.sock")
    );
    // File value preserved for image.
    assert_eq!(config.image.as_deref(), Some("file-image:v1"));
}

#[rstest]
fn load_config_handles_missing_config_file_gracefully() {
    let env = env_with(&[]);

    // Point to a non-existent config file.
    let options = ConfigLoadOptions {
        config_path_hint: Some(Utf8PathBuf::from("/nonexistent/config.toml")),
        discover_config: false,
        ..ConfigLoadOptions::default()
    };

    // Should succeed (missing file is OK, falls back to defaults).
    let config =
        load_config_with_env(&env, &options).expect("load_config should succeed for missing file");

    // All defaults should apply.
    assert!(config.engine_socket.is_none());
}

#[rstest]
fn load_config_rejects_malformed_config_file() {
    let toml_content = r"
        this is not valid TOML {{{
    ";
    let config_file =
        temp_config_file(toml_content).expect("temp config file creation should succeed");
    let config_path = Utf8PathBuf::try_from(config_file.path().to_path_buf())
        .expect("path should be valid UTF-8");

    let env = env_with(&[]);
    let options = ConfigLoadOptions {
        config_path_hint: Some(config_path),
        discover_config: false,
        ..ConfigLoadOptions::default()
    };
    let result = load_config_with_env(&env, &options);

    assert!(
        result.is_err(),
        "load_config should fail for malformed TOML"
    );
}

#[rstest]
fn load_config_preserves_nested_config_defaults() {
    // Only set top-level fields, nested should get defaults.
    let toml_content = r#"
        engine_socket = "unix:///test.sock"
    "#;
    let config_file =
        temp_config_file(toml_content).expect("temp config file creation should succeed");
    let config_path = Utf8PathBuf::try_from(config_file.path().to_path_buf())
        .expect("path should be valid UTF-8");

    let env = env_with(&[]);
    let options = ConfigLoadOptions {
        config_path_hint: Some(config_path),
        discover_config: false,
        ..ConfigLoadOptions::default()
    };
    let config = load_config_with_env(&env, &options).expect("load_config should succeed");

    // Top-level from file.
    assert_eq!(config.engine_socket.as_deref(), Some("unix:///test.sock"));

    // Nested defaults preserved.
    assert!(!config.sandbox.privileged);
    assert!(config.sandbox.mount_dev_fuse);
    assert_eq!(config.workspace.base_dir.as_str(), "/work");
    assert!(config.creds.copy_claude);
    assert!(config.creds.copy_codex);
}

#[rstest]
#[case("PODBOT_SANDBOX_PRIVILEGED", "maybe", "expected bool")]
#[case("PODBOT_GITHUB_APP_ID", "not-a-number", "expected unsigned integer")]
fn load_config_fails_on_invalid_typed_env_var(
    #[case] env_var: &str,
    #[case] invalid_value: &str,
    #[case] expected_type: &str,
) {
    let env = env_with(&[(env_var, invalid_value)]);
    let options = ConfigLoadOptions {
        discover_config: false,
        ..ConfigLoadOptions::default()
    };
    let result = load_config_with_env(&env, &options);

    let err = result.expect_err("load_config should fail for invalid typed env var");
    let err_str = err.to_string();
    assert!(
        err_str.contains(env_var),
        "error should mention env var: {err_str}"
    );
    assert!(
        err_str.contains(expected_type),
        "error should explain expected type: {err_str}"
    );
}

#[rstest]
#[case("PODBOT_SANDBOX_PRIVILEGED", "true")]
#[case("PODBOT_GITHUB_APP_ID", "12345")]
#[case("PODBOT_SANDBOX_SELINUX_LABEL_MODE", "keep_default")]
fn load_config_accepts_valid_typed_env_var(#[case] env_var: &str, #[case] value: &str) {
    let env = env_with(&[(env_var, value)]);
    let options = ConfigLoadOptions {
        discover_config: false,
        ..ConfigLoadOptions::default()
    };
    let config = load_config_with_env(&env, &options)
        .expect("load_config should succeed for valid typed env var");

    match env_var {
        "PODBOT_SANDBOX_PRIVILEGED" => {
            assert!(
                config.sandbox.privileged,
                "sandbox.privileged should be true"
            );
        }
        "PODBOT_GITHUB_APP_ID" => {
            assert_eq!(
                config.github.app_id,
                Some(12345),
                "github.app_id should be Some(12345)"
            );
        }
        "PODBOT_SANDBOX_SELINUX_LABEL_MODE" => {
            assert_eq!(
                config.sandbox.selinux_label_mode,
                SelinuxLabelMode::KeepDefault,
                "sandbox.selinux_label_mode should be KeepDefault"
            );
        }
        _ => panic!("unexpected env var in test: {env_var}"),
    }
}

#[rstest]
fn load_config_rejects_invalid_selinux_label_mode_env_var() {
    let env = env_with(&[("PODBOT_SANDBOX_SELINUX_LABEL_MODE", "banana")]);
    let options = ConfigLoadOptions {
        discover_config: false,
        ..ConfigLoadOptions::default()
    };
    let result = load_config_with_env(&env, &options);

    assert!(
        result.is_err(),
        "load_config should fail for invalid selinux_label_mode"
    );
}
