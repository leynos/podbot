//! GitHub App authentication and token management.
//!
//! This module handles loading GitHub App credentials for JWT signing,
//! constructing an authenticated Octocrab client for App operations,
//! and validating credentials against the GitHub API. It validates that
//! private key files contain PEM-encoded RSA keys, rejecting Ed25519 and
//! ECDSA keys at load time because GitHub App authentication requires
//! RS256.
//!
//! **Stability:** This module is internal to the library and subject to
//! change as the GitHub integration stabilizes.

mod pem_validation;

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use camino::Utf8Path;
use cap_std::ambient_authority;
use cap_std::fs_utf8::Dir;
use jsonwebtoken::EncodingKey;

use octocrab::Octocrab;
use octocrab::models::AppId;

use crate::error::GitHubError;
use pem_validation::parse_rsa_pem;

/// A boxed future for async trait methods.
///
/// This type alias enables `mockall::automock` compatibility and trait object
/// usage for async methods in [`GitHubAppClient`].
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

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

/// Build an authenticated Octocrab client for GitHub App operations.
///
/// Configures `OctocrabBuilder` with the given App ID and RSA private
/// key, producing a client ready for JWT signing and installation token
/// acquisition.
///
/// The client is constructed synchronously and does not make network
/// calls. Credential validation against GitHub occurs later, when the
/// client is used to acquire an installation token (Step 3.2).
///
/// # Tokio runtime
///
/// A Tokio runtime context must be active when this function is called
/// because Octocrab's builder spawns a Tower `Buffer` background task.
/// If no runtime is available the function returns an error instead of
/// panicking.
///
/// # Errors
///
/// Returns [`GitHubError::AuthenticationFailed`] if:
/// - No Tokio runtime context is active.
/// - The Octocrab builder fails to construct the HTTP client (for
///   example, due to TLS initialization failure).
pub fn build_app_client(app_id: u64, private_key: EncodingKey) -> Result<Octocrab, GitHubError> {
    // Guard: Octocrab's build() internally spawns a Tower Buffer task
    // via tokio::spawn. Without an active runtime the call panics.
    // Check up front and return a descriptive error instead.
    let _handle =
        tokio::runtime::Handle::try_current().map_err(|_| GitHubError::AuthenticationFailed {
            message: String::from(
                "failed to build GitHub App client: \
                 no Tokio runtime context is active \
                 (Octocrab requires one for its Tower buffer task)",
            ),
        })?;

    Octocrab::builder()
        .app(AppId(app_id), private_key)
        .build()
        .map_err(|error| GitHubError::AuthenticationFailed {
            message: format!("failed to build GitHub App client: {error}"),
        })
}

/// Trait for GitHub App client operations.
///
/// This trait abstracts the Octocrab client to enable testing without
/// network calls. Production code uses [`OctocrabAppClient`], while tests
/// inject mock implementations via `mockall`.
#[cfg_attr(test, mockall::automock)]
pub trait GitHubAppClient: Send + Sync {
    /// Validates that the App credentials are accepted by GitHub.
    ///
    /// Calls `GET /app` and verifies the response indicates a valid
    /// authenticated App.
    ///
    /// # Errors
    ///
    /// Returns an error if the API call fails or returns an error response.
    fn validate_credentials(&self) -> BoxFuture<'_, Result<(), GitHubError>>;
}

/// Production implementation of [`GitHubAppClient`] using Octocrab.
pub struct OctocrabAppClient {
    client: Octocrab,
}

impl OctocrabAppClient {
    /// Creates a new `OctocrabAppClient` from an authenticated Octocrab
    /// instance.
    #[must_use]
    pub const fn new(client: Octocrab) -> Self {
        Self { client }
    }
}

impl GitHubAppClient for OctocrabAppClient {
    fn validate_credentials(&self) -> BoxFuture<'_, Result<(), GitHubError>> {
        Box::pin(async move {
            self.client
                .get::<(), _, ()>("/app", None)
                .await
                .map_err(|error| GitHubError::AuthenticationFailed {
                    message: format!("failed to validate GitHub App credentials: {error}"),
                })?;
            Ok(())
        })
    }
}

/// Validates GitHub App credentials by loading the private key, building
/// the App client, and verifying credentials are accepted by GitHub.
///
/// This function performs a network call to GitHub's `/app` endpoint to
/// verify that the configured `app_id` and private key produce a valid JWT
/// that GitHub accepts.
///
/// # Arguments
///
/// * `app_id` - The GitHub App ID
/// * `private_key_path` - Path to the PEM-encoded RSA private key
///
/// # Errors
///
/// Returns [`GitHubError::PrivateKeyLoadFailed`] if the key cannot be loaded.
/// Returns [`GitHubError::AuthenticationFailed`] if the client cannot be
/// built or if GitHub rejects the credentials.
///
/// # Example
///
/// ```rust,no_run
/// use podbot::github::validate_app_credentials;
/// use camino::Utf8Path;
///
/// # async fn example() -> Result<(), podbot::error::GitHubError> {
/// let app_id = 12345;
/// let key_path = Utf8Path::new("/path/to/private-key.pem");
/// validate_app_credentials(app_id, key_path).await?;
/// println!("Credentials are valid!");
/// # Ok(())
/// # }
/// ```
pub async fn validate_app_credentials(
    app_id: u64,
    private_key_path: &Utf8Path,
) -> Result<(), GitHubError> {
    let private_key = load_private_key(private_key_path)?;
    let octocrab = build_app_client(app_id, private_key)?;
    let client = OctocrabAppClient::new(octocrab);
    validate_with_client(&client).await
}

/// Validates credentials using the provided client.
///
/// This is a testable helper that separates orchestration from client
/// construction. Tests can inject a mock [`GitHubAppClient`] to verify
/// behaviour without network calls.
///
/// # Errors
///
/// Returns [`GitHubError::AuthenticationFailed`] if the client rejects the
/// credentials or the API call fails.
pub async fn validate_with_client(client: &dyn GitHubAppClient) -> Result<(), GitHubError> {
    client.validate_credentials().await
}

/// Validates credentials with an injected client factory.
///
/// This function exercises the full orchestration path (key loading, client
/// building, validation) while allowing tests to inject a mock client. The
/// factory receives the App ID and encoding key, returning a client for
/// validation.
///
/// # Errors
///
/// Returns [`GitHubError::PrivateKeyLoadFailed`] if the key cannot be loaded.
/// Returns [`GitHubError::AuthenticationFailed`] if the client cannot be
/// built or if GitHub rejects the credentials.
pub async fn validate_with_factory<F, C>(
    app_id: u64,
    private_key_path: &Utf8Path,
    factory: F,
) -> Result<(), GitHubError>
where
    F: FnOnce(u64, EncodingKey) -> Result<C, GitHubError>,
    C: GitHubAppClient,
{
    let private_key = load_private_key(private_key_path)?;
    let client = factory(app_id, private_key)?;
    client.validate_credentials().await
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

#[cfg(test)]
mod tests;
