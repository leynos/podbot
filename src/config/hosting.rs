//! MCP hosting defaults for hosted app-server mode.

use serde::{Deserialize, Serialize};

/// Address binding strategy for HTTP-facing MCP bridges.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpBindStrategy {
    /// Reach the bridge through the host gateway instead of a wide-open port.
    #[default]
    HostGateway,
    /// Bind only to loopback.
    Loopback,
}

/// Auth-token issuance policy for hosted MCP bridges.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpAuthTokenPolicy {
    /// Issue one token per workspace.
    #[default]
    PerWorkspace,
    /// Issue a distinct token per wire.
    PerWire,
}

/// Cross-origin policy for HTTP-facing MCP endpoints.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpAllowedOriginPolicy {
    /// Accept same-origin requests only.
    #[default]
    SameOrigin,
    /// Accept any origin.
    Any,
}

/// Defaults for Podbot-managed MCP hosting.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct McpConfig {
    /// Network exposure strategy for the bridge.
    pub bind_strategy: McpBindStrategy,

    /// Idle timeout in seconds before an unused bridge is retired.
    pub idle_timeout_secs: u64,

    /// Maximum message size accepted by the bridge.
    pub max_message_size_bytes: u64,

    /// Auth-token issuance policy for hosted bridges.
    pub auth_token_policy: McpAuthTokenPolicy,

    /// Allowed-origin policy for HTTP-facing endpoints.
    pub allowed_origin_policy: McpAllowedOriginPolicy,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            bind_strategy: McpBindStrategy::HostGateway,
            idle_timeout_secs: 900,
            max_message_size_bytes: 1_048_576,
            auth_token_policy: McpAuthTokenPolicy::PerWorkspace,
            allowed_origin_policy: McpAllowedOriginPolicy::SameOrigin,
        }
    }
}
