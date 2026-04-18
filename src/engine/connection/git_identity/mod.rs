//! Git identity configuration for containers.
//!
//! This module reads Git `user.name` and `user.email` from the host
//! configuration and propagates them into a running container via
//! `git config --global` exec commands.

mod container_configurator;
mod host_reader;

pub use container_configurator::{GitIdentityResult, configure_git_identity};
pub use host_reader::{
    HostCommandRunner, HostGitIdentity, SystemCommandRunner, read_host_git_identity,
};

use crate::error::{ContainerError, PodbotError};

fn git_identity_exec_failed(container_id: &str, message: impl Into<String>) -> PodbotError {
    PodbotError::from(ContainerError::ExecFailed {
        container_id: String::from(container_id),
        message: message.into(),
    })
}

#[cfg(test)]
pub(crate) mod test_helpers;

#[cfg(test)]
mod tests;
