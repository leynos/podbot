//! Credential upload to container filesystems using tar archives.
//!
//! This module builds tar payloads from host credential directories and uploads
//! them to a running container via `Bollard`.

use std::future::Future;
use std::io;
use std::pin::Pin;

use bollard::query_parameters::{UploadToContainerOptions, UploadToContainerOptionsBuilder};
use bollard::{Docker, body_full};
use camino::Utf8PathBuf;
use cap_std::ambient_authority;
use cap_std::fs_utf8::Dir;

mod archive;

use super::EngineConnector;
use crate::config::AppConfig;
use crate::error::{ContainerError, FilesystemError, PodbotError};
use archive::build_tar_archive;
#[cfg(test)]
use archive::normalize_archive_path;

const CONTAINER_HOME_DIR: &str = "/root";
const CLAUDE_CREDENTIAL_DIR: &str = ".claude";
const CODEX_CREDENTIAL_DIR: &str = ".codex";

/// Boxed future type returned by [`ContainerUploader`] implementors.
pub type UploadToContainerFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), bollard::errors::Error>> + Send + 'a>>;

/// Behaviour required to upload an archive payload into a container.
///
/// This abstraction keeps credential-upload logic testable without a live
/// daemon.
pub trait ContainerUploader {
    /// Upload a tar archive payload into `container_id`.
    fn upload_to_container(
        &self,
        container_id: &str,
        options: Option<UploadToContainerOptions>,
        archive_bytes: Vec<u8>,
    ) -> UploadToContainerFuture<'_>;
}

impl ContainerUploader for Docker {
    fn upload_to_container(
        &self,
        container_id: &str,
        options: Option<UploadToContainerOptions>,
        archive_bytes: Vec<u8>,
    ) -> UploadToContainerFuture<'_> {
        let container_id_owned = String::from(container_id);

        Box::pin(async move {
            Self::upload_to_container(
                self,
                &container_id_owned,
                options,
                body_full(archive_bytes.into()),
            )
            .await
        })
    }
}

/// Parameters required to upload host credentials into a container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialUploadRequest {
    container_id: String,
    host_home_dir: Utf8PathBuf,
    copy_claude: bool,
    copy_codex: bool,
}

impl CredentialUploadRequest {
    /// Create a new credential-upload request.
    #[must_use]
    pub fn new(
        container_id: impl Into<String>,
        host_home_dir: impl Into<Utf8PathBuf>,
        copy_claude: bool,
        copy_codex: bool,
    ) -> Self {
        Self {
            container_id: container_id.into(),
            host_home_dir: host_home_dir.into(),
            copy_claude,
            copy_codex,
        }
    }

    /// Build a request from resolved application configuration.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use camino::Utf8PathBuf;
    /// use podbot::config::AppConfig;
    /// use podbot::engine::CredentialUploadRequest;
    ///
    /// let config = AppConfig::default();
    /// let request = CredentialUploadRequest::from_app_config(
    ///     "container-123",
    ///     Utf8PathBuf::from("/home/alice"),
    ///     &config,
    /// );
    ///
    /// assert_eq!(request.container_id(), "container-123");
    /// ```
    #[must_use]
    pub fn from_app_config(
        container_id: impl Into<String>,
        host_home_dir: impl Into<Utf8PathBuf>,
        config: &AppConfig,
    ) -> Self {
        Self::new(
            container_id,
            host_home_dir,
            config.creds.copy_claude,
            config.creds.copy_codex,
        )
    }

    /// Return the target container identifier.
    #[must_use]
    pub fn container_id(&self) -> &str {
        &self.container_id
    }

    /// Open the configured host home directory as a capability-oriented handle.
    ///
    /// This handle can be reused across multiple upload calls to avoid repeated
    /// ambient directory opens.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` when the host home directory cannot be opened.
    pub fn open_host_home_dir(&self) -> io::Result<Dir> {
        Dir::open_ambient_dir(&self.host_home_dir, ambient_authority()).map_err(|error| {
            io::Error::other(format!(
                "failed to open host home directory '{}': {error}",
                self.host_home_dir
            ))
        })
    }
}

