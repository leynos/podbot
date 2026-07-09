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
    #[error(
        "container engine socket not found: {path}\n\
Hint: Verify the socket path exists and the container daemon is running.\n\
For Docker: sudo systemctl start docker\n\
For Podman: systemctl --user start podman.socket"
    )]
    SocketNotFound {
        /// The path where the socket was expected.
        path: PathBuf,
    },

    /// Permission denied when accessing the container engine socket.
    #[error(
        "permission denied accessing container socket: {path}\n\
Hint: Add your user to the docker group or use rootless Podman.\n\
For Docker: sudo usermod -aG docker $USER && newgrp docker\n\
For Podman: Use socket at /run/user/$UID/podman/podman.sock"
    )]
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

    /// Failed to create a tokio runtime for synchronous health check.
    #[error("failed to create async runtime for health check: {message}")]
    RuntimeCreationFailed {
        /// A description of the runtime creation failure.
        message: String,
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
#[path = "error_tests.rs"]
mod tests;
