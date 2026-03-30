//! Sandboxed execution environment for AI coding agents.
//!
//! `podbot` provides a secure container-based sandbox for running AI coding agents
//! such as Claude Code and Codex. The design prioritises security by treating the
//! host container engine as high-trust infrastructure, while the agent container
//! operates in a low-trust playpen with no access to the host socket.
//!
//! # Architecture
//!
//! The core principle is straightforward: the Rust CLI acts as the sole holder of
//! the host Podman or Docker socket. The agent container never receives access to
//! this socket. Instead, the agent runs an inner Podman service for any nested
//! container operations.
//!
//! # Modules
//!
//! - [`api`]: Stable orchestration API for embedding and the CLI adapter
//! - [`config`]: Stable configuration system with layered precedence
//! - [`error`]: Stable semantic error types for the application
//! - `cli` feature: optional `Clap` parse types for the `podbot` binary
//! - Hidden support modules: internal and compatibility seams that are not part
//!   of the documented stable boundary

pub mod api;
#[cfg(feature = "cli")]
pub mod cli;
pub mod config;
#[doc(hidden)]
pub mod engine;
pub mod error;
#[doc(hidden)]
pub mod github;

#[cfg(test)]
mod tests {
    //! Compile-time proofs for feature-gated module visibility.

    /// Verify the `cli` module is available when the `cli` feature is enabled.
    ///
    /// This is a compile-time proof: if the test compiles, the module is
    /// accessible. The test body is intentionally minimal.
    #[cfg(feature = "cli")]
    #[test]
    fn cli_module_is_available_with_feature() {
        assert!(!std::any::type_name::<crate::cli::Cli>().is_empty());
    }
}
