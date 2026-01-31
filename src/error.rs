//! Semantic error types for the podbot application.
//!
//! This module defines the error hierarchy for podbot, following the principle of
//! using semantic error enums (via `thiserror`) for conditions the caller might
//! inspect, retry, or map to an HTTP status, while reserving opaque errors
//! (`eyre::Report`) for the application boundary.

use std::path::PathBuf;
use std::sync::Arc;

use thiserror::Error;

/// Errors that can occur during configuration loading and validation.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// The configuration file was not found at the expected path.
    #[error("configuration file not found: {path}")]
    FileNotFound {
        /// The path where the configuration file was expected.
        path: PathBuf,
    },

    /// The configuration file could not be parsed.
    #[error("failed to parse configuration file: {message}")]
    ParseError {
        /// A description of the parse error.
        message: String,
    },

    /// A required configuration value is missing.
    #[error("missing required configuration: {field}")]
    MissingRequired {
        /// The name of the missing field.
        field: String,
    },

    /// A configuration value failed validation.
    #[error("invalid configuration value for '{field}': {reason}")]
    InvalidValue {
        /// The name of the invalid field.
        field: String,
        /// The reason the value is invalid.
        reason: String,
    },

    /// The `OrthoConfig` library returned an error during configuration loading.
    ///
    /// This wraps errors from the layered configuration system, including:
    /// - Configuration file parsing errors
    /// - Environment variable parsing errors
    /// - CLI argument parsing errors
    /// - Missing required fields after layer merging
    #[error("configuration loading failed: {0}")]
    OrthoConfig(Arc<ortho_config::OrthoError>),
}

/// Errors that can occur during container operations.
#[derive(Debug, Error)]
pub enum ContainerError {
    /// Failed to connect to the container engine socket.
    #[error("failed to connect to container engine: {message}")]
    ConnectionFailed {
        /// A description of the connection failure.
        message: String,
    },

    /// The container engine socket was not found.
    #[error("container engine socket not found: {path}")]
    SocketNotFound {
        /// The path where the socket was expected.
        path: PathBuf,
    },

    /// Permission denied when accessing the container engine socket.
    #[error("permission denied accessing container socket: {path}")]
    PermissionDenied {
        /// The path to the socket.
        path: PathBuf,
    },

    /// Failed to create a container.
    #[error("failed to create container: {message}")]
    CreateFailed {
        /// A description of the creation failure.
        message: String,
    },

    /// Failed to start a container.
    #[error("failed to start container '{container_id}': {message}")]
    StartFailed {
        /// The ID of the container that failed to start.
        container_id: String,
        /// A description of the start failure.
        message: String,
    },

    /// Failed to upload files to a container.
    #[error("failed to upload files to container '{container_id}': {message}")]
    UploadFailed {
        /// The ID of the target container.
        container_id: String,
        /// A description of the upload failure.
        message: String,
    },

    /// Failed to execute a command in a container.
    #[error("failed to execute command in container '{container_id}': {message}")]
    ExecFailed {
        /// The ID of the container.
        container_id: String,
        /// A description of the execution failure.
        message: String,
    },

    /// Health check failed - engine did not respond correctly.
    #[error("container engine health check failed: {message}")]
    HealthCheckFailed {
        /// A description of the health check failure.
        message: String,
    },

    /// Health check timed out.
    #[error("container engine health check timed out after {seconds} seconds")]
    HealthCheckTimeout {
        /// The timeout duration in seconds.
        seconds: u64,
    },
}

/// Errors that can occur during GitHub operations.
#[derive(Debug, Error)]
pub enum GitHubError {
    /// GitHub App authentication failed.
    #[error("GitHub App authentication failed: {message}")]
    AuthenticationFailed {
        /// A description of the authentication failure.
        message: String,
    },

    /// Failed to load the GitHub App private key.
    #[error("failed to load private key from '{path}': {message}")]
    PrivateKeyLoadFailed {
        /// The path to the private key file.
        path: PathBuf,
        /// A description of the failure.
        message: String,
    },

    /// Failed to acquire an installation token.
    #[error("failed to acquire installation token: {message}")]
    TokenAcquisitionFailed {
        /// A description of the token acquisition failure.
        message: String,
    },

    /// The installation token has expired.
    #[error("installation token expired")]
    TokenExpired,

    /// Failed to refresh the installation token.
    #[error("failed to refresh installation token: {message}")]
    TokenRefreshFailed {
        /// A description of the refresh failure.
        message: String,
    },
}

/// Errors that can occur during filesystem operations.
#[derive(Debug, Error)]
pub enum FilesystemError {
    /// A file or directory was not found.
    #[error("path not found: {path}")]
    NotFound {
        /// The path that was not found.
        path: PathBuf,
    },

    /// Permission denied when accessing a path.
    #[error("permission denied: {path}")]
    PermissionDenied {
        /// The path that could not be accessed.
        path: PathBuf,
    },

    /// An I/O error occurred.
    #[error("I/O error at '{path}': {message}")]
    IoError {
        /// The path where the error occurred.
        path: PathBuf,
        /// A description of the I/O error.
        message: String,
    },
}

/// Top-level error type for the podbot application.
///
/// This enum aggregates all domain-specific errors into a single type that can
/// be used throughout the application. At the application boundary (main.rs),
/// these errors are typically converted to `eyre::Report` for human-readable
/// error reporting.
#[derive(Debug, Error)]
pub enum PodbotError {
    /// An error occurred during configuration.
    #[error(transparent)]
    Config(#[from] ConfigError),

    /// An error occurred during container operations.
    #[error(transparent)]
    Container(#[from] ContainerError),

    /// An error occurred during GitHub operations.
    #[error(transparent)]
    GitHub(#[from] GitHubError),

    /// An error occurred during filesystem operations.
    #[error(transparent)]
    Filesystem(#[from] FilesystemError),
}

/// A specialised `Result` type for podbot operations.
pub type Result<T> = std::result::Result<T, PodbotError>;

#[cfg(test)]
mod tests {
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
        assert_eq!(
            error.to_string(),
            "permission denied accessing container socket: /run/podman/podman.sock"
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
    fn container_error_health_check_failed_displays_correctly() {
        let error = ContainerError::HealthCheckFailed {
            message: String::from("ping failed"),
        };
        assert_eq!(
            error.to_string(),
            "container engine health check failed: ping failed"
        );
    }

    #[rstest]
    fn container_error_health_check_timeout_displays_correctly() {
        let error = ContainerError::HealthCheckTimeout { seconds: 10 };
        assert_eq!(
            error.to_string(),
            "container engine health check timed out after 10 seconds"
        );
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
}
