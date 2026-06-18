# Step 2.6.3: Add the explicit Agentic Control Protocol (ACP) delegation override

This ExecPlan (execution plan) is a living document. The sections `Constraints`,
`Tolerances`, `Risks`, `Progress`, `Surprises and discoveries`,
`Decision log`, and `Outcomes and retrospective` must be kept up to date as
work proceeds.

Status: DRAFT

This is the governing implementation document for roadmap entry 2.6.3 in
`docs/podbot-roadmap.md`. No `PLANS.md` file exists in this repository.

## Purpose and big picture

Step 2.6.1 stripped `terminal/*` and `fs/*` capability advertisements from the
client-side ACP `initialize` frame. Step 2.6.2 closed the symmetric door by
denying agent-emitted `terminal/*` and `fs/*` JavaScript Object Notation Remote
Procedure Call (JSON-RPC) requests after initialization. Both Steps shipped
their enforcement behind the
`CapabilityPolicy::{Disabled, MaskOnly, MaskAndDeny}` opt-in, which remains
production-dead until something selects a non-`Disabled` variant. The
`docs/podbot-design.md` "Execution flow" section captures the open requirement:

> The delegation override must be explicit, disabled by default, and surfaced
> in logs as a trust-boundary change.

Step 2.6.3 delivers that override. Concretely, this Step:

1. Introduces an operator-facing configuration knob,
   `acp.delegation`, which defaults to `disabled` (the sandbox-preserving
   posture) and can be relaxed to `allow_fs`, `allow_terminal`, or `allow_all`.
   The chosen value is the *single* explicit lever that an operator pulls to
   opt back in to Agentic Control Protocol host-side delegation.
2. Wires the existing `CapabilityPolicy` and `MethodDenylist` machinery to be
   selected from configuration when `agent.mode = "acp"`, replacing the dead-
   code annotation on `ExecSessionOptions::with_capability_policy`.
3. Parametrizes the initialize-time masker (`acp_helpers`) and the runtime
   denylist (`acp_policy`) so a relaxed override narrows *both* the masking set
   and the denylist set consistently. Without this, a partial override would
   still cause the masker to strip a capability that the runtime allows,
   producing observable inconsistency at the ACP handshake.
4. Adds semantic configuration validation: a non-default `acp.delegation`
   value is illegal unless `agent.mode = "acp"`; an invalid token is rejected
   at config load; and a non-default value produces a *warning-severity*
   `tracing::warn!` line on stderr at load time.
5. Adds a *trust-boundary audit event*: at protocol session startup, when
   the effective `CapabilityPolicy` is not `MaskAndDeny` with the default
   family list, podbot emits a single `tracing::warn!` line on the
   `podbot::acp::policy` target carrying the container identifier, the selected
   override value, the resulting `CapabilityPolicy`, and the resulting denylist
   family list. This is the audit trail that makes the trust-boundary change
   visible in logs, mirroring the precedent recorded in Architectural Decision
   Record (ADR) 008 for unpinned hook images.
6. Reserves the JSON-RPC application error code `-32002` (named
   `METHOD_OVERRIDE_REQUIRED_ERROR_CODE`) as a documented constant in
   `acp_policy.rs`, so future overrides that need to surface "this method is
   blocked but operator override would unblock it" diagnostics have the
   reserved code in place.

Observable success for this Step:

- A fresh `podbot host` (or library-driven ACP session) with default
  configuration enforces `CapabilityPolicy::MaskAndDeny` against the default
  `terminal/` and `fs/` families. Hosted agents continue to receive the Step
  2.6.2 synthesized JSON-RPC error response for blocked methods. The
  audit-event line is *not* emitted at startup (the posture is the safe
  default).
- Setting `acp.delegation = "allow_fs"` in configuration (or via the
  `PODBOT_ACP_DELEGATION` environment variable, or via a
  `--acp-delegation allow_fs` flag where exposed) causes the masker to leave
  `fs` in `clientCapabilities` and the runtime denylist to forward `fs/*`
  methods byte-identically while continuing to deny `terminal/*`.
- The same `allow_fs` configuration produces exactly one
  `tracing::warn!` line at config load with target `podbot::acp::policy` and
  message `"ACP delegation override enabled"`, and exactly one `tracing::warn!`
  line per session at startup with the same target and message
  `"ACP delegation override active for session"`.
- A configuration containing `acp.delegation = "allow_all"` together with
  `agent.mode = "podbot"` (interactive) fails semantic validation with
  `ConfigError::InvalidValue` and a hint pointing at `agent.mode = "acp"`.
- A configuration containing `acp.delegation = "rumpelstiltskin"` fails
  serde deserialization with a friendly error pointing at the legal values.
- `rstest` unit coverage, `rstest-bdd` v0.5.0 behavioural coverage, and a
  `proptest` property covering "narrowed denylist preserves the
  default-denylist behaviour on the still-blocked families" all pass before and
  after the change in the expected directions.
- `docs/podbot-design.md`, `docs/users-guide.md`, and
  `docs/developers-guide.md` describe the override knob, its precedence, its
  audit line, and its safety semantics.
- Only the third Step 2.6 roadmap checkbox is marked done.

## Constraints

- Scope is limited to roadmap entry 2.6.3 in `docs/podbot-roadmap.md`. The
  metric instrumentation contract sketched in `docs/developers-guide.md` §8.2.3
  (gauges, counters, span correlation) is *not* delivered by this Step; it
  remains follow-on work as the developers-guide already records.
- Preserve the protocol proxy contract from Step 2.5: stdout purity,
  stderr-only diagnostics, bounded buffering, explicit stdin shutdown ordering,
  and accurate exit-code reporting.
- Preserve the Step 2.6.1 tolerant first-frame masking semantics. Frames
  that are not parseable ACP `initialize` requests are still forwarded
  unchanged regardless of override.
- Preserve the Step 2.6.2 byte-identical forwarding contract for permitted
  frames. The relaxed denylist must continue to forward the original bytes, not
  re-serialize.
- Default behaviour of `ExecMode::Protocol` must remain a raw byte proxy.
  The default for `acp.delegation` is `disabled`, and the default
  `CapabilityPolicy` outside `agent.mode = "acp"` remains `Disabled`. Only
  `agent.mode = "acp"` plus the new wiring elevates the policy to `MaskAndDeny`.
- Do not introduce new runtime dependencies. `proptest` is already
  available as a workspace dev-dependency (verified during planning); reuse it.
  Do not add a new crate for configuration parsing.
- Keep every touched module within the 400-line guidance from `AGENTS.md`.
  The pure delegation selector and its tests go into a new module
  `src/engine/connection/exec/acp_delegation.rs` and matching tests file so
  `acp_policy.rs` (currently ~200 lines) and `acp_helpers.rs` (~280 lines) stay
  under the ceiling.
