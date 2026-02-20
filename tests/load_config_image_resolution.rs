//! Integration tests for configuration-driven container image resolution.
//!
//! These tests verify that layered configuration resolves `AppConfig.image`
//! correctly and that `CreateContainerRequest` consumes that resolved value.

mod test_utils;

use std::io::Write;

use camino::Utf8PathBuf;
use podbot::config::{AppConfig, Cli, Commands, load_config};
use podbot::engine::CreateContainerRequest;
use podbot::error::{ConfigError, PodbotError};
use rstest::rstest;
use serial_test::serial;
use tempfile::NamedTempFile;

use crate::test_utils::{EnvGuard, clean_env, set_env_var};

fn cli_with_image(config_path: Option<Utf8PathBuf>, image: Option<&str>) -> Cli {
    Cli {
        config: config_path,
        engine_socket: None,
        image: image.map(str::to_owned),
        command: Commands::Ps,
    }
}

fn temp_config_file(content: &str) -> std::io::Result<NamedTempFile> {
    let mut file = NamedTempFile::new()?;
    file.write_all(content.as_bytes())?;
    Ok(file)
}

#[expect(clippy::expect_used, reason = "test helper - panics are acceptable")]
fn config_file_with_image(image: Option<&str>) -> (Option<NamedTempFile>, Option<Utf8PathBuf>) {
    let Some(file_image) = image else {
        return (None, None);
    };

    let config_file = temp_config_file(&format!("image = \"{file_image}\"\n"))
        .expect("temp config file creation");
    let config_path = Utf8PathBuf::try_from(config_file.path().to_path_buf())
        .expect("path should be valid UTF-8");
    (Some(config_file), Some(config_path))
}

#[expect(clippy::expect_used, reason = "test helper - panics are acceptable")]
fn load_config_for_image_layers(
    clean_env: &EnvGuard<'_>,
    file_image: Option<&str>,
    env_image: Option<&str>,
    cli_image: Option<&str>,
) -> AppConfig {
    let (_config_file, config_path) = config_file_with_image(file_image);

    if let Some(image) = env_image {
        set_env_var(clean_env, "PODBOT_IMAGE", image);
    }

    let cli = cli_with_image(config_path, cli_image);
    load_config(&cli).expect("load_config should succeed for image layer test")
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
    cli: Option<&'static str>,
    expected: Option<&'static str>,
}

#[rstest]
#[case::file_only(ImageResolutionCase {
    file: Some("file-image:v1"),
    env: None,
    cli: None,
    expected: Some("file-image:v1"),
})]
#[case::env_overrides_file(ImageResolutionCase {
    file: Some("file-image:v1"),
    env: Some("env-image:v2"),
    cli: None,
    expected: Some("env-image:v2"),
})]
#[case::cli_overrides_env_and_file(ImageResolutionCase {
    file: Some("file-image:v1"),
    env: Some("env-image:v2"),
    cli: Some("cli-image:v3"),
    expected: Some("cli-image:v3"),
})]
#[serial]
fn load_config_resolves_image_with_layer_precedence(
    clean_env: EnvGuard<'static>,
    #[case] case: ImageResolutionCase,
) {
    let config = load_config_for_image_layers(&clean_env, case.file, case.env, case.cli);
    assert_eq!(config.image.as_deref(), case.expected);
}

struct ResolvedImageCase {
    file: Option<&'static str>,
    env: Option<&'static str>,
    cli: Option<&'static str>,
    expected: &'static str,
}

#[rstest]
#[case::from_file(ResolvedImageCase {
    file: Some("file-image:v1"),
    env: None,
    cli: None,
    expected: "file-image:v1",
})]
#[case::from_env(ResolvedImageCase {
    file: Some("file-image:v1"),
    env: Some("env-image:v2"),
    cli: None,
    expected: "env-image:v2",
})]
#[case::from_cli(ResolvedImageCase {
    file: Some("file-image:v1"),
    env: Some("env-image:v2"),
    cli: Some("cli-image:v3"),
    expected: "cli-image:v3",
})]
#[serial]
fn resolved_config_image_is_used_for_container_request(
    clean_env: EnvGuard<'static>,
    #[case] case: ResolvedImageCase,
) {
    let config = load_config_for_image_layers(&clean_env, case.file, case.env, case.cli);
    let request = request_from_resolved_config(&config)
        .expect("request should be created from non-empty resolved image");
    assert_eq!(request.image(), case.expected);
}

#[rstest]
#[case::missing_from_all_layers(ImageResolutionCase {
    file: None,
    env: None,
    cli: None,
    expected: None,
})]
#[case::blank_from_env(ImageResolutionCase {
    file: None,
    env: Some("   "),
    cli: None,
    expected: Some("   "),
})]
#[case::blank_cli_overrides_file(ImageResolutionCase {
    file: Some("file-image:v1"),
    env: Some("env-image:v2"),
    cli: Some("   "),
    expected: Some("   "),
})]
#[serial]
fn resolved_config_image_missing_or_blank_fails_container_request(
    clean_env: EnvGuard<'static>,
    #[case] case: ImageResolutionCase,
) {
    let config = load_config_for_image_layers(&clean_env, case.file, case.env, case.cli);
    assert_eq!(config.image.as_deref(), case.expected);

    let error = request_from_resolved_config(&config)
        .expect_err("request construction should fail when resolved image is missing/blank");
    assert_missing_image_error(&error);
}
