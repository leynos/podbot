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
use crate::error::{ContainerError, PodbotError};
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
    /// Returns `ContainerError::UploadFailed` when archive construction or
    /// daemon upload fails.
    pub async fn upload_credentials_async<U: ContainerUploader>(
        uploader: &U,
        request: &CredentialUploadRequest,
    ) -> Result<CredentialUploadResult, PodbotError> {
        let container_id = request.container_id().to_owned();
        let plan = build_upload_plan(request).map_err(|error| {
            PodbotError::from(ContainerError::UploadFailed {
                container_id: container_id.clone(),
                message: error.to_string(),
            })
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
    /// a caller-provided Tokio runtime handle.
    ///
    /// # Errors
    ///
    /// Returns `ContainerError::UploadFailed` when archive construction or
    /// daemon upload fails.
    pub fn upload_credentials<U: ContainerUploader>(
        runtime: &tokio::runtime::Handle,
        uploader: &U,
        request: &CredentialUploadRequest,
    ) -> Result<CredentialUploadResult, PodbotError> {
        runtime.block_on(Self::upload_credentials_async(uploader, request))
    }
}

fn build_upload_plan(request: &CredentialUploadRequest) -> io::Result<CredentialUploadPlan> {
    let host_home_dir = Dir::open_ambient_dir(&request.host_home_dir, ambient_authority())
        .map_err(|error| {
            io::Error::other(format!(
                "failed to open host home directory '{}': {error}",
                request.host_home_dir
            ))
        })?;

    let mut selected_sources = SelectedSources::default();

    include_selected_source(
        &host_home_dir,
        request.copy_claude,
        CLAUDE_CREDENTIAL_DIR,
        &mut selected_sources,
    )?;
    include_selected_source(
        &host_home_dir,
        request.copy_codex,
        CODEX_CREDENTIAL_DIR,
        &mut selected_sources,
    )?;

    if selected_sources.source_directory_names.is_empty() {
        return Ok(CredentialUploadPlan {
            archive_bytes: vec![],
            expected_container_paths: vec![],
        });
    }

    let archive_bytes =
        build_tar_archive(&host_home_dir, &selected_sources.source_directory_names)?;

    Ok(CredentialUploadPlan {
        archive_bytes,
        expected_container_paths: selected_sources.expected_container_paths,
    })
}

fn include_selected_source(
    host_home_dir: &Dir,
    is_enabled: bool,
    directory_name: &'static str,
    selected_sources: &mut SelectedSources,
) -> io::Result<()> {
    if !is_enabled {
        return Ok(());
    }

    match host_home_dir.metadata(directory_name) {
        Ok(metadata) if metadata.is_dir() => {
            selected_sources.source_directory_names.push(directory_name);
            selected_sources
                .expected_container_paths
                .push(format!("{CONTAINER_HOME_DIR}/{directory_name}"));
            Ok(())
        }
        Ok(_) => Err(io::Error::other(format!(
            "credential source '{directory_name}' exists but is not a directory"
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io::Error::other(format!(
            "failed to inspect credential source '{directory_name}': {error}"
        ))),
    }
}

fn build_upload_options() -> UploadToContainerOptions {
    UploadToContainerOptionsBuilder::default()
        .path(CONTAINER_HOME_DIR)
        .build()
}

#[cfg(test)]
mod tests;
