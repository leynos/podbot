//! Integration tests for configuration-driven container image resolution.
//!
//! These tests verify that layered configuration resolves `AppConfig.image`
//! correctly and that `CreateContainerRequest` consumes that resolved value.

#![cfg(feature = "internal")]

mod test_support;

use crate::test_support::{env_with, temp_config_path};
use camino::Utf8PathBuf;
use mockable::MockEnv;
use podbot::config::{
    AppConfig, CommandIntent, ConfigLoadOptions, ConfigOverrides, load_config_with_env,
};
use podbot::engine::CreateContainerRequest;
use podbot::error::{ConfigError, PodbotError};
use rstest::rstest;
use tempfile::NamedTempFile;

/// Creates a temporary config file naming `image`, or nothing when `image` is
/// `None`.
fn config_file_with_image(
    image: Option<&str>,
) -> std::io::Result<(Option<NamedTempFile>, Option<Utf8PathBuf>)> {
    let Some(file_image) = image else {
        return Ok((None, None));
    };

    let (config_file, config_path) = temp_config_path(&format!("image = \"{file_image}\"\n"))?;
    Ok((Some(config_file), Some(config_path)))
}

/// Loads configuration with the image supplied by the requested layers.
fn load_config_for_image_layers(
    env: &MockEnv,
    file_image: Option<&str>,
    overrides_image: Option<&str>,
) -> Result<AppConfig, String> {
    let (_config_file, config_path) =
        config_file_with_image(file_image).map_err(|error| error.to_string())?;

    let options = ConfigLoadOptions {
        config_path_hint: config_path,
        discover_config: false,
        overrides: ConfigOverrides {
            engine_socket: None,
            image: overrides_image.map(str::to_owned),
            agent_kind: None,
            agent_mode: None,
        },
        command_intent: CommandIntent::Any,
    };

    load_config_with_env(env, &options).map_err(|error| error.to_string())
}

fn request_from_resolved_config(config: &AppConfig) -> Result<CreateContainerRequest, PodbotError> {
    CreateContainerRequest::from_app_config(config)
}

fn assert_missing_image_error(error: &PodbotError) {
    assert!(
        matches!(
            error,
            PodbotError::Config(ConfigError::MissingRequired { field }) if field == "image"
        ),
        "expected missing image validation error, got: {error:?}"
    );
}

struct ImageResolutionCase {
    file: Option<&'static str>,
    env: Option<&'static str>,
    overrides: Option<&'static str>,
    expected: Option<&'static str>,
}

#[rstest]
#[case::file_only(ImageResolutionCase {
    file: Some("file-image:v1"),
    env: None,
    overrides: None,
    expected: Some("file-image:v1"),
})]
#[case::env_overrides_file(ImageResolutionCase {
    file: Some("file-image:v1"),
    env: Some("env-image:v2"),
    overrides: None,
    expected: Some("env-image:v2"),
})]
#[case::overrides_overrides_env_and_file(ImageResolutionCase {
    file: Some("file-image:v1"),
    env: Some("env-image:v2"),
    overrides: Some("overrides-image:v3"),
    expected: Some("overrides-image:v3"),
})]
fn load_config_resolves_image_with_layer_precedence(#[case] case: ImageResolutionCase) {
    let env = env_with(
        &case
            .env
            .map_or_else(Vec::new, |value| vec![("PODBOT_IMAGE", value)]),
    );
    let config = load_config_for_image_layers(&env, case.file, case.overrides)
        .expect("load_config should succeed for image layer test");
    assert_eq!(config.image.as_deref(), case.expected);
}

struct ResolvedImageCase {
    file: Option<&'static str>,
    env: Option<&'static str>,
    overrides: Option<&'static str>,
    expected: &'static str,
}

#[rstest]
#[case::from_file(ResolvedImageCase {
    file: Some("file-image:v1"),
    env: None,
    overrides: None,
    expected: "file-image:v1",
})]
#[case::from_env(ResolvedImageCase {
    file: Some("file-image:v1"),
    env: Some("env-image:v2"),
    overrides: None,
    expected: "env-image:v2",
})]
#[case::from_overrides(ResolvedImageCase {
    file: Some("file-image:v1"),
    env: Some("env-image:v2"),
    overrides: Some("overrides-image:v3"),
    expected: "overrides-image:v3",
})]
fn resolved_config_image_is_used_for_container_request(#[case] case: ResolvedImageCase) {
    let env = env_with(
        &case
            .env
            .map_or_else(Vec::new, |value| vec![("PODBOT_IMAGE", value)]),
    );
    let config = load_config_for_image_layers(&env, case.file, case.overrides)
        .expect("load_config should succeed for image layer test");
    let request = request_from_resolved_config(&config)
        .expect("request should be created from non-empty resolved image");
    assert_eq!(request.image(), case.expected);
}

#[rstest]
#[case::missing_from_all_layers(ImageResolutionCase {
    file: None,
    env: None,
    overrides: None,
    expected: None,
})]
#[case::blank_from_env(ImageResolutionCase {
    file: None,
    env: Some("   "),
    overrides: None,
    expected: Some("   "),
})]
#[case::blank_overrides_overrides_file(ImageResolutionCase {
    file: Some("file-image:v1"),
    env: Some("env-image:v2"),
    overrides: Some("   "),
    expected: Some("   "),
})]
fn resolved_config_image_missing_or_blank_fails_container_request(
    #[case] case: ImageResolutionCase,
) {
    let env = env_with(
        &case
            .env
            .map_or_else(Vec::new, |value| vec![("PODBOT_IMAGE", value)]),
    );
    let config = load_config_for_image_layers(&env, case.file, case.overrides)
        .expect("load_config should succeed for image layer test");
    assert_eq!(config.image.as_deref(), case.expected);

    let error = request_from_resolved_config(&config)
        .expect_err("request construction should fail when resolved image is missing/blank");
    assert_missing_image_error(&error);
}
