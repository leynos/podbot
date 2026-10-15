//! Layer-precedence step definitions for podbot configuration behaviour
//! tests: merging file, environment, and CLI layers over defaults.

use ortho_config::MergeComposer;
use ortho_config::serde_json::json;
use podbot::config::AppConfig;
use rstest_bdd_macros::{given, then, when};

use super::{ConfigState, get_config};

/// Recursively merges two JSON values, combining nested objects field-by-field.
///
/// For object values, fields are merged recursively. For non-objects, the new
/// value completely overwrites the existing one. This mirrors how `OrthoConfig`
/// merges nested configuration structures.
fn merge_json_values(
    existing: &ortho_config::serde_json::Value,
    new_value: &ortho_config::serde_json::Value,
) -> ortho_config::serde_json::Value {
    use ortho_config::serde_json::Value;

    match (existing, new_value) {
        (Value::Object(existing_obj), Value::Object(new_obj)) => {
            let mut merged = existing_obj.clone();
            for (key, new_child) in new_obj {
                if let Some(existing_child) = merged.get(key) {
                    // Recursively merge nested objects; for non-objects the new value wins.
                    merged.insert(key.clone(), merge_json_values(existing_child, new_child));
                } else {
                    merged.insert(key.clone(), new_child.clone());
                }
            }
            Value::Object(merged)
        }
        // For non-object values, the new value completely overwrites the existing one.
        _ => new_value.clone(),
    }
}

/// Merges a new value into an existing layer slot (if present).
fn merge_layer(
    existing: Option<ortho_config::serde_json::Value>,
    new_value: ortho_config::serde_json::Value,
) -> ortho_config::serde_json::Value {
    if let Some(existing_value) = existing {
        merge_json_values(&existing_value, &new_value)
    } else {
        new_value
    }
}

/// Merges the current file layer with a new value (combining fields).
fn merge_file_layer(
    config_state: &ConfigState,
    new_value: ortho_config::serde_json::Value,
) -> ortho_config::serde_json::Value {
    merge_layer(config_state.file_layer.get(), new_value)
}

/// Merges the current env layer with a new value (combining fields).
fn merge_env_layer(
    config_state: &ConfigState,
    new_value: ortho_config::serde_json::Value,
) -> ortho_config::serde_json::Value {
    merge_layer(config_state.env_layer.get(), new_value)
}

#[given("defaults provide engine_socket as nil")]
fn defaults_provide_engine_socket_nil(config_state: &ConfigState) {
    // Defaults already have engine_socket as None, nothing to do.
    // Access config_state to satisfy clippy (rstest_bdd requires the parameter).
    drop(config_state.file_layer.get());
}

#[given("a file layer provides engine_socket as {socket}")]
fn file_layer_provides_engine_socket(config_state: &ConfigState, socket: String) {
    let merged = merge_file_layer(config_state, json!({ "engine_socket": socket }));
    config_state.file_layer.set(merged);
}

#[given("an environment layer provides engine_socket as {socket}")]
fn env_layer_provides_engine_socket(config_state: &ConfigState, socket: String) {
    let merged = merge_env_layer(config_state, json!({ "engine_socket": socket }));
    config_state.env_layer.set(merged);
}

#[given("a CLI layer provides engine_socket as {socket}")]
fn cli_layer_provides_engine_socket(config_state: &ConfigState, socket: String) {
    config_state
        .cli_layer
        .set(json!({ "engine_socket": socket }));
}

#[given("a file layer provides image as {image}")]
fn file_layer_provides_image(config_state: &ConfigState, image: String) {
    let merged = merge_file_layer(config_state, json!({ "image": image }));
    config_state.file_layer.set(merged);
}

#[given("a file layer provides sandbox.privileged as {value}")]
fn file_layer_provides_sandbox_privileged(config_state: &ConfigState, value: bool) {
    let merged = merge_file_layer(config_state, json!({ "sandbox": { "privileged": value } }));
    config_state.file_layer.set(merged);
}

#[given("a file layer provides sandbox.mount_dev_fuse as {value}")]
fn file_layer_provides_sandbox_mount_dev_fuse(config_state: &ConfigState, value: bool) {
    let merged = merge_file_layer(
        config_state,
        json!({ "sandbox": { "mount_dev_fuse": value } }),
    );
    config_state.file_layer.set(merged);
}

#[given("an environment layer provides sandbox.privileged as {value}")]
fn env_layer_provides_sandbox_privileged(config_state: &ConfigState, value: bool) {
    let merged = merge_env_layer(config_state, json!({ "sandbox": { "privileged": value } }));
    config_state.env_layer.set(merged);
}

#[when("configuration is merged")]
fn configuration_is_merged(config_state: &ConfigState) -> Result<(), String> {
    let mut composer = MergeComposer::new();

    // Layer 1: Defaults
    let defaults = ortho_config::serde_json::to_value(AppConfig::default())
        .map_err(|error| format!("default configuration serialization failed: {error}"))?;
    composer.push_defaults(defaults);

    // Layer 2: File
    if let Some(file_layer) = config_state.file_layer.get() {
        composer.push_file(file_layer, None);
    }

    // Layer 3: Environment
    if let Some(env_layer) = config_state.env_layer.get() {
        composer.push_environment(env_layer);
    }

    // Layer 4: CLI
    if let Some(cli_layer) = config_state.cli_layer.get() {
        composer.push_cli(cli_layer);
    }

    let config: AppConfig = podbot::config::merge_from_layers_for_tests(composer.layers())
        .map_err(|error| format!("configuration layer merge failed: {error}"))?;
    config_state.config.set(config);
    Ok(())
}

#[then("the engine socket is {socket}")]
fn engine_socket_is(config_state: &ConfigState, socket: String) -> Result<(), String> {
    let config = get_config(config_state)?;
    if config.engine_socket.as_deref() == Some(socket.as_str()) {
        Ok(())
    } else {
        Err(format!(
            "Expected engine_socket to be {socket}, got: {:?}",
            config.engine_socket
        ))
    }
}

#[then("the image is {image}")]
fn image_is(config_state: &ConfigState, image: String) -> Result<(), String> {
    let config = get_config(config_state)?;
    if config.image.as_deref() == Some(image.as_str()) {
        Ok(())
    } else {
        Err(format!(
            "Expected image to be {image}, got: {:?}",
            config.image
        ))
    }
}

#[cfg(test)]
mod tests {
    //! Regression tests for fallible configuration layer setup.

    use rstest::rstest;

    use super::{ConfigState, configuration_is_merged, json};

    #[rstest]
    fn invalid_layer_returns_a_merge_error() {
        let state = ConfigState::default();
        state.file_layer.set(json!({ "engine_socket": 123 }));
        let result = configuration_is_merged(&state);
        let error = result.expect_err("an invalid layer must fail setup");
        assert!(
            error.starts_with("configuration layer merge failed:"),
            "invalid layer setup should preserve merge context: {error}",
        );
        assert!(
            state.config.get().is_none(),
            "failed merge must not populate configuration"
        );
    }
}
