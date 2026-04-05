//! Workspace configuration types for clone-based and host-mounted sessions.

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

/// The source of the workspace content exposed to the agent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceSource {
    /// Clone a repository into the sandbox-managed workspace path.
    #[default]
    GithubClone,
    /// Bind-mount an explicit host directory into the sandbox.
    HostMount,
}

/// Workspace configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct WorkspaceConfig {
    /// Where the workspace content comes from.
    pub source: WorkspaceSource,

    /// Base directory for cloned repositories inside the container.
    pub base_dir: Utf8PathBuf,

    /// Host directory to mount when `source = "host_mount"`.
    pub host_path: Option<Utf8PathBuf>,

    /// Container path where the mounted host workspace appears.
    pub container_path: Option<Utf8PathBuf>,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            source: WorkspaceSource::GithubClone,
            base_dir: Utf8PathBuf::from("/work"),
            host_path: None,
            container_path: None,
        }
    }
}

/// Default container path used for host-mounted workspaces.
#[must_use]
pub(crate) fn default_host_mount_container_path() -> Utf8PathBuf {
    Utf8PathBuf::from("/workspace")
}
