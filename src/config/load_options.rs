//! Library-facing configuration load options.
//!
//! This module defines the configuration-loading inputs that embedders pass to
//! the podbot library. The types here intentionally avoid depending on Clap or
//! any CLI parse structures so host applications can configure podbot without
//! going through command-line parsing.

use camino::Utf8PathBuf;

/// High-precedence configuration overrides supplied by the host application.
///
/// These overrides are applied after configuration files and environment
/// variables, matching the precedence of CLI flags in the `podbot` binary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigOverrides {
    /// Container engine socket path or URL.
    pub engine_socket: Option<String>,

    /// Sandbox container image reference.
    pub image: Option<String>,
}

impl ConfigOverrides {
    /// Returns whether no overrides are present.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.engine_socket.is_none() && self.image.is_none()
    }
}

/// Options controlling how podbot loads configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigLoadOptions {
    /// Optional config-path hint supplied by the host (for example `--config`).
    ///
    /// If the path is present but does not exist, the loader ignores it and can
    /// fall back to discovery, matching the existing `podbot` CLI behaviour.
    pub config_path_hint: Option<Utf8PathBuf>,

    /// Whether to search standard discovery locations when no explicit config
    /// path is usable.
    pub discover_config: bool,

    /// High-precedence overrides supplied by the host application.
    pub overrides: ConfigOverrides,
}

impl Default for ConfigLoadOptions {
    fn default() -> Self {
        Self {
            config_path_hint: None,
            discover_config: true,
            overrides: ConfigOverrides::default(),
        }
    }
}
