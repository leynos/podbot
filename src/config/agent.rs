//! Agent configuration types for interactive and hosted runtimes.

use serde::{Deserialize, Serialize};

/// The kind of AI agent to run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    /// Claude Code agent.
    #[default]
    Claude,
    /// `OpenAI` Codex agent.
    Codex,
    /// Custom operator-supplied agent launcher.
    Custom,
}

/// The execution mode for the agent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMode {
    /// Run the agent in podbot-managed interactive mode.
    #[default]
    Podbot,
    /// Run the agent as a Codex App Server.
    CodexAppServer,
    /// Run the agent as an ACP server.
    Acp,
}

impl AgentMode {
    /// Returns the `snake_case` token representation used in configuration and CLI.
    #[must_use]
    pub const fn as_token(&self) -> &'static str {
        match self {
            Self::Podbot => "podbot",
            Self::CodexAppServer => "codex_app_server",
            Self::Acp => "acp",
        }
    }
}

/// Agent execution configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AgentConfig {
    /// The type of agent to run.
    pub kind: AgentKind,

    /// The execution mode for the agent.
    pub mode: AgentMode,

    /// The executable used for custom hosted agents.
    pub command: Option<String>,

    /// Additional command-line arguments passed to the hosted agent.
    pub args: Vec<String>,

    /// Environment variable names copied from the host into the agent runtime.
    pub env_allowlist: Vec<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            kind: AgentKind::Claude,
            mode: AgentMode::Podbot,
            command: None,
            args: Vec::new(),
            env_allowlist: Vec::new(),
        }
    }
}
