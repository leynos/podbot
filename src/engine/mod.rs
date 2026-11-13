//! Container engine connection and management.
//!
//! This module provides the interface for connecting to Docker or Podman
//! container engines. The socket endpoint is resolved through a priority-based
//! fallback chain:
//!
//! 1. CLI argument (`--engine-socket`)
//! 2. Config file (`engine_socket` in TOML)
//! 3. `PODBOT_ENGINE_SOCKET` environment variable
//! 4. `DOCKER_HOST` environment variable
//! 5. `CONTAINER_HOST` environment variable
//! 6. `PODMAN_HOST` environment variable
//! 7. Platform default (`/var/run/docker.sock` on Unix)

mod connection;

#[cfg(feature = "experimental")]
pub(crate) use connection::block_on_runtime_handle;
#[cfg(any(feature = "internal", test))]
pub use connection::{
    BRANCH_LABEL_KEY, BindMount, ContainerCreator, ContainerExecClient, ContainerLifecycleClient,
    ContainerSecurityOptions, ContainerUploader, CreateContainerFuture, CreateContainerRequest,
    CreateExecFuture, CredentialUploadRequest, CredentialUploadResult, EngineConnector, ExecMode,
    ExecRequest, ExecResult, GitIdentityResult, HostCommandRunner, HostGitIdentity,
    InspectContainerFuture, InspectExecFuture, ListContainersFuture, MANAGED_LABEL_KEY,
    MANAGED_LABEL_VALUE, REPOSITORY_LABEL_KEY, RemoveContainerFuture, RepositoryCloneRequest,
    RepositoryCloneResult, ResizeExecFuture, SESSION_LABEL_KEY, SelinuxLabelMode, SocketResolver,
    StartContainerFuture, StartExecFuture, StopContainerFuture, StopOutcome, SystemCommandRunner,
    UploadToContainerFuture, clone_repository_into_workspace, configure_git_identity, is_managed,
    label_value, managed_label, managed_label_filter, ownership_labels, read_host_git_identity,
};
#[cfg(all(feature = "experimental", not(any(feature = "internal", test))))]
pub(crate) use connection::{
    BRANCH_LABEL_KEY, BindMount, ContainerCreator, ContainerLifecycleClient,
    ContainerSecurityOptions, ContainerUploader, CreateContainerRequest, CredentialUploadRequest,
    CredentialUploadResult, GitIdentityResult, HostCommandRunner, HostGitIdentity,
    InspectContainerFuture, ListContainersFuture, REPOSITORY_LABEL_KEY, StopOutcome,
    clone_repository_into_workspace, configure_git_identity, is_managed, label_value,
    managed_label_filter, ownership_labels, read_host_git_identity,
};
#[cfg(not(any(feature = "internal", test)))]
pub(crate) use connection::{
    ContainerExecClient, EngineConnector, ExecMode, ExecRequest, SocketResolver,
};

#[cfg(test)]
pub(crate) use connection::test_helpers;
