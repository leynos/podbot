//! GitHub App authentication and token management.
//!
//! This module handles loading GitHub App credentials for JWT signing.
//! It validates that private key files contain PEM-encoded RSA keys,
//! rejecting Ed25519 and ECDSA keys at load time because GitHub App
//! authentication requires RS256.
//!
//! **Stability:** This module is internal to the library and subject to
//! change as the GitHub integration stabilises.

use std::path::PathBuf;

use camino::Utf8Path;
use cap_std::ambient_authority;
use cap_std::fs_utf8::Dir;
use jsonwebtoken::EncodingKey;

use crate::error::GitHubError;

/// PEM tag for ECDSA private keys (SEC 1 / RFC 5915 format).
const EC_PRIVATE_KEY_TAG: &str = "-----BEGIN EC PRIVATE KEY-----";

/// PEM tag for OpenSSH private keys (Ed25519, ECDSA, RSA in OpenSSH format).
const OPENSSH_PRIVATE_KEY_TAG: &str = "-----BEGIN OPENSSH PRIVATE KEY-----";

/// Load a GitHub App RSA private key from the configured path.
///
/// Opens the parent directory of `key_path` using ambient authority,
/// reads the file contents, and parses them as a PEM-encoded RSA
/// private key suitable for JWT signing with RS256.
///
/// # Key format
///
/// The file must contain a PEM-encoded RSA private key in either PKCS#1
/// (`RSA PRIVATE KEY`) or PKCS#8 (`PRIVATE KEY`) format. Ed25519 and
/// ECDSA keys are rejected because GitHub App authentication requires
/// RS256.
///
/// # Errors
///
/// Returns [`GitHubError::PrivateKeyLoadFailed`] if:
/// - The parent directory cannot be opened.
/// - The file cannot be read.
/// - The file is empty.
/// - The file contains an ECDSA or Ed25519 key.
/// - The content is not a valid PEM-encoded RSA private key.
pub fn load_private_key(key_path: &Utf8Path) -> Result<EncodingKey, GitHubError> {
    let (dir, file_name) = open_key_directory(key_path)?;
    load_private_key_from_dir(&dir, file_name, key_path)
}

/// Load a private key from an already-opened directory capability.
///
/// Separated from [`load_private_key`] for testability: tests provide a
/// `cap_std::fs_utf8::Dir` backed by a temporary directory.
fn load_private_key_from_dir(
    dir: &Dir,
    file_name: &str,
    display_path: &Utf8Path,
) -> Result<EncodingKey, GitHubError> {
    let pem_contents = read_key_file(dir, file_name, display_path)?;
    parse_rsa_pem(&pem_contents, display_path)
}

/// Open the parent directory of the key path as a capability handle.
fn open_key_directory(key_path: &Utf8Path) -> Result<(Dir, &str), GitHubError> {
    let parent = key_path.parent().unwrap_or_else(|| Utf8Path::new("."));
    let file_name = key_path
        .file_name()
        .ok_or_else(|| make_error(key_path, "path does not contain a filename"))?;

    let dir = Dir::open_ambient_dir(parent, ambient_authority()).map_err(|error| {
        make_error(
            key_path,
            &format!("failed to open parent directory: {error}"),
        )
    })?;

    Ok((dir, file_name))
}

/// Read the key file contents and validate non-empty.
fn read_key_file(
    dir: &Dir,
    file_name: &str,
    display_path: &Utf8Path,
) -> Result<String, GitHubError> {
    let contents = dir
        .read_to_string(file_name)
        .map_err(|error| make_error(display_path, &format!("failed to read file: {error}")))?;

    if contents.trim().is_empty() {
        return Err(make_error(display_path, "file is empty"));
    }

    Ok(contents)
}

