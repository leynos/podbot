//! Unit tests for token-daemon orchestration helpers.

use std::path::PathBuf;

use camino::Utf8PathBuf;
use rstest::rstest;

use super::*;
use crate::config::{GitHubConfig, SandboxConfig};
use crate::error::GitHubError;

#[test]
fn blank_container_id_is_rejected() {
    let result = validate_container_id("   ");

    assert!(
        matches!(
            result,
            Err(PodbotError::Config(ConfigError::MissingRequired { ref field }))
                if field == "container_id"
        ),
        "expected container_id rejection, got {result:?}"
    );
}

#[test]
fn complete_credentials_are_extracted() {
    let config = AppConfig {
        github: GitHubConfig {
            app_id: Some(1),
            installation_id: Some(2),
            private_key_path: Some(Utf8PathBuf::from("/keys/app.pem")),
        },
        sandbox: SandboxConfig::default(),
        ..AppConfig::default()
    };

    let (app_id, installation_id, path) =
        github_credentials(&config).expect("complete credentials should resolve");

    assert_eq!(app_id, 1);
    assert_eq!(installation_id, 2);
    assert_eq!(path.as_str(), "/keys/app.pem");
}

#[rstest]
#[case::missing_app_id(None, Some(2), Some("/keys/app.pem"), "github.app_id")]
#[case::missing_installation_id(Some(1), None, Some("/keys/app.pem"), "github.installation_id")]
#[case::missing_private_key(Some(1), Some(2), None, "github.private_key_path")]
fn incomplete_credentials_are_rejected(
    #[case] app_id: Option<u64>,
    #[case] installation_id: Option<u64>,
    #[case] private_key_path: Option<&str>,
    #[case] expected_field: &str,
) {
    let config = AppConfig {
        github: GitHubConfig {
            app_id,
            installation_id,
            private_key_path: private_key_path.map(Utf8PathBuf::from),
        },
        sandbox: SandboxConfig::default(),
        ..AppConfig::default()
    };

    let result = github_credentials(&config);

    assert!(
        matches!(
            result,
            Err(PodbotError::Config(ConfigError::MissingRequired { ref field }))
                if field == expected_field
        ),
        "expected a missing {expected_field} error, got {result:?}"
    );
}

#[test]
fn daemon_success_requires_cleanup_success() {
    let outcome =
        finish_with_cleanup(Ok(()), Ok(())).expect("successful daemon and cleanup should succeed");
    assert_eq!(outcome, CommandOutcome::Success);

    let cleanup_error = PodbotError::from(GitHubError::TokenStoreCleanupFailed {
        path: PathBuf::from("/run/user/1000/podbot/session"),
        message: String::from("permission denied"),
    });
    let result = finish_with_cleanup(Ok(()), Err(cleanup_error));

    assert!(
        matches!(
            result,
            Err(PodbotError::GitHub(
                GitHubError::TokenStoreCleanupFailed { .. }
            ))
        ),
        "cleanup failure should surface, got {result:?}"
    );
}

#[test]
fn daemon_failure_is_preserved_when_cleanup_also_fails() {
    let daemon_error = PodbotError::from(GitHubError::DaemonFailed {
        message: String::from("out of retries"),
    });
    let cleanup_error = PodbotError::from(GitHubError::TokenStoreCleanupFailed {
        path: PathBuf::from("/run/user/1000/podbot/session"),
        message: String::from("permission denied"),
    });

    let result = finish_with_cleanup(Err(daemon_error), Err(cleanup_error));

    assert!(
        matches!(
            result,
            Err(PodbotError::GitHub(GitHubError::DaemonFailed { .. }))
        ),
        "daemon failure should take precedence, got {result:?}"
    );
}
