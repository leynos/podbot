//! Layer precedence tests for `MergeComposer` config composition.

use crate::config::AppConfig;
use crate::config::tests::helpers::{
    assert_config_has_defaults, create_composer_with_defaults, create_composer_with_file_and_env,
    merge_config,
};
use ortho_config::MergeComposer;
use ortho_config::serde_json::json;
use rstest::rstest;

/// Test that serialised `AppConfig::default()` can round-trip through `MergeComposer`.
///
/// This mirrors the production `load_config` behaviour, which serialises
/// `AppConfig::default()` as the defaults layer.
#[rstest]
fn layer_precedence_serialised_defaults_round_trip() {
    // This is exactly what load_config does: serialise defaults, push to composer.
    let composer = create_composer_with_defaults().expect("composer creation should succeed");
    let config = merge_config(composer).expect("merge should succeed");
    let expected = AppConfig::default();

    // Verify key fields match to ensure the serialisation round-trip works.
    assert_eq!(config.engine_socket, expected.engine_socket);
    assert_eq!(config.image, expected.image);
    assert_eq!(config.sandbox.privileged, expected.sandbox.privileged);
    assert_eq!(
        config.sandbox.mount_dev_fuse,
        expected.sandbox.mount_dev_fuse
    );
    assert_eq!(config.workspace.base_dir, expected.workspace.base_dir);
    assert_eq!(config.agent.kind, expected.agent.kind);
    assert_eq!(config.agent.mode, expected.agent.mode);
    assert_eq!(config.creds.copy_claude, expected.creds.copy_claude);
    assert_eq!(config.creds.copy_codex, expected.creds.copy_codex);
}

/// Test that defaults layer provides baseline configuration values.
#[rstest]
fn layer_precedence_defaults_provide_baseline() {
    let composer = create_composer_with_defaults().expect("composer creation should succeed");
    let config = merge_config(composer).expect("merge should succeed");

    assert_config_has_defaults(&config);
}

/// Test that file layer overrides defaults.
#[rstest]
fn layer_precedence_file_overrides_defaults() {
    let mut composer = create_composer_with_defaults().expect("composer creation should succeed");
    composer.push_file(
        json!({
            "engine_socket": "unix:///from/file.sock",
            "image": "file-image:latest"
        }),
        None,
    );

    let config = merge_config(composer).expect("merge should succeed");

    assert_eq!(
        config.engine_socket.as_deref(),
        Some("unix:///from/file.sock")
    );
    assert_eq!(config.image.as_deref(), Some("file-image:latest"));
}

/// Parameterized test verifying layer precedence for env and CLI overrides.
///
/// This test consolidates the logic for testing that higher-precedence layers
/// (environment, CLI) correctly override lower layers (file) for `engine_socket`,
/// while preserving `image` from the file layer.
#[rstest]
#[case::env_overrides_file(None, "unix:///from/env.sock")]
#[case::cli_overrides_all(Some("unix:///from/cli.sock"), "unix:///from/cli.sock")]
fn layer_precedence_override_for_engine_socket(
    #[case] cli_override: Option<&str>,
    #[case] expected_socket: &str,
) {
    let mut composer =
        create_composer_with_file_and_env().expect("composer creation should succeed");

    if let Some(socket) = cli_override {
        composer.push_cli(json!({ "engine_socket": socket }));
    }

    let config = merge_config(composer).expect("merge should succeed");

    // Verify the expected layer wins for engine_socket
    assert_eq!(config.engine_socket.as_deref(), Some(expected_socket));
    // File value preserved for image (not overridden by env or CLI layers)
    assert_eq!(config.image.as_deref(), Some("file-image:latest"));
}

/// Test full precedence chain: defaults < file < env < CLI.
#[rstest]
fn layer_precedence_full_chain() {
    let mut composer = create_composer_with_defaults().expect("composer creation should succeed");

    // Layer 2: File provides base configuration
    composer.push_file(
        json!({
            "engine_socket": "file-socket",
            "image": "file-image",
            "sandbox": { "privileged": true },
            "github": { "app_id": 100 }
        }),
        None,
    );

    // Layer 3: Environment overrides some values
    composer.push_environment(json!({
        "image": "env-image",
        "github": { "app_id": 200, "installation_id": 300 }
    }));

    // Layer 4: CLI overrides the highest priority values
    composer.push_cli(json!({
        "engine_socket": "cli-socket"
    }));

    let config = merge_config(composer).expect("merge should succeed");

    // CLI wins for engine_socket
    assert_eq!(config.engine_socket.as_deref(), Some("cli-socket"));
    // Env wins for image
    assert_eq!(config.image.as_deref(), Some("env-image"));
    // File wins for sandbox.privileged (not overridden by higher layers)
    assert!(config.sandbox.privileged);
    // Env wins for github.app_id (higher than file, no CLI override)
    assert_eq!(config.github.app_id, Some(200));
    // Env provides github.installation_id
    assert_eq!(config.github.installation_id, Some(300));
}

/// Test that nested config merges correctly across layers.
#[rstest]
fn layer_precedence_nested_config_merges() {
    let mut composer = create_composer_with_defaults().expect("composer creation should succeed");
    composer.push_file(
        json!({
            "sandbox": {
                "privileged": true,
                "mount_dev_fuse": false
            }
        }),
        None,
    );
    composer.push_environment(json!({
        "sandbox": {
            "privileged": false
        }
    }));

    let config = merge_config(composer).expect("merge should succeed");

    // Environment overrides file for privileged
    assert!(!config.sandbox.privileged);
    // File value preserved for mount_dev_fuse (not in env layer)
    assert!(!config.sandbox.mount_dev_fuse);
}

/// Test that missing layers result in injected defaults being used.
#[rstest]
fn layer_precedence_empty_layers_use_defaults() {
    let config = AppConfig::merge_from_layers(MergeComposer::new().layers())
        .expect("merge should inject defaults when callers provide no layers");

    assert_config_has_defaults(&config);
}

/// Test that explicit empty defaults layers are still rejected.
///
/// `merge_from_layers` now injects `AppConfig::default()` for callers that omit
/// defaults entirely, but an explicit defaults layer must still contain the
/// serialized default value rather than an empty object.
#[rstest]
fn layer_precedence_empty_json_defaults_still_fail() {
    let mut empty_composer = MergeComposer::new();
    empty_composer.push_defaults(json!({}));

    let result = AppConfig::merge_from_layers(empty_composer.layers());

    assert!(
        result.is_err(),
        "explicit empty defaults layers should still be rejected"
    );
}

/// Test that caller-supplied defaults must match `AppConfig::default()`.
#[rstest]
fn layer_precedence_noncanonical_defaults_fail() {
    let mut composer = MergeComposer::new();
    composer.push_defaults(json!({
        "engine_socket": "unix:///custom.sock"
    }));

    let result = AppConfig::merge_from_layers(composer.layers());

    assert!(
        result.is_err(),
        "non-canonical defaults layers should be rejected"
    );
}

/// Test that serialised `AppConfig::default()` works correctly as a defaults layer.
///
/// This remains a valid explicit caller input, even though `merge_from_layers`
/// now injects the same defaults layer internally.
#[rstest]
fn layer_precedence_serialised_defaults_works() {
    // Production approach: serialise AppConfig::default() as the defaults layer.
    let composer = create_composer_with_defaults().expect("composer creation should succeed");
    let config = merge_config(composer).expect("merge should succeed");

    // Verify the config matches the expected defaults.
    assert_config_has_defaults(&config);
}
