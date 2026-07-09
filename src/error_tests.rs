//! Unit tests for error type display formatting and conversion behaviour.

use super::*;
use eyre::Report;
use rstest::{fixture, rstest};

/// Fixture providing a sample configuration file path.
#[fixture]
fn config_path() -> PathBuf {
    PathBuf::from("/etc/podbot/config.toml")
}

/// Fixture providing a sample container socket path.
#[fixture]
fn socket_path() -> PathBuf {
    PathBuf::from("/run/podman/podman.sock")
}

/// Fixture providing a sample container ID.
#[fixture]
fn container_id() -> String {
    String::from("abc123")
}

#[rstest]
fn config_error_file_not_found_displays_correctly(config_path: PathBuf) {
    let error = ConfigError::FileNotFound { path: config_path };
    assert_eq!(
        error.to_string(),
        "configuration file not found: /etc/podbot/config.toml"
    );
}

#[rstest]
#[case(
    "port",
    "must be a positive integer",
    "invalid configuration value for 'port': must be a positive integer"
)]
#[case(
    "image",
    "cannot be empty",
    "invalid configuration value for 'image': cannot be empty"
)]
fn config_error_invalid_value_displays_correctly(
    #[case] field: &str,
    #[case] reason: &str,
    #[case] expected: &str,
) {
    let error = ConfigError::InvalidValue {
        field: String::from(field),
        reason: String::from(reason),
    };
    assert_eq!(error.to_string(), expected);
}

#[rstest]
fn config_error_parse_error_displays_message() {
    let error = ConfigError::ParseError {
        message: String::from("unexpected token"),
    };
    assert_eq!(
        error.to_string(),
        "failed to parse configuration file: unexpected token"
    );
}

#[rstest]
fn config_error_ortho_config_displays_correctly() {
    let ortho_error = ortho_config::OrthoError::Validation {
        key: String::from("github.app_id"),
        message: String::from("must be a positive integer"),
    };
    let error = ConfigError::OrthoConfig(Arc::new(ortho_error));
    assert_eq!(
        error.to_string(),
        "configuration loading failed: Validation failed for 'github.app_id': must be a positive integer"
    );
}

#[rstest]
fn container_error_permission_denied_displays_correctly(socket_path: PathBuf) {
    let error = ContainerError::PermissionDenied { path: socket_path };
    let msg = error.to_string();
    assert!(
        msg.starts_with("permission denied accessing container socket: /run/podman/podman.sock"),
        "error message should start with socket path, got: {msg}"
    );
    assert!(
        msg.contains("Hint:"),
        "error message should contain remediation hint, got: {msg}"
    );
}

#[rstest]
fn container_error_start_failed_includes_container_id(container_id: String) {
    let error = ContainerError::StartFailed {
        container_id,
        message: String::from("image not found"),
    };
    assert_eq!(
        error.to_string(),
        "failed to start container 'abc123': image not found"
    );
}

#[rstest]
#[case::health_check_failed(
    ContainerError::HealthCheckFailed { message: String::from("ping failed") },
    "container engine health check failed: ping failed"
)]
#[case::health_check_timeout(
    ContainerError::HealthCheckTimeout { seconds: 10 },
    "container engine health check timed out after 10 seconds"
)]
#[case::runtime_creation_failed(
    ContainerError::RuntimeCreationFailed { message: String::from("cannot create reactor") },
    "failed to create async runtime for health check: cannot create reactor"
)]
fn container_error_health_check_displays_correctly(
    #[case] error: ContainerError,
    #[case] expected: &str,
) {
    assert_eq!(error.to_string(), expected);
}

#[rstest]
fn github_error_token_expired_displays_correctly() {
    let error = GitHubError::TokenExpired;
    assert_eq!(error.to_string(), "installation token expired");
}

#[rstest]
fn github_error_auth_failed_displays_message() {
    let error = GitHubError::AuthenticationFailed {
        message: String::from("invalid signature"),
    };
    assert_eq!(
        error.to_string(),
        "GitHub App authentication failed: invalid signature"
    );
}

#[rstest]
fn filesystem_error_io_error_displays_message(config_path: PathBuf) {
    let error = FilesystemError::IoError {
        path: config_path,
        message: String::from("disk full"),
    };
    assert_eq!(
        error.to_string(),
        "I/O error at '/etc/podbot/config.toml': disk full"
    );
}

#[rstest]
fn podbot_error_wraps_config_error() {
    let config_error = ConfigError::MissingRequired {
        field: String::from("github.app_id"),
    };
    let podbot_error: PodbotError = config_error.into();
    assert_eq!(
        podbot_error.to_string(),
        "missing required configuration: github.app_id"
    );
}

#[rstest]
fn podbot_error_wraps_container_error(container_id: String) {
    let container_error = ContainerError::ExecFailed {
        container_id,
        message: String::from("command not found"),
    };
    let podbot_error: PodbotError = container_error.into();
    assert_eq!(
        podbot_error.to_string(),
        "failed to execute command in container 'abc123': command not found"
    );
}

#[rstest]
fn podbot_error_wraps_github_error() {
    let github_error = GitHubError::TokenRefreshFailed {
        message: String::from("rate limited"),
    };
    let podbot_error: PodbotError = github_error.into();
    assert_eq!(
        podbot_error.to_string(),
        "failed to refresh installation token: rate limited"
    );
}

#[rstest]
fn podbot_error_wraps_filesystem_error(config_path: PathBuf) {
    let fs_error = FilesystemError::NotFound { path: config_path };
    let podbot_error: PodbotError = fs_error.into();
    assert_eq!(
        podbot_error.to_string(),
        "path not found: /etc/podbot/config.toml"
    );
}

#[rstest]
#[case(
    PodbotError::from(ConfigError::MissingRequired {
        field: String::from("github.app_id"),
    }),
    "missing required configuration: github.app_id"
)]
#[case(
    PodbotError::from(ContainerError::StartFailed {
        container_id: String::from("abc123"),
        message: String::from("image missing"),
    }),
    "failed to start container 'abc123': image missing"
)]
#[case(
    PodbotError::from(GitHubError::TokenExpired),
    "installation token expired"
)]
fn eyre_report_preserves_error_messages(#[case] error: PodbotError, #[case] expected: &str) {
    let report = Report::from(error);
    assert_eq!(report.to_string(), expected);
}

/// Verify that `PodbotError` satisfies the trait bounds required for use
/// in async contexts and across thread boundaries.
#[rstest]
fn podbot_error_implements_std_error_send_sync() {
    fn assert_bounds<T: std::error::Error + Send + Sync + 'static>() {}
    assert_bounds::<PodbotError>();
}

/// Verify that each domain error enum implements `std::error::Error`.
#[rstest]
fn domain_errors_implement_std_error() {
    fn assert_error<T: std::error::Error>() {}
    assert_error::<ConfigError>();
    assert_error::<ContainerError>();
    assert_error::<GitHubError>();
    assert_error::<FilesystemError>();
}
