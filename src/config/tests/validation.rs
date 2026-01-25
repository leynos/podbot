//! `GitHubConfig` validation tests.

use camino::Utf8PathBuf;
use rstest::rstest;

#[rstest]
fn github_config_validate_succeeds_when_complete() {
    let config = crate::config::GitHubConfig {
        app_id: Some(12345),
        installation_id: Some(67890),
        private_key_path: Some(Utf8PathBuf::from("/path/to/key.pem")),
    };
    let result = config.validate();
    assert!(
        result.is_ok(),
        "Expected validation to succeed for complete config"
    );
}

#[rstest]
#[case(
    None,
    None,
    None,
    "github.app_id, github.installation_id, github.private_key_path"
)]
#[case(
    Some(123),
    None,
    None,
    "github.installation_id, github.private_key_path"
)]
#[case(None, Some(456), None, "github.app_id, github.private_key_path")]
#[case(
    None,
    None,
    Some(Utf8PathBuf::from("/k.pem")),
    "github.app_id, github.installation_id"
)]
#[case(Some(123), Some(456), None, "github.private_key_path")]
#[case(
    Some(123),
    None,
    Some(Utf8PathBuf::from("/k.pem")),
    "github.installation_id"
)]
// Zero values are treated as missing (GitHub never issues ID 0)
#[case(
    Some(0),
    Some(67890),
    Some(Utf8PathBuf::from("/k.pem")),
    "github.app_id"
)]
#[case(
    Some(12345),
    Some(0),
    Some(Utf8PathBuf::from("/k.pem")),
    "github.installation_id"
)]
#[case(
    Some(0),
    Some(0),
    Some(Utf8PathBuf::from("/k.pem")),
    "github.app_id, github.installation_id"
)]
fn github_config_validate_reports_missing_fields(
    #[case] app_id: Option<u64>,
    #[case] installation_id: Option<u64>,
    #[case] private_key_path: Option<Utf8PathBuf>,
    #[case] expected_fields: &str,
) {
    let config = crate::config::GitHubConfig {
        app_id,
        installation_id,
        private_key_path,
    };
    let result = config.validate();
    let error = result.expect_err("validation should fail with missing fields");
    match error {
        crate::error::PodbotError::Config(crate::error::ConfigError::MissingRequired { field }) => {
            assert_eq!(
                field, expected_fields,
                "Field mismatch: expected '{expected_fields}', got '{field}'"
            );
        }
        other => panic!("Expected ConfigError::MissingRequired, got: {other:?}"),
    }
}

#[rstest]
fn github_config_is_configured_true_when_complete() {
    let config = crate::config::GitHubConfig {
        app_id: Some(12345),
        installation_id: Some(67890),
        private_key_path: Some(Utf8PathBuf::from("/path/to/key.pem")),
    };
    assert!(config.is_configured());
}

#[rstest]
fn github_config_is_configured_false_when_default() {
    let config = crate::config::GitHubConfig::default();
    assert!(!config.is_configured());
}

#[rstest]
fn github_config_is_configured_false_when_partial() {
    let config = crate::config::GitHubConfig {
        app_id: Some(12345),
        installation_id: None,
        private_key_path: Some(Utf8PathBuf::from("/path/to/key.pem")),
    };
    assert!(!config.is_configured());
}

#[rstest]
#[case(Some(0), Some(67890))]
#[case(Some(12345), Some(0))]
fn github_config_is_configured_false_when_id_is_zero(
    #[case] app_id: Option<u64>,
    #[case] installation_id: Option<u64>,
) {
    let config = crate::config::GitHubConfig {
        app_id,
        installation_id,
        private_key_path: Some(Utf8PathBuf::from("/path/to/key.pem")),
    };
    assert!(!config.is_configured());
}
