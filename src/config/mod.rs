//! Configuration system for podbot.
//!
//! This module provides the configuration structures and library-facing loading
//! API for the podbot application. Configuration loading and precedence merging
//! is handled by the `ortho_config` crate. Intended precedence: host overrides
//! (for example CLI flags) override environment variables, which override
//! configuration files, which override defaults.
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
//! selinux_label_mode = "disable_for_container"
//!
//! [agent]
//! kind = "claude"
//! mode = "podbot"
//! ```

mod env_vars;
mod load_options;
mod loader;
mod types;

#[cfg(test)]
mod tests;

pub use env_vars::env_var_names;
pub use load_options::{ConfigLoadOptions, ConfigOverrides};
pub use loader::{load_config, load_config_with_env};
pub use types::{
    AgentConfig, AgentKind, AgentMode, AppConfig, CredsConfig, GitHubConfig, SandboxConfig,
    SelinuxLabelMode, WorkspaceConfig,
};
