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

pub use connection::{
    ContainerCreator, ContainerExecClient, ContainerSecurityOptions, ContainerUploader,
    CreateContainerFuture, CreateContainerRequest, CreateExecFuture, CredentialUploadRequest,
    CredentialUploadResult, EngineConnector, ExecMode, ExecRequest, ExecResult, GitIdentityResult, HostCommandRunner, HostGitIdentity, ExecSessionOptions,
    InspectExecFuture, ResizeExecFuture, SelinuxLabelMode,
    SocketResolver, StartExecFuture, SystemCommandRunner,
    UploadToContainerFuture,
    configure_git_identity, read_host_git_identity,
};

#[cfg(test)]
pub(crate) use connection::test_helpers;
