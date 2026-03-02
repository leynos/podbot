//! PEM format validation for GitHub App private keys.
//!
//! This module validates PEM-encoded private key files, rejecting unsupported
//! key types (ECDSA, Ed25519, public keys, certificates) and encrypted keys
//! with clear error messages.

use std::path::PathBuf;

use camino::Utf8Path;
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

/// Inspect PEM headers to reject known non-RSA-private-key types early.
///
/// Checks for public keys, certificates, ECDSA (`EC PRIVATE KEY`), and
/// OpenSSH (`OPENSSH PRIVATE KEY`) headers, producing targeted error
/// messages. PKCS#8 (`PRIVATE KEY`) is ambiguous (RSA, EC, or Ed25519)
/// so it is left for `from_rsa_pem` to validate.
pub(super) fn validate_rsa_pem(
    pem_contents: &str,
    display_path: &Utf8Path,
) -> Result<(), GitHubError> {
    let trimmed = pem_contents.trim();
    let path = PathBuf::from(display_path.as_std_path());

    if trimmed.starts_with(PUBLIC_KEY_TAG) || trimmed.starts_with(RSA_PUBLIC_KEY_TAG) {
        return Err(GitHubError::PrivateKeyLoadFailed {
            path,
            message: concat!(
                "GitHub App authentication requires an RSA private key; ",
                "the file appears to contain a public key"
            )
            .to_owned(),
        });
    }

    if trimmed.starts_with(CERTIFICATE_TAG) {
        return Err(GitHubError::PrivateKeyLoadFailed {
            path,
            message: concat!(
                "GitHub App authentication requires an RSA private key; ",
                "the file appears to contain a certificate"
            )
            .to_owned(),
        });
    }

    if trimmed.starts_with(EC_PRIVATE_KEY_TAG) {
        return Err(GitHubError::PrivateKeyLoadFailed {
            path,
            message: concat!(
                "GitHub App authentication requires an RSA private key; ",
                "the file appears to contain an ECDSA key"
            )
            .to_owned(),
        });
    }

    if trimmed.starts_with(OPENSSH_PRIVATE_KEY_TAG) {
        return Err(GitHubError::PrivateKeyLoadFailed {
            path,
            message: concat!(
                "GitHub App authentication requires an RSA private key; ",
                "the file appears to contain an OpenSSH-format key ",
                "(try converting with: ssh-keygen -p -m pem -f <keyfile>)"
            )
            .to_owned(),
        });
    }

    Ok(())
}

/// Check whether PEM content represents an encrypted private key.
///
/// Detects both PKCS#8 encrypted keys (`ENCRYPTED PRIVATE KEY` header)
/// and legacy OpenSSL-encrypted keys with a `Proc-Type: 4,ENCRYPTED`
/// header. Leading blank lines and whitespace are skipped so the check
/// works with padded input.
fn is_encrypted_pem(pem_contents: &str) -> bool {
    let mut lines = pem_contents
        .lines()
        .map(str::trim)
        .skip_while(|line| line.is_empty());

    // PKCS#8 encrypted key: explicit header on the first non-blank line.
    let first_line_encrypted = lines
        .next()
        .is_some_and(|first| first.contains(ENCRYPTED_PRIVATE_KEY_TAG));

    // Legacy OpenSSL: Proc-Type header before the blank-line separator.
    let legacy_encrypted = lines
        .take_while(|line| !line.is_empty())
        .any(|line| line.starts_with("Proc-Type:") && line.contains("4,ENCRYPTED"));

    first_line_encrypted || legacy_encrypted
}

/// Parse PEM content into an RSA `EncodingKey`.
///
/// Encrypted private keys (for example, those with a
/// `BEGIN ENCRYPTED PRIVATE KEY` header or a `Proc-Type: 4,ENCRYPTED`
/// header) are rejected with a specific error message, since
/// `EncodingKey::from_rsa_pem` does not support them.
pub(super) fn parse_rsa_pem(
    pem_contents: &str,
    display_path: &Utf8Path,
) -> Result<EncodingKey, GitHubError> {
    if is_encrypted_pem(pem_contents) {
        return Err(GitHubError::PrivateKeyLoadFailed {
            path: PathBuf::from(display_path.as_std_path()),
            message: concat!(
                "encrypted private keys are not supported; ",
                "provide an unencrypted RSA private key"
            )
            .to_owned(),
        });
    }

    validate_rsa_pem(pem_contents, display_path)?;

    EncodingKey::from_rsa_pem(pem_contents.as_bytes()).map_err(|error| {
        GitHubError::PrivateKeyLoadFailed {
            path: PathBuf::from(display_path.as_std_path()),
            message: format!("invalid RSA private key: {error}"),
        }
    })
}
