//! Configuration system for podbot.
//!
//! This module provides the configuration structures and CLI definitions for the
//! podbot application. Configuration loading and precedence merging is handled by
//! the `ortho_config` crate. Intended precedence: CLI flags override environment
//! variables, which override configuration files, which override defaults.
//!
//! The configuration file is expected at `~/.config/podbot/config.toml` by default.
//!
//! # Example Configuration
//!
//! ```toml
//! engine_socket = "unix:///run/user/1000/podman/podman.sock"
//! image = "ghcr.io/example/podbot-sandbox:latest"
//!
//! [github]
//! app_id = 12345
//! installation_id = 67890
//! private_key_path = "/home/user/.config/podbot/github-app.pem"
//!
//! [workspace]
//! base_dir = "/work"
//!
//! [sandbox]
//! privileged = false
//! mount_dev_fuse = true
//!
//! [agent]
//! kind = "claude"
//! mode = "podbot"
//! ```

mod cli;
mod types;

#[cfg(test)]
mod tests;

pub use cli::{Cli, Commands, ExecArgs, RunArgs, StopArgs, TokenDaemonArgs};
pub use types::{
    AgentConfig, AgentKind, AgentMode, AppConfig, CredsConfig, GitHubConfig, SandboxConfig,
    WorkspaceConfig,
};