- New configuration code lives in a new module `src/config/acp.rs`,
  mirroring the layout of `src/config/hosting.rs` (which holds `McpConfig`), so
  existing `src/config/types.rs` and `src/config/agent.rs` are not forced over
  the line limit.
- Hexagonal architecture (see the `hexagonal-architecture` skill) applies
  to the *boundary*, not as a structural transplant. Specifically: the pure
  delegation selector and the pure capability mask live in the engine module's
  policy layer; configuration deserialisation and validation live in
  `src/config/`; the audit-emission `tracing::warn!` adapter lives in the thin
  wiring layer that constructs `ExecSessionOptions`. Tests for the selector are
  pure and run without any I/O or `tracing` setup.
- Production code must be panic-free. No new `unwrap()` or `expect()`.
- Use British English with Oxford spelling
  (`-ize`, `-yse`, `-our`, Oxford comma when it improves clarity) in all
  documentation and code comments, except references to external Application
  Programming Interface (API) identifiers (such as `clientCapabilities`).
- Run the full Rust gate stack before each milestone commit:
  `make check-fmt`, `make lint`, `make test`. Run the documentation gates
  (`make fmt`, `make markdownlint`, `make nixie`) when Markdown files are
  touched. Run gates *sequentially*, never in parallel, so the shared Cargo
  cache is honoured.
- Pipe long-running gate output through `tee` to
  `/tmp/$ACTION-podbot-$(git branch --show-current).out` so truncated console
  output does not hide failures.
- After each major milestone, run `coderabbit review --agent` and resolve
  every concern before requesting a CodeRabbit review for the next milestone.
  Do not use CodeRabbit for issues already catchable by `make lint` or
  `make test`.
- Commit early and often. Each Stage's "validation gate" is a natural
  commit boundary.

## Tolerances (exception triggers)

Stop and escalate (do not improvise) when any of the following occurs.

- Scope tolerance: implementation requires touching more than twelve files
  or exceeding roughly 1100 net lines added (production plus tests, excluding
  documentation and generated bindings).
- Interface tolerance: completing the change forces a *public* Application
  Programming Interface (API) signature break in `src/api/exec.rs`,
  `src/api/mod.rs`, `src/config/mod.rs`, or `src/lib.rs`. Adding new public
  types is acceptable; changing the shape of existing public types is not.
- Default-behaviour tolerance: any path through the change alters
  observable behaviour when `acp.delegation` is unset or `disabled`. The
  default posture must be byte-identical to Step 2.6.2 once
  `agent.mode = "acp"` is configured.
- Dependency tolerance: a new crate is required. `proptest`,
  `ortho_config`, `serde`, `serde_json`, and `tracing` are already in the
  workspace; no others may be added.
- Iteration tolerance: any of `make check-fmt`, `make lint`, or
  `make test` still fails after three focused fix passes against a single
  failure mode.
- Concurrency tolerance: wiring the override through the existing
  `ExecSessionOptions` path requires changing the sink-task or channel
  ownership established in Step 2.6.2.
- Ambiguity tolerance: a roadmap or design requirement admits more than
  one defensible interpretation; surface the options before proceeding.

## Risks

- Risk: the override invites partial inconsistency between init-time
  masking and runtime denylist. For example, if the masker is *not*
  parametrized and the runtime *is*, then `allow_fs` would strip `fs` from
  `clientCapabilities` (signalling lack of support) while still permitting
  `fs/*` calls — a contradiction the hosted agent would never observe a
  capability to call. Severity: high. Likelihood: medium without the
  parametrization. Mitigation: introduce a single `AcpCapabilityMask` value
  that drives both the masker and the runtime denylist; build it inside the
  pure selector so the two enforcement edges cannot disagree. Cover this with a
  behavioural scenario that asserts init masking and runtime denial agree for
  every override value.

- Risk: a misconfiguration that elevates `MaskAndDeny` for non-ACP modes
  would break the byte-transparent contract of `ExecMode::Protocol`. Severity:
  high. Likelihood: low. Mitigation: the selector returns
  `CapabilityPolicy::Disabled` for any mode other than `AgentMode::Acp`, and
  the configuration validation step rejects a non-default `acp.delegation`
  value when `agent.mode != "acp"`.

- Risk: adding a `tracing::warn!` line at session startup runs counter
  to the stdout-purity invariant if it accidentally lands on host stdout.
  Severity: high. Likelihood: low. Mitigation: `tracing::warn!` events are
  routed through the existing tracing subscriber which writes to stderr; the
  only places stdout is touched by Step 2.6.2 are `host_stdout` writes in
  `acp_runtime.rs`, none of which are modified. Cover with a unit test that
  captures the audit emission through the existing
  `tracing_subscriber::fmt::MakeWriter` capture pattern used by
  `src/bin_tests/observability_helpers.rs::capture_run_logs`.
- Risk: the `MethodDenylist` is currently `&'static [MethodFamily]`,
  which forces every override permutation to ship as a separate static array.
  Severity: medium. Likelihood: high if implemented naively. Mitigation:
  enumerate the legal permutations as a small set of pre-allocated `&'static`
  arrays (the override space is closed: `disabled`, `allow_fs`,
  `allow_terminal`, `allow_all`), keep the existing
  `MethodDenylist::new(&'static [MethodFamily])` API untouched, and add a pure
  constructor `MethodDenylist::for_override(AcpDelegationOverride)` that
  selects among the static arrays. No `Cow` or `Vec` is required, so the
  existing zero-allocation policy path is preserved.

- Risk: ortho-config layering may surprise an operator who sets
  `acp.delegation` at the environment-variable layer but expects the
  config-file value to win. Severity: medium. Likelihood: medium. Mitigation:
  rely on the existing precedence rules captured in
  `docs/ortho-config-users-guide.md` (defaults < file < env < CLI flags), add
  documentation examples to `docs/users-guide.md` that show the precedence
  explicitly, and add a `rstest` parameterized test in
  `src/config/tests/layer_precedence_tests.rs` (the existing location) covering
  every override permutation across every layer.

- Risk: a legitimate ACP agent that the operator *wants* to delegate to
  emits `fs/read_text_file` while `acp.delegation` is still `disabled`, causing
  a confusing JSON-RPC `-32001` error in the agent's logs. Severity: low.
  Likelihood: medium. Mitigation: the `-32001` error message and `data.reason`
  field already point operators at the policy; this Step reserves
  `-32002 METHOD_OVERRIDE_REQUIRED_ERROR_CODE` so a *future* Step may upgrade
  the response to specifically advertise the override path. Documented in the
  users-guide as the operator remediation hint.

