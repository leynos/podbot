//! Internal exec-session options for protocol and test seams.

use super::protocol::ProtocolSessionOptions;

/// Capability-enforcement policy for Agentic Control Protocol (ACP) hosting.
///
/// `Disabled` is the default and matches the original
/// [`super::ExecMode::Protocol`] contract: a byte-transparent stdin/stdout
/// proxy. `MaskOnly` rewrites the first ACP `initialize` frame to strip
/// `terminal/*` and `fs/*` capability advertisements but otherwise forwards
/// every later frame unchanged. `MaskAndDeny` additionally enforces a
/// runtime denylist on agent-emitted JSON-RPC frames, refusing
/// `terminal/*` and `fs/*` requests with a synthesized JSON-RPC error
/// response and recording each denial on stderr.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "MaskOnly and MaskAndDeny remain unused until podbot host wires the opt-in"
    )
)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum CapabilityPolicy {
    /// Pure byte proxy; no ACP-specific behaviour.
    #[default]
    Disabled,
    /// Init-time capability masking only (Step 2.6.1 behaviour).
    MaskOnly,
    /// Init-time masking plus runtime method denylist (Step 2.6.2 behaviour).
    MaskAndDeny,
}

impl CapabilityPolicy {
    /// Return `true` when the policy should rewrite the first ACP
    /// `initialize` frame to mask blocked capabilities.
    pub(crate) const fn rewrites_initialize(self) -> bool {
        matches!(self, Self::MaskOnly | Self::MaskAndDeny)
    }

    /// Return `true` when the policy should enforce the runtime
    /// method denylist on agent-emitted frames.
    pub(crate) const fn allows_runtime_enforcement(self) -> bool {
        matches!(self, Self::MaskAndDeny)
    }
}

/// Internal exec-session knobs used by test harnesses that need deterministic
/// stream behaviour.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ExecSessionOptions {
    disable_protocol_stdin_forwarding: bool,
    capability_policy: CapabilityPolicy,
}

impl ExecSessionOptions {
    /// Create default exec-session options with production behaviour.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            disable_protocol_stdin_forwarding: false,
            capability_policy: CapabilityPolicy::Disabled,
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

    /// Select the [`CapabilityPolicy`] for this protocol-mode session.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "reserved for production ACP session selection when podbot host is enabled"
        )
    )]
    #[must_use]
    pub const fn with_capability_policy(mut self, policy: CapabilityPolicy) -> Self {
        self.capability_policy = policy;
        self
    }
}

pub(super) const fn protocol_session_options(
    options: ExecSessionOptions,
) -> ProtocolSessionOptions {
    ProtocolSessionOptions::new()
        .with_stdin_forwarding_disabled(options.disable_protocol_stdin_forwarding)
        .with_capability_policy(options.capability_policy)
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
        assert_eq!(opts.capability_policy, CapabilityPolicy::Disabled);
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
    fn protocol_session_options_reflects_mask_only_policy() {
        let opts = ExecSessionOptions::new().with_capability_policy(CapabilityPolicy::MaskOnly);

        assert_eq!(
            protocol_session_options(opts),
            ProtocolSessionOptions::new().with_capability_policy(CapabilityPolicy::MaskOnly),
        );
    }

    #[test]
    fn protocol_session_options_reflects_mask_and_deny_policy() {
        let opts = ExecSessionOptions::new().with_capability_policy(CapabilityPolicy::MaskAndDeny);

        assert_eq!(
            protocol_session_options(opts),
            ProtocolSessionOptions::new().with_capability_policy(CapabilityPolicy::MaskAndDeny),
        );
    }

    #[test]
    fn capability_policy_rewrites_initialize_for_masked_modes() {
        assert!(!CapabilityPolicy::Disabled.rewrites_initialize());
        assert!(CapabilityPolicy::MaskOnly.rewrites_initialize());
        assert!(CapabilityPolicy::MaskAndDeny.rewrites_initialize());
    }

    #[test]
    fn capability_policy_runtime_enforcement_only_for_mask_and_deny() {
        assert!(!CapabilityPolicy::Disabled.allows_runtime_enforcement());
        assert!(!CapabilityPolicy::MaskOnly.allows_runtime_enforcement());
        assert!(CapabilityPolicy::MaskAndDeny.allows_runtime_enforcement());
    }
}