/// Result of a credential upload operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialUploadResult {
    expected_container_paths: Vec<String>,
}

impl CredentialUploadResult {
    /// Return expected absolute credential paths inside the container.
    ///
    /// These paths are reported in deterministic order (`.claude`, then
    /// `.codex`) and include only credential families that were both enabled and
    /// present on the host.
    #[must_use]
    pub fn expected_container_paths(&self) -> &[String] {
        &self.expected_container_paths
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CredentialUploadPlan {
    archive_bytes: Vec<u8>,
    expected_container_paths: Vec<String>,
}

#[derive(Debug, Default)]
struct SelectedSources {
    source_directory_names: Vec<&'static str>,
    expected_container_paths: Vec<String>,
}

impl EngineConnector {
    /// Upload selected host credentials into a container (async version).
    ///
    /// Missing source directories are skipped. This keeps behaviour predictable
    /// for hosts that use only one agent while leaving both credential toggles
    /// enabled by default.
    ///
    /// # Errors
    ///
    /// Returns `FilesystemError::IoError` when host-side credential selection or
    /// archive construction fails, and `ContainerError::UploadFailed` when the
    /// daemon upload fails.
    pub async fn upload_credentials_async<U: ContainerUploader>(
        uploader: &U,
        request: &CredentialUploadRequest,
    ) -> Result<CredentialUploadResult, PodbotError> {
        let host_home_dir = request.open_host_home_dir().map_err(|error| {
            map_local_upload_error(LocalUploadError {
                path: request.host_home_dir.clone(),
                error,
            })
        })?;

        Self::upload_credentials_with_host_home_dir_async(uploader, request, &host_home_dir).await
    }

    /// Upload selected host credentials into a container (async version),
    /// using a pre-opened host home directory capability.
    ///
    /// # Errors
    ///
    /// Returns `FilesystemError::IoError` when host-side credential selection or
    /// archive construction fails, and `ContainerError::UploadFailed` when the
    /// daemon upload fails.
    pub async fn upload_credentials_with_host_home_dir_async<U: ContainerUploader>(
        uploader: &U,
        request: &CredentialUploadRequest,
        host_home_dir: &Dir,
    ) -> Result<CredentialUploadResult, PodbotError> {
        let container_id = request.container_id().to_owned();
        let plan = build_upload_plan(host_home_dir, request).map_err(|error| {
            let path = if error.to_string().contains(CLAUDE_CREDENTIAL_DIR) {
                request.host_home_dir.join(CLAUDE_CREDENTIAL_DIR)
            } else if error.to_string().contains(CODEX_CREDENTIAL_DIR) {
                request.host_home_dir.join(CODEX_CREDENTIAL_DIR)
            } else {
                request.host_home_dir.clone()
            };

            map_local_upload_error(LocalUploadError { path, error })
        })?;

        let CredentialUploadPlan {
            archive_bytes,
            expected_container_paths,
        } = plan;

        if expected_container_paths.is_empty() {
            return Ok(CredentialUploadResult {
                expected_container_paths,
            });
        }

        uploader
            .upload_to_container(&container_id, Some(build_upload_options()), archive_bytes)
            .await
            .map_err(|error| {
                PodbotError::from(ContainerError::UploadFailed {
                    container_id: container_id.clone(),
                    message: error.to_string(),
                })
            })?;

        Ok(CredentialUploadResult {
            expected_container_paths,
        })
    }

    /// Upload selected host credentials into a container.
    ///
    /// This synchronous helper blocks on [`Self::upload_credentials_async`] via
    /// a caller-provided `Tokio` runtime handle.
    ///
    /// # Errors
    ///
    /// Returns `FilesystemError::IoError` when host-side credential selection or
    /// archive construction fails, and `ContainerError::UploadFailed` when the
    /// daemon upload fails.
    pub fn upload_credentials<U: ContainerUploader>(
        runtime: &tokio::runtime::Handle,
        uploader: &U,
        request: &CredentialUploadRequest,
    ) -> Result<CredentialUploadResult, PodbotError> {
        runtime.block_on(Self::upload_credentials_async(uploader, request))
    }

    /// Upload selected host credentials into a container, using a pre-opened
    /// host home directory capability.
    ///
    /// This synchronous helper blocks on
    /// [`Self::upload_credentials_with_host_home_dir_async`] via a
    /// caller-provided `Tokio` runtime handle.
    ///
    /// # Errors
    ///
    /// Returns `FilesystemError::IoError` when host-side credential selection or
    /// archive construction fails, and `ContainerError::UploadFailed` when the
    /// daemon upload fails.
    pub fn upload_credentials_with_host_home_dir<U: ContainerUploader>(
        runtime: &tokio::runtime::Handle,
        uploader: &U,
        request: &CredentialUploadRequest,
        host_home_dir: &Dir,
    ) -> Result<CredentialUploadResult, PodbotError> {
        runtime.block_on(Self::upload_credentials_with_host_home_dir_async(
            uploader,
            request,
            host_home_dir,
        ))
    }
}

#[derive(Debug)]
struct LocalUploadError {
    path: Utf8PathBuf,
    error: io::Error,
}

/// Try to include a credential source in the upload plan.
///
/// Returns `Ok(Some((dir_name, container_path)))` when the source is enabled,
/// present, and valid. Returns `Ok(None)` when disabled or missing. Returns
/// `Err` when the source exists but is invalid.
fn include_credential_source(
    host_home_dir: &Dir,
    is_enabled: bool,
    directory_name: &'static str,
) -> io::Result<Option<(&'static str, String)>> {
    if !is_enabled {
        return Ok(None);
    }

    match host_home_dir.metadata(directory_name) {
        Ok(metadata) if metadata.is_dir() => {
            let container_path = format!("{CONTAINER_HOME_DIR}/{directory_name}");
            Ok(Some((directory_name, container_path)))
        }
        Ok(_) => Err(io::Error::other(format!(
            "credential source '{directory_name}' exists but is not a directory"
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io::Error::other(format!(
            "failed to inspect credential source '{directory_name}': {error}"
        ))),
    }
}

fn build_upload_plan(
    host_home_dir: &Dir,
    request: &CredentialUploadRequest,
) -> io::Result<CredentialUploadPlan> {
    let mut selected_sources = SelectedSources::default();

    for (is_enabled, directory_name) in [
        (request.copy_claude, CLAUDE_CREDENTIAL_DIR),
        (request.copy_codex, CODEX_CREDENTIAL_DIR),
    ] {
        if let Some((dir_name, container_path)) =
            include_credential_source(host_home_dir, is_enabled, directory_name)?
        {
            selected_sources.source_directory_names.push(dir_name);
            selected_sources
                .expected_container_paths
                .push(container_path);
        }
    }

    build_plan_from_selected_sources(host_home_dir, selected_sources)
}

fn map_local_upload_error(local_error: LocalUploadError) -> PodbotError {
    let LocalUploadError { path, error } = local_error;
    PodbotError::from(FilesystemError::IoError {
        path: path.as_std_path().to_path_buf(),
        message: error.to_string(),
    })
}

fn build_plan_from_selected_sources(
    host_home_dir: &Dir,
    selected_sources: SelectedSources,
) -> io::Result<CredentialUploadPlan> {
    if selected_sources.source_directory_names.is_empty() {
        return Ok(CredentialUploadPlan {
            archive_bytes: vec![],
            expected_container_paths: vec![],
        });
    }

    let archive_bytes = build_tar_archive(host_home_dir, &selected_sources.source_directory_names)?;

    Ok(CredentialUploadPlan {
        archive_bytes,
        expected_container_paths: selected_sources.expected_container_paths,
    })
}

fn build_upload_options() -> UploadToContainerOptions {
    UploadToContainerOptionsBuilder::default()
        .path(CONTAINER_HOME_DIR)
        .build()
}

#[cfg(test)]
mod tests;