- Risk: feature-file edits for `rstest-bdd` are compile-time inputs;
  stale generated bindings can mask scenario-name mismatches. Severity: medium.
  Likelihood: medium. Mitigation: keep scenario titles synchronized with the
  feature file and trigger a clean rebuild
  (`cargo clean -p podbot && make test 2>&1 | tee ...`) once if generated
  bindings appear stale.

- Risk: `proptest` shrink reports may be noisy for failures involving
  randomly generated method names. Severity: low. Likelihood: low. Mitigation:
  constrain the strategy to ASCII alphanumeric methods plus the `/` delimiter,
  and assert the property only on the *blocked-family partition*, so failures
  localize quickly.

## Context and orientation

Read the following first; the plan assumes nothing else.

- `docs/podbot-roadmap.md` lines 257-282 define Step 2.6 and entry 2.6.3.
  The completion criterion is: "operators can opt into host-side ACP delegation
  only through a visible trust-boundary change covered by configuration
  validation."
- `docs/podbot-design.md` lines 208-274 record the design intent: ACP
  hosting must default to sandbox-preserving masking, must defensively reject
  blocked methods at runtime, and must surface override decisions as
  trust-boundary events.
- `docs/adr-006-define-the-validate-surface-and-capability-disposition-model.md`
  defines the capability disposition taxonomy. ACP `terminal` and `fs` fall
  under the `HostEnforced` disposition by default; this Step keeps that
  disposition unless the override is set, in which case the disposition becomes
  `Native` for the unmasked families.
- `docs/adr-008-define-secrets-and-trust-boundaries-for-hooks-prompts-and-validation.md`
  §"Hook image pinning / Operator override" establishes the precedent that
  trust-boundary relaxations must be explicit, produce a `Warning`-severity
  diagnostic, and be logged as a trust-boundary event. The ACP delegation
  override follows this pattern exactly.
- `docs/execplans/2-6-1-intercept-acp-initialization.md` and
  `docs/execplans/2-6-2-runtime-denylist.md` are the upstream Steps.
- `src/config/types.rs` defines `AppConfig`; new fields land here as a
  `pub acp: AcpConfig` sibling of `pub mcp: McpConfig`.
