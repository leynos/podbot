//! GitHub App authentication and token management.
//!
//! This module handles loading GitHub App credentials for JWT signing.
//! It validates that private key files contain PEM-encoded RSA keys,
//! rejecting Ed25519 and ECDSA keys at load time because GitHub App
//! authentication requires RS256.
//!
//! **Stability:** This module is internal to the library and subject to
//! change as the GitHub integration stabilizes.

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

/// PEM tag for generic public keys (PKCS#8 / X.509 `SubjectPublicKeyInfo`).
const PUBLIC_KEY_TAG: &str = "-----BEGIN PUBLIC KEY-----";

/// PEM tag for RSA-specific public keys (PKCS#1 `RSAPublicKey`).
const RSA_PUBLIC_KEY_TAG: &str = "-----BEGIN RSA PUBLIC KEY-----";

/// PEM tag for X.509 certificates.
const CERTIFICATE_TAG: &str = "-----BEGIN CERTIFICATE-----";

/// PEM tag for PKCS#8 encrypted private keys.
const ENCRYPTED_PRIVATE_KEY_TAG: &str = "-----BEGIN ENCRYPTED PRIVATE KEY-----";

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
/// - The file contains a public key or certificate.
/// - The file contains an encrypted private key.
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
    let parent = key_path
        .parent()
        .filter(|p| !p.as_str().is_empty())
        .unwrap_or_else(|| Utf8Path::new("."));
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

/// Inspect PEM headers to reject known non-RSA-private-key types early.
///
/// Checks for public keys, certificates, ECDSA (`EC PRIVATE KEY`), and
/// OpenSSH (`OPENSSH PRIVATE KEY`) headers, producing targeted error
/// messages. PKCS#8 (`PRIVATE KEY`) is ambiguous (RSA, EC, or Ed25519)
/// so it is left for `from_rsa_pem` to validate.
fn validate_rsa_pem(pem_contents: &str, display_path: &Utf8Path) -> Result<(), GitHubError> {
    let trimmed = pem_contents.trim();

    if trimmed.starts_with(PUBLIC_KEY_TAG) || trimmed.starts_with(RSA_PUBLIC_KEY_TAG) {
        return Err(make_error(
            display_path,
            concat!(
                "GitHub App authentication requires an RSA private key; ",
                "the file appears to contain a public key"
            ),
        ));
    }

    if trimmed.starts_with(CERTIFICATE_TAG) {
        return Err(make_error(
            display_path,
            concat!(
                "GitHub App authentication requires an RSA private key; ",
                "the file appears to contain a certificate"
            ),
        ));
    }

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

/// Check whether PEM content represents an encrypted private key.
///
/// Detects both PKCS#8 encrypted keys (`ENCRYPTED PRIVATE KEY` header)
/// and legacy OpenSSL-encrypted keys with a `Proc-Type: 4,ENCRYPTED`
/// header.
fn is_encrypted_pem(pem_contents: &str) -> bool {
    let mut lines = pem_contents.lines();

    // PKCS#8 encrypted key with an explicit header.
    if let Some(first) = lines.next() {
        if first.contains(ENCRYPTED_PRIVATE_KEY_TAG) {
            return true;
        }
    }

    // Legacy OpenSSL encryption header before the blank-line separator.
    for line in lines {
        if line.is_empty() {
            break;
        }

        if line.starts_with("Proc-Type:") && line.contains("4,ENCRYPTED") {
            return true;
        }
    }

    false
}

/// Parse PEM content into an RSA `EncodingKey`.
///
/// Encrypted private keys (for example, those with a
/// `BEGIN ENCRYPTED PRIVATE KEY` header or a `Proc-Type: 4,ENCRYPTED`
/// header) are rejected with a specific error message, since
/// `EncodingKey::from_rsa_pem` does not support them.
fn parse_rsa_pem(pem_contents: &str, display_path: &Utf8Path) -> Result<EncodingKey, GitHubError> {
    if is_encrypted_pem(pem_contents) {
        return Err(make_error(
            display_path,
            concat!(
                "encrypted private keys are not supported; ",
                "provide an unencrypted RSA private key"
            ),
        ));
    }

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
mod tests;