/// Inspect PEM headers to reject known non-RSA key types early.
///
/// Checks for ECDSA (`EC PRIVATE KEY`) and OpenSSH (`OPENSSH PRIVATE KEY`)
/// headers, producing targeted error messages. PKCS#8 (`PRIVATE KEY`) is
/// ambiguous (RSA, EC, or Ed25519) so it is left for `from_rsa_pem` to
/// validate.
fn validate_rsa_pem(pem_contents: &str, display_path: &Utf8Path) -> Result<(), GitHubError> {
    let trimmed = pem_contents.trim();

    if trimmed.starts_with(EC_PRIVATE_KEY_TAG) {
        return Err(make_error(
            display_path,
            concat!(
                "GitHub App authentication requires an RSA private key; ",
                "the file appears to contain an ECDSA key"
            ),
        ));
    }

    if trimmed.starts_with(OPENSSH_PRIVATE_KEY_TAG) {
        return Err(make_error(
            display_path,
            concat!(
                "GitHub App authentication requires an RSA private key; ",
                "the file appears to contain an OpenSSH-format key ",
                "(try converting with: ssh-keygen -p -m pem -f <keyfile>)"
            ),
        ));
    }

    Ok(())
}

/// Parse PEM content into an RSA `EncodingKey`.
fn parse_rsa_pem(pem_contents: &str, display_path: &Utf8Path) -> Result<EncodingKey, GitHubError> {
    validate_rsa_pem(pem_contents, display_path)?;

    EncodingKey::from_rsa_pem(pem_contents.as_bytes())
        .map_err(|error| make_error(display_path, &format!("invalid RSA private key: {error}")))
}