- `src/config/hosting.rs` is the layout precedent for new substructs.
- `src/config/validation.rs` is where semantic legality (e.g., "override
  requires `agent.mode = acp`") lands.
- `src/config/loader.rs` and `src/config/load_options.rs` are the points
  where environment variables (`PODBOT_ACP_DELEGATION`) and CLI flags
  (`--acp-delegation`) are surfaced.
- `src/cli/mod.rs` exposes `RunArgs` and `HostArgs`; the override flag
  belongs on `HostArgs` because ACP hosting is what the override governs.
- `src/engine/connection/exec/session.rs` owns `ExecSessionOptions` and
  the existing `CapabilityPolicy` enum. The dead-code annotation on
  `with_capability_policy` is removed by this Step.
- `src/engine/connection/exec/acp_policy.rs` owns `MethodDenylist`,
  `DEFAULT_BLOCKED_FAMILIES`, and `METHOD_BLOCKED_ERROR_CODE`. The new reserved
  `METHOD_OVERRIDE_REQUIRED_ERROR_CODE` constant and the override- derived
  static slices live here.
- `src/engine/connection/exec/acp_helpers.rs` owns the init-time
  rewriter. This Step parametrizes `mask_acp_initialize_frame` to take an
  `AcpCapabilityMask`.
- `src/engine/connection/exec/protocol.rs` owns
  `ProtocolSessionOptions` and the protocol session driver. This Step
  introduces the audit `tracing::warn!` emission at session start and threads
  the override-derived denylist into the `OutboundFrameAssembler` constructor.
- `src/engine/connection/exec/acp_runtime.rs` already accepts a
  `MethodDenylist` through the assembler; this Step does not modify its
  internal behaviour.
- `src/api/exec.rs` and `src/api/run.rs` are the library-facing entry
  points; the override is consumed via `AppConfig` and does not require changes
  to these public surfaces.

ACP framing assumed by this plan (consistent with Step 2.6.2):

- ACP traffic is line-delimited JSON-RPC 2.0. Each frame is a single
  JSON-RPC object terminated by `\n` (or `\r\n`).
- The `initialize` request's `params.clientCapabilities` advertises
  `terminal: bool` and `fs: { readTextFile, writeTextFile }`. Masking removes
  the *key*, not just the value, so the agent treats the capability as
  `UNSUPPORTED` per the ACP specification at
  <https://agentclientprotocol.com/protocol/initialization#client-capabilities>.
- The `terminal/*` family is governed by the single `terminal` boolean;
  the `fs/*` family is governed by the `fs.readTextFile`/`fs.writeTextFile`
  sub-booleans. For this Step the override granularity is per-family (terminal
  vs fs as a whole), not per-method, because the JSON-RPC method prefix is the
  natural enforcement axis and matches the existing `MethodDenylist` shape.
- JSON-RPC application error codes `-32099..=-32000` are spec-conformant
  for application-defined errors. `-32001` (already used) is "method blocked by
  capability policy"; `-32002` (reserved by this Step) is "method blocked,
  operator override would unblock it" for future use.

## Plan of work

Each Stage ends with a validation gate. Do not proceed past a failing
validation. Each Stage's "Validation" section is a natural commit point.

### Stage A: Confirm the landing zones (no code changes)

Read the modules listed under `Context and orientation` and confirm:

- the `MethodDenylist::new(&'static [MethodFamily])` constructor remains the
  only seam through which `acp_policy` accepts blocked families. (Verified at
  planning time in `acp_policy.rs:62-77`.)
- `ExecSessionOptions::with_capability_policy` is annotated with
  `#[cfg_attr(not(test), expect(dead_code, ...))]` and is reachable from a
  single call site through `protocol_session_options`. (Verified at
  `session.rs:46-58` and `session.rs:63-69`.)
- `mask_acp_initialize_frame` strips both `terminal` and `fs` keys
  unconditionally. (Verified at `acp_helpers.rs:198-229`.)
- No public Application Programming Interface (API) consumer of
  `AppConfig` or `AgentConfig` constructs `AppConfig` by struct-literal in a
  way that would be broken by adding a new `acp` field; the existing
  `AppConfig::default()` and serde-driven construction are the only paths.

Validation: a one-paragraph note in `Surprises and discoveries` if any
assumption fails. Otherwise proceed.

### Stage B: Add the configuration domain

Create `src/config/acp.rs` with:

- a `pub enum AcpDelegationOverride { Disabled, AllowFs, AllowTerminal,
  AllowAll }` deriving `Debug, Clone, Copy, Default, PartialEq, Eq, Serialize,
  Deserialize`, with `#[serde(rename_all = "snake_case")]` and `#[default]
  Disabled`.
- a `pub struct AcpConfig { pub delegation: AcpDelegationOverride }`
  deriving the same traits plus `Default`, with `#[serde(default)]` on the
  struct so legacy configurations omit the section without failure.
- An `impl AcpConfig` exposing `pub const fn is_default(&self) -> bool`
  used by both validation and the audit-event emitter.

Add unit tests in `src/config/tests/acp_tests.rs` covering:

- serde round-trip for each `AcpDelegationOverride` variant in TOML, JSON,
  and YAML;
- default deserialisation produces `AcpDelegationOverride::Disabled`;
- unknown variants are rejected with an error message that names the
  legal tokens;
- `AcpConfig::default()` is `is_default()`.

Add the `pub acp: AcpConfig` field to `AppConfig` in `src/config/types.rs` with
`#[serde(default)]`. Update `src/config/mod.rs` to re-export `AcpConfig` and
`AcpDelegationOverride` so the public boundary is explicit.

Validation: `make check-fmt`, `make lint`, `make test`. The new tests fail
before the module is implemented and pass after. Commit at this gate.

### Stage C: Semantic validation and the load-time warning

In `src/config/validation.rs`, extend `validate_agent_config` with a new
`validate_acp_delegation` helper that:

- returns `Ok(())` when `self.acp.delegation == Disabled`;
- returns `Ok(())` when `self.acp.delegation != Disabled` and
  `self.agent.mode == AgentMode::Acp`;
- returns `ConfigError::InvalidValue` for any other case, with field
  `"acp.delegation"` and reason naming `agent.mode = "acp"` as the prerequisite.

After validation succeeds, when `self.acp.delegation != Disabled`, emit a single
`tracing::warn!` line with `target = "podbot::acp::policy"` and message
`"ACP delegation override enabled"`, carrying the override token (`Display`
form), the configured `agent.mode`, and the configured `agent.kind`. The
emission is the *only* I/O the validation function performs; it is a thin
driven-adapter call on top of the pure domain decision.

Extend `src/config/tests/semantic_validation_tests.rs` with `rstest` cases
covering:

- `(agent.mode = acp, delegation = disabled)` → Ok;
- `(agent.mode = acp, delegation = allow_fs)` → Ok;
- `(agent.mode = acp, delegation = allow_terminal)` → Ok;
- `(agent.mode = acp, delegation = allow_all)` → Ok;
- `(agent.mode = podbot, delegation = allow_fs)` → InvalidValue;
- `(agent.mode = codex_app_server, delegation = allow_all)` →
  InvalidValue;
- a log-capture test (modelled on the existing
  `capture_run_logs` helper in `src/bin_tests/observability_helpers.rs`, which
  uses `tracing_subscriber::fmt::MakeWriter` plus
  `tracing::subscriber::with_default`) that asserts a `WARN`-level event with
  `target = "podbot::acp::policy"` and the message
  `"ACP delegation override enabled"` is produced exactly once when the
  override is non-default. No new test crate is required; reuse the existing
  `tracing-subscriber` dev-dependency.

Validation: `make check-fmt`, `make lint`, `make test`. Commit at this gate.

### Stage D: Pure delegation selector and `MethodDenylist` parametrization

Create `src/engine/connection/exec/acp_delegation.rs` containing the pure
domain selector. The module deliberately does *not* depend on `tokio` or
`tracing`:

- a `pub(super) struct AcpCapabilityMask { pub strip_terminal: bool,
  pub strip_fs: bool }` value type produced by the selector;
- a `pub(super) struct AcpSessionPolicy { pub policy: CapabilityPolicy,
  pub denylist: MethodDenylist, pub mask: AcpCapabilityMask }` aggregate;
- the selector function:

  ```rust,no_run
  pub(super) const fn select_acp_session_policy(
      mode: AgentMode,
      override_: AcpDelegationOverride,
  ) -> AcpSessionPolicy;
  ```

  The function:

  - returns `AcpSessionPolicy { policy: Disabled, denylist:
    MethodDenylist::default_families(), mask: AcpCapabilityMask { false,
    false } }` for any `mode != AgentMode::Acp`. The masker and runtime
    enforcement are never reached, but the value is still well-defined.
  - returns `AcpSessionPolicy { policy: MaskAndDeny, denylist: <full>,
    mask: <full> }` for `(Acp, Disabled)`.
  - returns `MaskAndDeny` with `terminal/`-only denylist and mask for
    `(Acp, AllowFs)`.
  - returns `MaskAndDeny` with `fs/`-only denylist and mask for
    `(Acp, AllowTerminal)`.
  - returns `Disabled` (full byte-transparency) for `(Acp, AllowAll)`.

In `src/engine/connection/exec/acp_policy.rs`, add:

- four `static` arrays of `MethodFamily`:
  `FAMILIES_FULL` (existing `DEFAULT_BLOCKED_FAMILIES`),
  `FAMILIES_TERMINAL_ONLY`, `FAMILIES_FS_ONLY`, `FAMILIES_EMPTY`;
- a constructor `MethodDenylist::for_override(AcpDelegationOverride) ->
  Self` that selects among the four static arrays;
- the reserved constant
  `METHOD_OVERRIDE_REQUIRED_ERROR_CODE: i64 = -32_002` (with `pub(crate)`
  visibility), carrying a doc comment that explains it is reserved for future
  override-required diagnostics and is not used in this Step.

In `src/engine/connection/exec/acp_helpers.rs`, parametrize
`mask_acp_initialize_frame` to accept an `AcpCapabilityMask`:

```rust,no_run
pub(super) fn mask_acp_initialize_frame(
    frame: &[u8],
    mask: AcpCapabilityMask,
) -> Vec<u8>;
```

When `mask.strip_terminal` is true, remove the `terminal` key; when
`mask.strip_fs` is true, remove the `fs` key. When both are false, return the
frame unchanged (the frame may still be a valid ACP `initialize`, but no
capabilities need masking). Update `read_and_mask_initial_acp_frame` and its
callers to thread an `AcpCapabilityMask` through; the existing tests pass
`AcpCapabilityMask { strip_terminal: true, strip_fs: true }` to preserve
behaviour.

Add unit tests in `src/engine/connection/exec/acp_delegation_tests.rs` covering
every `(AgentMode, AcpDelegationOverride)` pair and asserting the returned
`AcpSessionPolicy` values. Add `rstest` parameterized cases.

Add a `proptest` property in the same tests module:

```rust,no_run
proptest! {
    #[test]
    fn narrowed_denylist_preserves_default_blocking_for_still_blocked_families(
        ovr in any::<AcpDelegationOverride>(),
        method in "[a-z]+/[a-z]+",
    ) {
        let policy = select_acp_session_policy(AgentMode::Acp, ovr);
        let default = MethodDenylist::default_families();
        if default.is_blocked(&method) && !policy.mask.allows(&method) {
            prop_assert!(policy.denylist.is_blocked(&method));
        }
    }
}
```

The intent: any method that the default denylist blocks must remain blocked
unless the override explicitly relaxes its family. A helper
`AcpCapabilityMask::allows(&str) -> bool` predicates the property; the helper
is also useful for tests.

Validation: `make check-fmt`, `make lint`, `make test`. Commit at this gate.

### Stage E: Wire the selector into protocol sessions

In `src/engine/connection/exec/session.rs`:

- Remove the `#[cfg_attr(not(test), expect(dead_code, ...))]` annotation
  from `ExecSessionOptions::with_capability_policy` (it is now production code).
- Add a sibling field
  `pub(crate) denylist: MethodDenylist`, `pub(crate) mask: AcpCapabilityMask`,
  default-constructed to the full-blocking pair so the defaults preserve Step
  2.6.2 behaviour.
- Add a builder `pub(crate) fn with_acp_session_policy(self, policy:
  AcpSessionPolicy) -> Self` that sets `capability_policy`, `denylist`, and `
  mask` from the aggregate. Keep `with_capability_policy` for tests.

In `src/engine/connection/exec/protocol.rs`:

- Extend `ProtocolSessionOptions` with `denylist: MethodDenylist` and
  `mask: AcpCapabilityMask` fields and a `with_acp_session_policy` builder.
- When constructing the `OutboundFrameAssembler`, pass the configured
  `denylist` instead of `MethodDenylist::default_families()`.
- When the initialize masker runs (the `MaskOnly` and `MaskAndDeny` paths
  at `protocol.rs:256` and `protocol.rs:291`), pass the configured `mask` to
  `mask_acp_initialize_frame`.
- Add an `emit_trust_boundary_audit_event` helper that takes the
  container identifier and a `&ProtocolSessionOptions`. When the effective
  override is non-default (`mask.is_relaxed()` returns `true`, or the policy is
  not `MaskAndDeny`), the helper emits exactly one `tracing::warn!` line with
  `target = "podbot::acp::policy"` and the message
  `"ACP delegation override active for session"`, carrying the container
  identifier, the selected policy, and the resulting denylist family list
  (rendered as a comma-separated string). The helper is called once at session
  start, before the input and output tasks spawn, so the audit event always
  precedes any forwarded bytes in the stderr log stream.

In the upstream construction path (`api::exec` -> `engine::exec_async`), plumb
`ExecSessionOptions` from the loaded `AppConfig`:

- Add a private helper
  `build_exec_session_options(config: &AppConfig) -> ExecSessionOptions` inside
  `src/engine/connection/exec/session.rs`. The layering keeps the helper inside
  the engine crate but visible to the internal `api::exec` adapter.
- Modify `EngineConnector::exec_async` (or, more precisely, the
  `api::exec::exec_with_client` wrapper at `src/api/exec.rs:255-275`) to
  construct `ExecSessionOptions` from the `AppConfig` passed through
  `ExecContext`, then call `exec_async_with_options` instead of `exec_async`.
  This keeps the public `exec_async` signature stable while threading the
  override through the internal seam.

Validation: `make check-fmt`, `make lint`, `make test`. The previously
dead-coded `with_capability_policy` is now wired and reachable from the
`api::exec` path. Commit at this gate.

### Stage F: Behavioural coverage with `rstest-bdd`

Create `tests/features/acp_delegation_override.feature` with scenarios that
exercise the end-to-end behaviour through the existing `RecordingWriter`-style
doubles used in `src/engine/connection/exec/protocol_acp_bdd_tests.rs` and
`acp_runtime_bdd_tests.rs`.

Scenario set:

1. Default configuration with `agent.mode = "acp"` masks both families
   and blocks both families at runtime.
2. `acp.delegation = "allow_fs"` leaves the `fs` capability advertised in
   `initialize` and forwards `fs/read_text_file` byte-identically while still
   blocking `terminal/create`.
3. `acp.delegation = "allow_terminal"` leaves `terminal` advertised and
   forwards `terminal/create` while blocking `fs/read_text_file`.
4. `acp.delegation = "allow_all"` produces the byte-transparent
   `Disabled` policy: both methods pass through unchanged and no init masking
   is applied.
5. `acp.delegation = "allow_fs"` produces *exactly one* audit
   `tracing::warn!` line at session start with the expected target, message,
   and fields.
6. `acp.delegation = "allow_fs"` together with `agent.mode = "podbot"`
   fails configuration validation with `ConfigError::InvalidValue` and a
   message naming `acp.delegation` and `agent.mode = "acp"`.

Bind the scenarios in `src/engine/connection/exec/acp_delegation_bdd_tests.rs`,
mirroring the `AcpMaskingState` pattern.

Validation: `make test` passes; the new `.feature` scenarios appear in the test
output and fail before the bindings are present, pass after. Commit at this
gate.

### Stage G: Configuration layer-precedence and end-to-end tests

Extend `src/config/tests/layer_precedence_tests.rs` with `rstest` parameterized
cases that confirm `acp.delegation` resolves correctly across the defaults <
file < env < CLI layers, including:

- env-only `PODBOT_ACP_DELEGATION=allow_fs` produces `AllowFs`;
- file-only `[acp] delegation = "allow_terminal"` produces
  `AllowTerminal`;
- file `disabled` + env `allow_all` produces `AllowAll`;
- file `allow_all` + CLI `--acp-delegation disabled` produces
  `Disabled`.

Wire a `--acp-delegation` flag on `HostArgs` in `src/cli/mod.rs` that maps to
the `ConfigLoadOptions::overrides` payload. The flag is *not* added to
`RunArgs` because `agent.mode = "podbot"` makes the override illegal anyway;
surfacing the flag there would only mislead operators.

Extend `src/bin_tests/main_tests.rs` with an end-to-end case that drives
`podbot host` (or the existing host-stub) with `--acp-delegation allow_fs` and
asserts the audit line appears on stderr. Where the `podbot host` subcommand is
still stubbed to return an error, instead assert that the configuration *loads*
and the override is reflected in the resolved `AppConfig`.

Validation: `make check-fmt`, `make lint`, `make test`. Commit at this gate.

### Stage H: Documentation

Update `docs/podbot-design.md` (Execution flow section) to describe:

- the `acp.delegation` configuration knob, its legal tokens, and its
  precedence;
- the four `(mode, override)` selector outcomes and the resulting
  `(CapabilityPolicy, MethodDenylist, AcpCapabilityMask)` triple;
- the audit-event contract (target, level, message, fields, when emitted);
- the reserved `-32002 METHOD_OVERRIDE_REQUIRED_ERROR_CODE` constant and
  its future role.

Update `docs/users-guide.md` to:

- replace the existing reference to roadmap "Step 2.6.4" (stale) with
  Step 2.6.3 and the new override semantics;
- describe the four override values, their effect on the agent, and the
  audit line operators should expect to see in their logs;
- show the precedence example (file, env, CLI) explicitly.

Update `docs/developers-guide.md` §8.2 to:

- document the new `acp_delegation.rs` module and the selector;
- document the `AcpCapabilityMask` and the parametrized masker signature;
- update §8.2.3 to note that the override now selects the
  `CapabilityPolicy` for ACP sessions; metric instrumentation remains follow-on
  work.

Update `docs/podbot-roadmap.md` to mark only the third Step 2.6 checkbox done.

Update
`docs/adr-006-define-the-validate-surface-and-capability-disposition-model.md`
*only* to add a one-paragraph note (in "Outstanding decisions" or as a short
ACP-section) clarifying that when `acp.delegation` is non-default, the ACP
capability disposition in the validate-prompt response shifts from
`HostEnforced` to `Native` for the unmasked families. This keeps the ADR and
the runtime aligned without enlarging ADR 006's scope.

Validation: `make fmt 2>&1 | tee ...`, `make markdownlint 2>&1 | tee ...`, and
`make nixie 2>&1 | tee ...` succeed. Commit at this gate.

### Stage I: Final gate stack and roadmap

Run the full gate stack sequentially with `tee` for each command (do not
parallelize):

```shell
set -o pipefail
make fmt 2>&1 | tee /tmp/fmt-podbot-2-6-3.out
make markdownlint 2>&1 | tee /tmp/markdownlint-podbot-2-6-3.out
make nixie 2>&1 | tee /tmp/nixie-podbot-2-6-3.out
make check-fmt 2>&1 | tee /tmp/check-fmt-podbot-2-6-3.out
make lint 2>&1 | tee /tmp/lint-podbot-2-6-3.out
make test 2>&1 | tee /tmp/test-podbot-2-6-3.out
```

After all gates pass, request `coderabbit review --agent` on the finalised
branch and resolve every concern.

Mark the third Step 2.6 roadmap checkbox done.

## Validation and acceptance

Acceptance is observable through the following experiments.

- Compile-time: `cargo build --workspace --all-targets --all-features`
  succeeds with the new modules `src/config/acp.rs`,
  `src/engine/connection/exec/acp_delegation.rs`, and matching tests present.
- Unit tests: `cargo test -p podbot --lib` reports the new
  `acp_tests`, `acp_delegation_tests`, and parametrized semantic validation
  cases as passing. Each new test fails when run against the tip of `main` and
  passes against this branch.
- Behavioural tests: `cargo test -p podbot --test bdd` (or whichever
  test binary already executes existing ACP scenarios) reports the six new
  `acp_delegation_override` scenarios as passing.
- Property test: the `proptest`-driven invariant
  `narrowed_denylist_preserves_default_blocking_for_still_blocked_families`
  passes for at least 256 cases.
- End-to-end: invoking `podbot host` (or its current stub) with
  `PODBOT_ACP_DELEGATION=allow_fs` loads, validates, and (where the stub
  permits) emits the audit line on stderr.
- Documentation: `docs/podbot-design.md`, `docs/users-guide.md`,
  `docs/developers-guide.md`, and `docs/podbot-roadmap.md` describe and reflect
  the shipped behaviour.
- Roadmap: only the third Step 2.6 checkbox is marked done.
- Gates: `make check-fmt`, `make lint`, `make test`, `make fmt`,
  `make markdownlint`, and `make nixie` all succeed.

Quality criteria:

- No new `clippy` warnings under `-D warnings`.
- No new `unwrap()` or `expect()` in production code.
- Every new module begins with `//!` documentation.
- Every public item retains British English Oxford spelling in its
  documentation.
- The 400-line guidance holds for every touched module.
- `coderabbit review --agent` reports zero unresolved concerns at the
  final commit.

Quality method:

- `make check-fmt`, `make lint`, `make test` run sequentially with
  `tee` output.
- `make fmt`, `make markdownlint`, `make nixie` for documentation
  changes.
- `coderabbit review --agent` after each major milestone.

## Idempotence and recovery

- Configuration changes are additive; the `acp.delegation` field defaults
  to `Disabled` and legacy configurations omit the section without failure.
- The selector and `AcpCapabilityMask` are pure values; running them
  twice for the same inputs produces identical outputs.
- If `rstest-bdd` scenario bindings appear stale, run
  `cargo clean -p podbot && make test 2>&1 | tee ...` once.
- If the `proptest` invariant detects a regression, the failing
  shrunken input can be added as a deterministic `rstest` case before fixing
  the selector.

## Agent team execution model

Run reconnaissance and review concurrently up front so the main implementation
thread stays coherent.

- Lane A (docs and roadmap reconnaissance owner, may be a sub-agent):
  read the roadmap, design, ADRs 006 and 008, and the prior Step 2.6.1 and
  2.6.2 plans to confirm scope and documentation surface. Output: a short note
  flagging any surprises before Stage B begins. Completed during planning; see
  Surprises and discoveries.
- Lane B (Logisphere design review, runs in parallel during planning):
  Pandalump (structure), Wafflecat (alternatives), Buzzy Bee (scale and
  observability), Telefono (configuration contract correctness), Doggylump
  (failure modes and ordering), Dinolump (long-term viability). Output feeds
  the Decision log before Stage D begins. Recommend invoking the
  `logisphere-design-review` skill explicitly before Stage D for this Step.
- Lane C (primary implementation owner, main thread): drive Stages A
  through I sequentially, integrating Lane B findings into the design before
  writing code.

Coordination rule: code edits land only in Lane C. Sub-agents may read and
report; they must not write to the working tree.

## Progress

- [x] (2026-05-29) Drafted ExecPlan after reading the roadmap (2.6.3),
  podbot-design §Execution flow, ADRs 006 and 008, the Corbusier-conformance
  doc, and the prior Step 2.6.1 and 2.6.2 plans. Surveyed `src/config/`,
  `src/engine/connection/exec/`, and `src/api/` to identify the configuration
  seam (`AppConfig.acp`), the engine seam (`ExecSessionOptions`), and the
  existing dead-code annotation on `with_capability_policy`. Researched ACP
  capability advertisement semantics and prior art for sandbox-override flags
  (Podman `--privileged`, Docker `--cap-add`, MCP gateway ACLs, Codex sandbox
  modes, Claude Code permission modes); the per-family override granularity
  matches both the ACP `clientCapabilities` shape and the dominant prior-art
  pattern.
- [ ] Stage A: confirm landing zones; no code changes.
- [ ] Stage B: add `src/config/acp.rs` and the `AppConfig.acp` field.
- [ ] Stage C: semantic validation and load-time audit warning.
- [ ] Stage D: pure delegation selector and `MethodDenylist`
  parametrization (run Logisphere design review before this Stage).
- [ ] Stage E: wire the selector through `ExecSessionOptions` and
  `ProtocolSessionOptions`; emit the session-start audit event.
- [ ] Stage F: behavioural coverage with `rstest-bdd`.
- [ ] Stage G: configuration layer-precedence and end-to-end tests.
- [ ] Stage H: documentation.
- [ ] Stage I: final gate stack and roadmap update.

## Surprises and discoveries

- Discovery (planning): `docs/users-guide.md` line 85 currently states
  the operator override is tracked under roadmap "Step 2.6.4". The current
  roadmap places it at Step 2.6.3. Treat the existing text as stale and update
  it during Stage H.
- Discovery (planning): `docs/developers-guide.md` §8.2.3 already hints
  that Step 2.6.3 will either enable `MaskAndDeny` by default or surface a
  user-facing override. This plan does both: `MaskAndDeny` becomes the default
  for `agent.mode = "acp"`, and `acp.delegation` is the user-facing knob for
  relaxing it. The metric contract sketched in §8.2.3 remains follow-on work.
- Discovery (planning): the ACP specification models `clientCapabilities.fs`
  as a sub-object (`{ readTextFile, writeTextFile }`) and
  `clientCapabilities.terminal` as a single boolean. The override granularity
  chosen here (per-family: `fs` as a whole, `terminal` as a whole) matches the
  JSON-RPC method prefix axis and the existing `MethodDenylist` shape. Finer
  per-method overrides (`fs.readTextFile` vs `fs.writeTextFile`) remain a
  future refinement and are out of scope.
- Discovery (planning): `MethodDenylist` currently holds a
  `&'static [MethodFamily]`. The override space is *closed* (four variants), so
  the parametrization can ship as four static arrays plus one selector
  constructor. No `Cow`, `Vec`, or runtime allocation is needed; the
  zero-allocation policy path is preserved.
- Discovery (planning): JSON-RPC application error codes
  `-32099..=-32000` are explicitly available for application-defined errors per
  <https://www.jsonrpc.org/specification#error_object>. Reserving `-32002` for
  "override required" is spec-conformant.
- Discovery (planning): the prior-art pattern across Podman, Docker,
  MCP gateway proxies, Codex, and Claude Code is overwhelmingly *default-off,
  named explicitly, surfaced at startup, logged as a policy event*. The
  audit-line emission at session start follows this precedent.

## Decision log

- Decision: choose configuration shape
  `acp.delegation = "disabled"|"allow_fs"|"allow_terminal"|"allow_all"` rather
  than two booleans or a per-method allowlist. Rationale: a single closed enum
  is testable, exhaustively cover-able in `rstest`, and easy to validate at
  config load. Two booleans (one per family) would invite drift and produce
  four implicit modes without explicit naming, repeating the mistake Step 2.6.2
  corrected for `CapabilityPolicy`. A per-method allowlist would require a
  schema for method names and is out of proportion to the current enforcement
  granularity. Date/Author: 2026-05-29 / planning lane.
- Decision: introduce a dedicated pure module
  `src/engine/connection/exec/acp_delegation.rs` for the selector and the
  `AcpCapabilityMask`, rather than extending `acp_policy.rs`. Rationale: keeps
  each module focused on a single concern (decision vs. denylist), respects the
  400-line ceiling, and matches the hex-arch guidance to keep pure decision
  logic separable from the static-data policy module. Date/Author: 2026-05-29 /
  planning lane.
- Decision: emit two distinct audit `tracing::warn!` lines for
  non-default overrides: one at config-load time (after semantic validation
  succeeds) and one at protocol session start. Rationale: the load-time line
  surfaces the change as early as a CI pipeline can observe it (mirroring the
  ADR 008 trust-boundary- relaxation pattern for hook image overrides); the
  session-start line ties each session to the override that governed it for
  incident forensics. Emitting only one of the two would leave a gap.
  Date/Author: 2026-05-29 / planning lane.
- Decision: keep the override surface on `HostArgs` only and not on
  `RunArgs`. Rationale: the validation rule makes `acp.delegation` illegal when
  `agent.mode != "acp"`, and `RunArgs` produces `agent.mode = "podbot"`.
  Surfacing the flag on `RunArgs` would mislead operators into thinking it
  applies to interactive runs. Date/Author: 2026-05-29 / planning lane.
- Decision: reserve `-32002 METHOD_OVERRIDE_REQUIRED_ERROR_CODE` as a
  documented constant but do not synthesize the new error response in this
  Step. Rationale: the reserved code is referenced by both the design doc and
  the existing `acp_policy.rs` documentation; codifying the constant now
  removes future renumbering risk and signals intent. The actual synthesised
  "override required" response shape is a separate, larger decision (do we
  route the operator to a documentation URL? do we surface the override token?)
  and belongs in a follow-on Step. Date/Author: 2026-05-29 / planning lane.
- Decision: keep `proptest` (already a workspace dev-dependency) as the
  rigour vehicle for the family-narrowing invariant, rather than reaching for
  `kani` or `verus`. Rationale: the invariant ranges over a small, closed enum
  cross a string method name; proptest's shrink is sufficient and avoids the
  toolchain cost of a bounded model checker or deductive proof. Should a later
  Step introduce a per-method override surface, `kani` becomes appropriate.
  Date/Author: 2026-05-29 / planning lane.

## Outcomes and retrospective

To be filled in after Stage I. Capture:

- whether `MaskAndDeny` becoming the default for `agent.mode = "acp"`
  produced any compatibility surprises in the existing test suites;
- whether the audit-event emissions integrated cleanly with the
  existing `tracing` subscriber configuration;
- whether the per-family `MethodDenylist` parametrization compelled
  any further refactoring of `acp_policy.rs`;
- whether CodeRabbit raised any concerns that the planning lane could
  have anticipated.

## Interfaces and dependencies

In `src/config/acp.rs`:

```rust,no_run
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcpDelegationOverride {
    #[default]
    Disabled,
    AllowFs,
    AllowTerminal,
    AllowAll,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcpConfig {
    pub delegation: AcpDelegationOverride,
}

impl AcpConfig {
    pub const fn is_default(&self) -> bool {
        matches!(self.delegation, AcpDelegationOverride::Disabled)
    }
}
```

In `src/config/types.rs`:

```rust,no_run
pub struct AppConfig {
    // existing fields ...
    #[serde(default)]
    pub acp: crate::config::AcpConfig,
}
```

In `src/engine/connection/exec/acp_delegation.rs`:

```rust,no_run
use crate::config::{AcpDelegationOverride, AgentMode};
use super::acp_policy::MethodDenylist;
use super::session::CapabilityPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AcpCapabilityMask {
    pub(super) strip_terminal: bool,
    pub(super) strip_fs: bool,
}

impl AcpCapabilityMask {
    pub(super) const fn is_relaxed(self) -> bool {
        !(self.strip_terminal && self.strip_fs)
    }

    pub(super) fn allows(self, method: &str) -> bool {
        let allowed_terminal = !self.strip_terminal
            && method.strip_prefix("terminal/").is_some_and(|r| !r.is_empty());
        let allowed_fs = !self.strip_fs
            && method.strip_prefix("fs/").is_some_and(|r| !r.is_empty());
        allowed_terminal || allowed_fs
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct AcpSessionPolicy {
    pub(super) policy: CapabilityPolicy,
    pub(super) denylist: MethodDenylist,
    pub(super) mask: AcpCapabilityMask,
}

pub(super) fn select_acp_session_policy(
    mode: AgentMode,
    override_: AcpDelegationOverride,
) -> AcpSessionPolicy;
```

In `src/engine/connection/exec/acp_policy.rs`, additions:

```rust,no_run
pub(crate) const METHOD_OVERRIDE_REQUIRED_ERROR_CODE: i64 = -32_002;

static FAMILIES_FULL: &[MethodFamily] = DEFAULT_BLOCKED_FAMILIES;
static FAMILIES_TERMINAL_ONLY: &[MethodFamily] =
    &[MethodFamily { prefix: "terminal/" }];
static FAMILIES_FS_ONLY: &[MethodFamily] = &[MethodFamily { prefix: "fs/" }];
static FAMILIES_EMPTY: &[MethodFamily] = &[];

impl MethodDenylist {
    pub(crate) const fn for_override(
        override_: crate::config::AcpDelegationOverride,
    ) -> Self {
        match override_ {
            AcpDelegationOverride::Disabled => Self::new(FAMILIES_FULL),
            AcpDelegationOverride::AllowFs => Self::new(FAMILIES_TERMINAL_ONLY),
            AcpDelegationOverride::AllowTerminal => Self::new(FAMILIES_FS_ONLY),
            AcpDelegationOverride::AllowAll => Self::new(FAMILIES_EMPTY),
        }
    }
}
```

In `src/engine/connection/exec/acp_helpers.rs`, the parametrized masker
signature:

```rust,no_run
pub(super) fn mask_acp_initialize_frame(
    frame: &[u8],
    mask: AcpCapabilityMask,
) -> Vec<u8>;
```

In `src/engine/connection/exec/session.rs`:

```rust,no_run
pub(crate) struct ExecSessionOptions {
    disable_protocol_stdin_forwarding: bool,
    capability_policy: CapabilityPolicy,
    denylist: MethodDenylist,
    mask: AcpCapabilityMask,
}

impl ExecSessionOptions {
    pub(crate) const fn with_acp_session_policy(
        mut self,
        policy: AcpSessionPolicy,
    ) -> Self {
        self.capability_policy = policy.policy;
        self.denylist = policy.denylist;
        self.mask = policy.mask;
        self
    }
}

pub(crate) fn build_exec_session_options(config: &AppConfig)
    -> ExecSessionOptions;
```

In `src/cli/mod.rs`:

```rust,no_run
pub struct HostArgs {
    pub agent: Option<AgentKindArg>,
    pub mode: Option<AgentModeArg>,
    /// Explicit ACP delegation override.
    #[arg(long = "acp-delegation", value_enum)]
    pub acp_delegation: Option<AcpDelegationOverrideArg>,
}
```

with `AcpDelegationOverrideArg` mirroring the configuration enum's tokens
through the `clap::ValueEnum` derive.

No new external dependencies are introduced. `proptest`, `serde`, `serde_json`,
`tracing`, and `tracing-subscriber` are already in the workspace; `clap` is
already used by `src/cli/mod.rs`. The audit-line test reuses the
`tracing_subscriber::fmt::MakeWriter` plus `tracing::subscriber::with_default`
pattern from `src/bin_tests/observability_helpers.rs`.

## Signposts

- Skills:
  - `execplans` — this skill, governing the document.
  - `hexagonal-architecture` — boundary discipline between the
    selector (pure) and the audit/I/O adapters.
  - `rust-router` and the routed sub-skills `rust-types-and-apis`,
    `rust-errors`, and `domain-cli-and-daemons` — for the typestate
    on `AcpDelegationOverride`, the configuration error shape, and
    the operator-facing CLI surface.
  - `logisphere-design-review` — run before Stage D to stress-test the
    selector's structure and contracts.
  - `code-review` — request after each milestone before the CodeRabbit
    gate.
  - `commit-message` — for each commit at a Stage gate.
- Documentation:
  - `docs/podbot-design.md` §Execution flow (target for Stage H).
  - `docs/podbot-roadmap.md` §2.6 (target for Stage I).
  - `docs/users-guide.md` (operator behaviour and precedence example).
  - `docs/developers-guide.md` §8.2 (internal architecture).
  - `docs/adr-006-define-the-validate-surface-and-capability-disposition-model.md`
    (capability disposition shifts under override).
  - `docs/adr-008-define-secrets-and-trust-boundaries-for-hooks-prompts-and-validation.md`
    §"Operator override" (precedent).
  - `docs/corbusier-conformance-design-for-agents-mcp-wires-and-hooks.md`
    §Security and trust boundary changes / ACP delegation.
  - `docs/rust-testing-with-rstest-fixtures.md` (fixture style for
    Stage B–G unit tests).
  - `docs/rstest-bdd-users-guide.md` (binding style for Stage F).
  - `docs/rust-doctest-dry-guide.md` (doctest discipline for new
    public items).
  - `docs/reliable-testing-in-rust-via-dependency-injection.md` (when
    threading the audit emitter through tests).
  - `docs/ortho-config-users-guide.md` (Stage G layer precedence).
  - `docs/complexity-antipatterns-and-refactoring-strategies.md` (if
    the selector grows beyond a single match).

## Revision note

- 2026-05-29: initial draft. Establishes scope, constraints,
  tolerances, risks, the nine Stages, the interface sketches, and the
  signposts. No work has been done yet beyond planning.
