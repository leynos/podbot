//! Error classification helpers for container engine connection failures.
//!
//! This module converts low-level `Bollard` errors into semantic
//! `ContainerError` variants so callers receive actionable diagnostics.

use std::path::Path;

use crate::error::ContainerError;

/// Extract the filesystem path from a socket URI.
///
/// Strips the scheme prefix (`unix://`, `npipe://`) to get the raw path.
/// For HTTP endpoints or bare paths, returns `None` as they either do not have
/// filesystem paths or lack the scheme prefix needed for reliable extraction.
pub(super) fn extract_socket_path(socket_uri: &str) -> Option<&Path> {
    socket_uri
        .strip_prefix("unix://")
        .or_else(|| socket_uri.strip_prefix("npipe://"))
        .map(Path::new)
}

/// Classify an I/O error kind into a semantic `ContainerError`.
///
/// Maps specific `ErrorKind` variants to their corresponding `ContainerError`
/// variants when a socket path is available, falling back to `ConnectionFailed`
/// for other error kinds or when no path can be extracted.
fn classify_io_error_kind(
    kind: std::io::ErrorKind,
    socket_path: Option<&Path>,
    error_msg: &str,
) -> ContainerError {
    match kind {
        std::io::ErrorKind::PermissionDenied => socket_path.map_or_else(
            || ContainerError::ConnectionFailed {
                message: error_msg.to_owned(),
            },
            |path| ContainerError::PermissionDenied {
                path: path.to_path_buf(),
            },
        ),
        std::io::ErrorKind::NotFound => socket_path.map_or_else(
            || ContainerError::ConnectionFailed {
                message: error_msg.to_owned(),
            },
            |path| ContainerError::SocketNotFound {
                path: path.to_path_buf(),
            },
        ),
        _ => ContainerError::ConnectionFailed {
            message: error_msg.to_owned(),
        },
    }
}

/// Classify a `Bollard` connection error into a semantic `ContainerError`.
///
/// Inspects the error type and underlying cause to determine the most
/// specific error variant. Falls back to `ConnectionFailed` for errors
/// that do not match known patterns or for endpoints without filesystem paths.
pub(super) fn classify_connection_error(
    bollard_error: &bollard::errors::Error,
    socket_uri: &str,
) -> ContainerError {
    let socket_path = extract_socket_path(socket_uri);
    let error_msg = bollard_error.to_string();

    match bollard_error {
        bollard::errors::Error::SocketNotFoundError(_) => {
            if let Some(path) = socket_path {
                return ContainerError::SocketNotFound {
                    path: path.to_path_buf(),
                };
            }
        }
        bollard::errors::Error::IOError { err } => {
            let direct_kind = err.kind();
            if let Some(chained_kind) = io_error_kind_in_chain(err) {
                return classify_io_error_kind(chained_kind, socket_path, &error_msg);
            }
            return classify_io_error_kind(direct_kind, socket_path, &error_msg);
        }
        _ => {}
    }

    if let Some(kind) = io_error_kind_in_chain(bollard_error) {
        return classify_io_error_kind(kind, socket_path, &error_msg);
    }

    ContainerError::ConnectionFailed { message: error_msg }
}

/// Walk the error source chain looking for an `io::Error` kind.
fn io_error_kind_in_chain(error: &dyn std::error::Error) -> Option<std::io::ErrorKind> {
    let mut current: Option<&(dyn std::error::Error + 'static)> = error.source();
    while let Some(err) = current {
        if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
            return Some(io_err.kind());
        }
        current = err.source();
    }
    None
}

#[cfg(test)]
mod tests {
    use std::fmt;

    use super::io_error_kind_in_chain;

    #[derive(Debug)]
    struct ChainRootError {
        source: std::io::Error,
    }

    impl fmt::Display for ChainRootError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "root wrapper")
        }
    }

    impl std::error::Error for ChainRootError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.source)
        }
    }

    #[derive(Debug)]
    struct TopLevelError {
        source: ChainRootError,
    }

    impl fmt::Display for TopLevelError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "top level wrapper")
        }
    }

    impl std::error::Error for TopLevelError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.source)
        }
    }

    #[test]
    fn io_error_kind_in_chain_finds_nested_io_error_kind() {
        let error = TopLevelError {
            source: ChainRootError {
                source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
            },
        };

        assert_eq!(
            io_error_kind_in_chain(&error),
            Some(std::io::ErrorKind::PermissionDenied)
        );
    }
}
