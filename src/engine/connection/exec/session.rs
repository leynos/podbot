//! Internal exec-session options for protocol and test seams.

use super::protocol::ProtocolSessionOptions;

/// Internal exec-session knobs used by test harnesses that need deterministic
/// stream behaviour.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ExecSessionOptions {
    /// When `true`, protocol-mode sessions replace host stdin with a held-open
    /// no-op reader so that the process's inherited stdin is not forwarded.
    disable_protocol_stdin_forwarding: bool,
    /// When `true`, the first ACP `initialize` frame sent from host stdin is
    /// rewritten to remove `terminal` and `fs` capabilities before being
    /// forwarded to the container.
    rewrite_acp_initialize: bool,
}

impl ExecSessionOptions {
    /// Create default exec-session options with production behaviour.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            disable_protocol_stdin_forwarding: false,
            rewrite_acp_initialize: false,
        }
    }

    /// Disable protocol stdin forwarding for tests that must avoid inherited
    /// process stdin.
    #[cfg(test)]
    #[must_use]
    pub const fn with_protocol_stdin_forwarding_disabled(mut self, disable: bool) -> Self {
        self.disable_protocol_stdin_forwarding = disable;
        self
    }

    /// Enable ACP initialization rewriting for protocol-mode sessions.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "reserved for production ACP session selection when podbot host is enabled"
        )
    )]
    #[must_use]
    pub const fn with_acp_initialize_rewrite_enabled(mut self, enable: bool) -> Self {
        self.rewrite_acp_initialize = enable;
        self
    }
}

/// Convert exec-session options into the lower-level [`ProtocolSessionOptions`]
/// consumed by the protocol proxy loop.
pub(super) const fn protocol_session_options(
    options: ExecSessionOptions,
) -> ProtocolSessionOptions {
    ProtocolSessionOptions::new()
        .with_stdin_forwarding_disabled(options.disable_protocol_stdin_forwarding)
        .with_acp_initialize_rewrite_enabled(options.rewrite_acp_initialize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_session_options_default_enables_stdin_forwarding() {
        let opts = ExecSessionOptions::new();
        assert!(
            !opts.disable_protocol_stdin_forwarding,
            "stdin forwarding should be enabled by default",
        );
        assert!(
            !opts.rewrite_acp_initialize,
            "ACP initialize rewriting should be opt-in",
        );
    }

    #[test]
    fn exec_session_options_disable_flag_can_be_set() {
        let opts = ExecSessionOptions::new().with_protocol_stdin_forwarding_disabled(true);
        assert!(
            opts.disable_protocol_stdin_forwarding,
            "stdin forwarding should be disabled after setting the flag",
        );
    }

    #[test]
    fn exec_session_options_builder_is_non_mutating() {
        let original = ExecSessionOptions::new();
        let updated = original.with_protocol_stdin_forwarding_disabled(true);
        assert!(
            !original.disable_protocol_stdin_forwarding,
            "original should be unchanged",
        );
        assert!(
            updated.disable_protocol_stdin_forwarding,
            "updated copy should have the flag set",
        );
    }

    #[test]
    fn protocol_session_options_reflects_exec_session_options_flag() {
        let enabled_opts = ExecSessionOptions::new().with_protocol_stdin_forwarding_disabled(false);
        let disabled_opts = ExecSessionOptions::new().with_protocol_stdin_forwarding_disabled(true);

        let enabled_proto = protocol_session_options(enabled_opts);
        let disabled_proto = protocol_session_options(disabled_opts);

        assert_eq!(
            enabled_proto,
            ProtocolSessionOptions::new().with_stdin_forwarding_disabled(false),
        );
        assert_eq!(
            disabled_proto,
            ProtocolSessionOptions::new().with_stdin_forwarding_disabled(true),
        );
    }

    #[test]
    fn protocol_session_options_reflects_acp_rewrite_flag() {
        let opts = ExecSessionOptions::new().with_acp_initialize_rewrite_enabled(true);

        assert_eq!(
            protocol_session_options(opts),
            ProtocolSessionOptions::new().with_acp_initialize_rewrite_enabled(true),
        );
    }
}