/// Construct a `GitHubError::PrivateKeyLoadFailed` from a path and message.
fn make_error(display_path: &Utf8Path, message: &str) -> GitHubError {
    GitHubError::PrivateKeyLoadFailed {
        path: PathBuf::from(display_path.as_std_path()),
        message: String::from(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cap_std::fs_utf8::Dir as Utf8Dir;
    use rstest::{fixture, rstest};
    use tempfile::TempDir;

    /// Fixture providing valid RSA PEM content (PKCS#1 format).
    #[fixture]
    fn valid_rsa_pem() -> String {
        include_str!("../tests/fixtures/test_rsa_private_key.pem").to_owned()
    }

    /// Fixture providing an ECDSA PEM key (SEC 1 format).
    #[fixture]
    fn ec_pem() -> String {
        include_str!("../tests/fixtures/test_ec_private_key.pem").to_owned()
    }

    /// Fixture providing an Ed25519 PEM key (PKCS#8 format).
    #[fixture]
    fn ed25519_pem() -> String {
        include_str!("../tests/fixtures/test_ed25519_private_key.pem").to_owned()
    }

    /// Fixture providing a temporary directory opened as a `Dir` capability.
    #[fixture]

    fn temp_key_dir() -> (TempDir, Utf8Dir) {
        let temp_dir = tempfile::tempdir().expect("should create temp dir");
        let path_str = temp_dir
            .path()
            .to_str()
            .expect("temp dir path should be UTF-8");
        let dir =
            Utf8Dir::open_ambient_dir(path_str, ambient_authority()).expect("should open temp dir");
        (temp_dir, dir)
    }

    #[rstest]

    fn load_valid_rsa_key_succeeds(valid_rsa_pem: String, temp_key_dir: (TempDir, Utf8Dir)) {
        let (_tmp, dir) = temp_key_dir;
        dir.write("key.pem", &valid_rsa_pem)
            .expect("should write key");
        let path = Utf8Path::new("/display/key.pem");
        let result = load_private_key_from_dir(&dir, "key.pem", path);
        assert!(result.is_ok(), "expected Ok, got: {result:?}");
    }

    #[rstest]
    fn load_missing_file_returns_error(temp_key_dir: (TempDir, Utf8Dir)) {
        let (_tmp, dir) = temp_key_dir;
        let path = Utf8Path::new("/config/missing.pem");
        let result = load_private_key_from_dir(&dir, "missing.pem", path);
        assert!(result.is_err(), "expected Err for missing file");
        let error = result.as_ref().err();
        let message = format!("{error:?}");
        assert!(
            message.contains("failed to read file"),
            "error should mention file read failure: {message}"
        );
    }

    #[rstest]

    fn load_empty_file_returns_error(temp_key_dir: (TempDir, Utf8Dir)) {
        let (_tmp, dir) = temp_key_dir;
        dir.write("empty.pem", "").expect("should write empty file");
        let path = Utf8Path::new("/config/empty.pem");
        let result = load_private_key_from_dir(&dir, "empty.pem", path);
        assert!(result.is_err(), "expected Err for empty file");
        let message = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            message.contains("empty"),
            "error should mention empty file: {message}"
        );
    }

    #[rstest]

    fn load_invalid_pem_returns_error(temp_key_dir: (TempDir, Utf8Dir)) {
        let (_tmp, dir) = temp_key_dir;
        dir.write("garbage.pem", "this is not a PEM file at all")
            .expect("should write garbage file");
        let path = Utf8Path::new("/config/garbage.pem");
        let result = load_private_key_from_dir(&dir, "garbage.pem", path);
        assert!(result.is_err(), "expected Err for invalid PEM");
        let message = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            message.contains("invalid RSA private key"),
            "error should mention invalid RSA key: {message}"
        );
    }

    #[rstest]

    fn load_ec_key_returns_clear_error(ec_pem: String, temp_key_dir: (TempDir, Utf8Dir)) {
        let (_tmp, dir) = temp_key_dir;
        dir.write("ec.pem", &ec_pem).expect("should write EC key");
        let path = Utf8Path::new("/config/ec.pem");
        let result = load_private_key_from_dir(&dir, "ec.pem", path);
        assert!(result.is_err(), "expected Err for ECDSA key");
        let message = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            message.contains("ECDSA"),
            "error should mention ECDSA: {message}"
        );
        assert!(
            message.contains("RSA"),
            "error should mention RSA requirement: {message}"
        );
    }

    #[rstest]

    fn load_ed25519_key_returns_clear_error(ed25519_pem: String, temp_key_dir: (TempDir, Utf8Dir)) {
        let (_tmp, dir) = temp_key_dir;
        dir.write("ed25519.pem", &ed25519_pem)
            .expect("should write Ed25519 key");
        let path = Utf8Path::new("/config/ed25519.pem");
        let result = load_private_key_from_dir(&dir, "ed25519.pem", path);
        assert!(result.is_err(), "expected Err for Ed25519 key");
        let message = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            message.contains("invalid RSA private key"),
            "Ed25519 PKCS#8 should fail RSA parse: {message}"
        );
    }

    #[rstest]
    fn error_includes_file_path(temp_key_dir: (TempDir, Utf8Dir)) {
        let (_tmp, dir) = temp_key_dir;
        let display = Utf8Path::new("/home/user/.config/podbot/app.pem");
        let result = load_private_key_from_dir(&dir, "nonexistent.pem", display);
        match result {
            Err(GitHubError::PrivateKeyLoadFailed { ref path, .. }) => {
                assert_eq!(
                    path.to_str(),
                    Some("/home/user/.config/podbot/app.pem"),
                    "error path should match display path"
                );
            }
            other => panic!("expected PrivateKeyLoadFailed, got: {other:?}"),
        }
    }

    #[rstest]

    fn load_private_key_resolves_full_path(
        valid_rsa_pem: String,
        temp_key_dir: (TempDir, Utf8Dir),
    ) {
        let (tmp, dir) = temp_key_dir;
        dir.write("github-app.pem", &valid_rsa_pem)
            .expect("should write key");
        let full_path = tmp.path().join("github-app.pem");
        let utf8_path = Utf8Path::from_path(&full_path).expect("temp path should be UTF-8");
        let result = load_private_key(utf8_path);
        assert!(result.is_ok(), "expected Ok, got: {result:?}");
    }

    #[rstest]
    fn load_private_key_missing_parent_returns_error() {
        let path = Utf8Path::new("/nonexistent/directory/key.pem");
        let result = load_private_key(path);
        assert!(result.is_err(), "expected Err for missing parent");
        let message = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            message.contains("failed to open parent directory"),
            "error should mention parent directory: {message}"
        );
    }
}
