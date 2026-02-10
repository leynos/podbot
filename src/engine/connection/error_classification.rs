//! Error classification helpers for container engine connection failures.
//!
//! This module converts low-level `Bollard` errors into semantic
//! `ContainerError` variants so callers receive actionable diagnostics.

use std::path::Path;

use crate::error::ContainerError;

/// Extract the filesystem path from a socket URI.
///
/// Strips the scheme prefix (`unix://`, `npipe://`) to get the raw path.
/// Scheme-less absolute paths are treated as socket paths. HTTP and TCP
/// endpoints return `None`.
pub(super) fn extract_socket_path(socket_uri: &str) -> Option<&Path> {
    socket_uri
        .strip_prefix("unix://")
        .map(Path::new)
        .or_else(|| {
            socket_uri
                .strip_prefix("npipe://")
                .map(Path::new)
                .or_else(|| {
                    let path = Path::new(socket_uri);
                    path.is_absolute().then_some(path)
                })
        })
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

    match *bollard_error {
        bollard::errors::Error::SocketNotFoundError(ref _missing_socket) => {
            if let Some(path) = socket_path {
                return ContainerError::SocketNotFound {
                    path: path.to_path_buf(),
                };
            }
        }
        bollard::errors::Error::IOError { ref err } => {
            // Prefer the deepest chained kind when wrappers stack kinds,
            // e.g. `Other(PermissionDenied(...))`.
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

    use rstest::rstest;

    use super::{classify_connection_error, extract_socket_path, io_error_kind_in_chain};
    use crate::error::ContainerError;

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

    #[derive(Debug)]
    struct NonIoRootError;

    impl fmt::Display for NonIoRootError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "non-io root")
        }
    }

    impl std::error::Error for NonIoRootError {}

    #[derive(Debug)]
    struct NonIoTopLevelError {
        source: NonIoRootError,
    }

    impl fmt::Display for NonIoTopLevelError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "non-io top level wrapper")
        }
    }

    impl std::error::Error for NonIoTopLevelError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.source)
        }
    }

    #[derive(Debug)]
    struct IoSourceWrapper {
        source: std::io::Error,
    }

    impl fmt::Display for IoSourceWrapper {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "io source wrapper")
        }
    }

    impl std::error::Error for IoSourceWrapper {
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

    #[test]
    fn io_error_kind_in_chain_returns_none_when_chain_has_no_io_error() {
        let error = NonIoTopLevelError {
            source: NonIoRootError,
        };

        assert_eq!(io_error_kind_in_chain(&error), None);
    }

    #[rstest]
    #[case::unix_socket("unix:///var/run/docker.sock", Some("/var/run/docker.sock"))]
    #[case::npipe("npipe:////./pipe/docker_engine", Some("//./pipe/docker_engine"))]
    #[case::http("http://localhost:2375", None)]
    #[case::tcp("tcp://localhost:2375", None)]
    #[case::bare_path("/var/run/docker.sock", Some("/var/run/docker.sock"))]
    #[case::https("https://docker.example.com:2376", None)]
    fn extract_socket_path_parses_correctly(#[case] uri: &str, #[case] expected: Option<&str>) {
        let result = extract_socket_path(uri);
        assert_eq!(
            result
                .as_ref()
                .map(|path| path.to_str().expect("valid UTF-8")),
            expected
        );
    }

    #[rstest]
    #[case::permission_denied_unix(
        std::io::ErrorKind::PermissionDenied,
        "unix:///var/run/docker.sock",
        Some("/var/run/docker.sock")
    )]
    #[case::not_found_unix(
        std::io::ErrorKind::NotFound,
        "unix:///nonexistent.sock",
        Some("/nonexistent.sock")
    )]
    fn classify_connection_error_maps_socket_kinds(
        #[case] io_error_kind: std::io::ErrorKind,
        #[case] socket_uri: &str,
        #[case] expected_path: Option<&str>,
    ) {
        let bollard_err = bollard::errors::Error::IOError {
            err: std::io::Error::new(io_error_kind, "test error"),
        };

        let result = classify_connection_error(&bollard_err, socket_uri);

        match io_error_kind {
            std::io::ErrorKind::PermissionDenied => {
                assert!(
                    matches!(
                        result,
                        ContainerError::PermissionDenied { ref path }
                            if path.to_str() == expected_path
                    ),
                    "expected PermissionDenied with path {expected_path:?}, got: {result:?}"
                );
            }
            std::io::ErrorKind::NotFound => {
                assert!(
                    matches!(
                        result,
                        ContainerError::SocketNotFound { ref path }
                            if path.to_str() == expected_path
                    ),
                    "expected SocketNotFound with path {expected_path:?}, got: {result:?}"
                );
            }
            _ => panic!("unexpected test setup for io_error_kind: {io_error_kind:?}"),
        }
    }

    #[rstest]
    #[case::connection_refused(
        std::io::ErrorKind::ConnectionRefused,
        "unix:///var/run/docker.sock"
    )]
    #[case::permission_denied_http(std::io::ErrorKind::PermissionDenied, "http://localhost:2375")]
    #[case::not_found_tcp(std::io::ErrorKind::NotFound, "tcp://remotehost:2375")]
    #[case::permission_denied_tcp(std::io::ErrorKind::PermissionDenied, "tcp://remotehost:2375")]
    fn classify_connection_error_falls_back_for_unmapped_or_non_socket_context(
        #[case] io_error_kind: std::io::ErrorKind,
        #[case] socket_uri: &str,
    ) {
        let bollard_err = bollard::errors::Error::IOError {
            err: std::io::Error::new(io_error_kind, "test error"),
        };
        let result = classify_connection_error(&bollard_err, socket_uri);

        assert!(
            matches!(result, ContainerError::ConnectionFailed { .. }),
            "expected ConnectionFailed fallback, got: {result:?}"
        );
    }

    #[test]
    fn classify_connection_error_handles_socket_not_found_variant() {
        let bollard_err = bollard::errors::Error::SocketNotFoundError(String::from("missing"));
        let result = classify_connection_error(&bollard_err, "unix:///var/run/docker.sock");

        assert!(
            matches!(
                result,
                ContainerError::SocketNotFound { ref path }
                    if path.to_str() == Some("/var/run/docker.sock")
            ),
            "expected SocketNotFound with socket path, got: {result:?}"
        );
    }

    #[test]
    fn classify_chained_io_error_via_find_io_error_in_chain() {
        let wrapped = std::io::Error::other(IoSourceWrapper {
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied"),
        });
        let serde_err = serde_json::Error::io(wrapped);
        let bollard_err = bollard::errors::Error::JsonSerdeError { err: serde_err };

        let result = classify_connection_error(&bollard_err, "unix:///var/run/docker.sock");

        assert!(
            matches!(
                result,
                ContainerError::PermissionDenied { ref path }
                    if path.to_str() == Some("/var/run/docker.sock")
            ),
            "expected PermissionDenied from chained io::Error, got: {result:?}"
        );
    }

    #[test]
    fn classify_connection_error_non_io_errors_fall_back_to_connection_failed() {
        let bollard_err = bollard::errors::Error::UnsupportedURISchemeError {
            uri: String::from("foo://example"),
        };
        let result = classify_connection_error(&bollard_err, "foo://example");

        assert!(
            matches!(result, ContainerError::ConnectionFailed { .. }),
            "expected ConnectionFailed for non-io error, got: {result:?}"
        );
    }
}
