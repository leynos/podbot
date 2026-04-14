//! Internal exec-session options for protocol and test seams.

use super::protocol::ProtocolSessionOptions;

/// Internal exec-session knobs used by test harnesses that need deterministic
/// stream behaviour.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ExecSessionOptions {
    disable_protocol_stdin_forwarding: bool,
}

impl ExecSessionOptions {
    /// Create default exec-session options with production behaviour.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            disable_protocol_stdin_forwarding: false,
        }
    }

    /// Disable protocol stdin forwarding for tests that must avoid inherited
    /// process stdin.
    #[must_use]
    pub const fn with_protocol_stdin_forwarding_disabled(mut self, disable: bool) -> Self {
        self.disable_protocol_stdin_forwarding = disable;
        self
    }
}

pub(super) const fn protocol_session_options(
    options: ExecSessionOptions,
) -> ProtocolSessionOptions {
    ProtocolSessionOptions::new()
        .with_stdin_forwarding_disabled(options.disable_protocol_stdin_forwarding)
}
