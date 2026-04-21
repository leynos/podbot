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
    #[cfg(test)]
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
}
