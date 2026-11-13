//! GitHub App private-key loading.
//!
//! Reads a PEM-encoded RSA private key through a directory capability and
//! rejects anything GitHub App authentication cannot use. Keeping the file
//! access here leaves [`super`] focused on client construction and token
//! policy.

use std::path::PathBuf;

use camino::Utf8Path;
use cap_std::ambient_authority;
use cap_std::fs_utf8::Dir;
use jsonwebtoken::EncodingKey;

use crate::error::GitHubError;

use super::pem_validation::parse_rsa_pem;

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
pub(crate) fn load_private_key_from_dir(
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
        .ok_or_else(|| GitHubError::PrivateKeyLoadFailed {
            path: PathBuf::from(key_path.as_std_path()),
            message: "path does not contain a filename".to_owned(),
        })?;

    let dir = Dir::open_ambient_dir(parent, ambient_authority()).map_err(|error| {
        GitHubError::PrivateKeyLoadFailed {
            path: PathBuf::from(key_path.as_std_path()),
            message: format!("failed to open parent directory: {error}"),
        }
    })?;

    Ok((dir, file_name))
}

/// Read the key file contents and validate non-empty.
fn read_key_file(
    dir: &Dir,
    file_name: &str,
    display_path: &Utf8Path,
) -> Result<String, GitHubError> {
    let contents =
        dir.read_to_string(file_name)
            .map_err(|error| GitHubError::PrivateKeyLoadFailed {
                path: PathBuf::from(display_path.as_std_path()),
                message: format!("failed to read file: {error}"),
            })?;

    if contents.trim().is_empty() {
        return Err(GitHubError::PrivateKeyLoadFailed {
            path: PathBuf::from(display_path.as_std_path()),
            message: "file is empty".to_owned(),
        });
    }

    Ok(contents)
}
