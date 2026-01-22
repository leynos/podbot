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

/// Test that environment layer overrides file layer.
#[rstest]
fn layer_precedence_env_overrides_file() {
    let composer = create_composer_with_file_and_env().expect("composer creation should succeed");
    let config = merge_config(composer).expect("merge should succeed");

    // Environment overrides file for engine_socket
    assert_eq!(
        config.engine_socket.as_deref(),
        Some("unix:///from/env.sock")
    );
    // File value preserved for image (not in env layer)
    assert_eq!(config.image.as_deref(), Some("file-image:latest"));
}

/// Test that CLI layer overrides all other layers.
#[rstest]
fn layer_precedence_cli_overrides_all() {
    let mut composer =
        create_composer_with_file_and_env().expect("composer creation should succeed");
    composer.push_cli(json!({
        "engine_socket": "unix:///from/cli.sock"
    }));

    let config = merge_config(composer).expect("merge should succeed");

    // CLI overrides everything for engine_socket
    assert_eq!(
        config.engine_socket.as_deref(),
        Some("unix:///from/cli.sock")
    );
    // File value preserved for image (not in env or CLI layers)
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

/// Test that missing layers result in defaults being used.
#[rstest]
fn layer_precedence_empty_layers_use_defaults() {
    let mut composer = create_composer_with_defaults().expect("composer creation should succeed");
    // Add empty override layers (no effect on values)
    composer.push_file(json!({}), None);
    composer.push_environment(json!({}));
    composer.push_cli(json!({}));

    let config = merge_config(composer).expect("merge should succeed");

    assert_config_has_defaults(&config);
}

/// Test that empty JSON defaults do NOT work - serialised `AppConfig::default()` is required.
///
/// This test verifies that using `push_defaults(json!({}))` fails to produce a valid
/// configuration. OrthoConfig requires fully-specified defaults from the serialized
/// `AppConfig::default()` value. Empty JSON would result in null/missing fields that
/// cannot be deserialized into the target struct.
///
/// This documents why the production loader MUST use the serialized defaults approach
/// rather than relying on serde's `#[serde(default)]` during deserialization.
#[rstest]
fn layer_precedence_empty_json_defaults_fails() {
    // Empty JSON defaults should fail to produce a valid config.
    let mut empty_composer = MergeComposer::new();
    empty_composer.push_defaults(json!({}));

    let result = AppConfig::merge_from_layers(empty_composer.layers());

    // The merge should fail because empty JSON doesn't provide required defaults.
    assert!(
        result.is_err(),
        "empty JSON defaults should fail; production MUST serialize AppConfig::default()"
    );
}

/// Test that serialised `AppConfig::default()` works correctly as a defaults layer.
///
/// This is the correct approach used by the production `load_config` function.
/// Contrast with `layer_precedence_empty_json_defaults_fails` which demonstrates
/// that empty JSON does NOT work.
#[rstest]
fn layer_precedence_serialised_defaults_works() {
    // Production approach: serialise AppConfig::default() as the defaults layer.
    let composer = create_composer_with_defaults().expect("composer creation should succeed");
    let config = merge_config(composer).expect("merge should succeed");

    // Verify the config matches the expected defaults.
    assert_config_has_defaults(&config);
}
