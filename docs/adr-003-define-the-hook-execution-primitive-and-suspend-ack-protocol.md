# Architectural decision record (ADR) 003: Define the hook execution primitive and suspend-ack protocol

## Status

Proposed.

## Date

2026-03-16.

## Context and problem statement

Podbot's roadmap anticipates hooks as a governance mechanism for agent
sessions, but no current specification defines what a hook is, how it executes,
or how the orchestrator interacts with hook lifecycle. The Podbot design
document does not mention hooks at all, making this clean clay rather than wet
concrete.[^1]

Hooks are a prerequisite for orchestrator-governed workflows such as pre-commit
review, pre-merge validation, and policy-gated tool invocation. Before any hook
implementation lands, this ADR must lock down:

- Hook artefact types and execution model.
- Workspace access modes for hook runners.
- Event payloads, timeouts, and exit-code semantics.
- The suspend-and-acknowledge protocol between Podbot and the orchestrator.
- stdout/stderr capture rules.
- Trust boundary separation from Model Context Protocol (MCP) helper-container
  `RepoAccess`.

This ADR depends on ADR 002 (hosted session API) because hook events and
acknowledgements flow through the `SessionEvent` stream and session handle
methods defined there.

## Decision drivers

- Hooks must not violate Podbot's stdout purity guarantee for hosted
  sessions.[^1]
- Hook execution must be governable: the orchestrator decides whether a hook
  proceeds, and the agent session suspends until that decision arrives.
- Hook workspace access is a distinct trust boundary from MCP helper-container
  `RepoAccess` (defined in the MCP server hosting design[^2]). They govern
  different concerns and must not be conflated.
- The initial design should be deliberately simple. Additional states (such as
  `Skip`) should only be added when a concrete use case demands them.
- Hook artefact execution must be deterministic and auditable.

## Requirements

### Functional requirements

- The orchestrator can subscribe hooks at session launch by including hook
  subscriptions in the launch request (see ADR 007).
- Podbot emits a `HookTriggered` event when a subscribed trigger fires.
- Podbot suspends the agent session upon emitting `HookTriggered` and waits
  for an orchestrator acknowledgement before proceeding.
- The orchestrator responds with `Continue` (execute the hook and resume the
  agent) or `Abort` (skip the hook and terminate or roll back the triggering
  action).
- Podbot executes the hook artefact if continued, captures exit code and
  output, emits `HookCompleted`, and resumes agent execution.
- Hook timeout expiry without acknowledgement produces a deterministic
  outcome (session abort, not silent continuation).

### Technical requirements

- Hook events and acknowledgements use the `SessionEvent` stream and session
  handle methods from ADR 002.
- Hook artefact execution happens in an isolated context (container or
  subprocess), not in the agent container's process namespace.
- Hook stdout and stderr are captured separately and made available in the
  `HookCompleted` event, never mixed into the agent protocol stream.
- Hook workspace access is governed by a dedicated `HookWorkspaceAccess`
  enum, not by reusing `RepoAccess` from MCP wiring.

## Hook artefact model

A hook artefact is the executable unit that Podbot runs when a hook is
continued. The initial model supports two artefact kinds:

- **Inline script:** A single executable file, identified by path relative
  to the session's staged artefact area (see ADR 007). The script runs in the
  hook execution context with `#!/bin/sh` semantics unless the shebang
  specifies otherwise.
- **Container image:** An Open Container Initiative (OCI) image reference
  (digest-pinned for non-inline hooks; see ADR 008) that Podbot pulls and runs
  as a short-lived container.

Future artefact kinds (tar archives, WASM modules) may be added by extending
the enum without breaking existing consumers.

```rust,no_run
#[non_exhaustive]
pub enum HookArtefactKind {
    InlineScript { path: String },
    ContainerImage { image: String, entrypoint: Option<Vec<String>> },
}

pub struct HookArtefact {
    pub kind: HookArtefactKind,
    pub digest: Option<String>,
}
```

## Hook subscription model

A hook subscription binds a named hook to one or more trigger points, with
explicit workspace access, environment policy, and timeout.

```rust,no_run
pub struct HookSubscription {
    pub hook_name: String,
    pub artefact: HookArtefact,
    pub triggers: Vec<HookTrigger>,
    pub workspace_access: HookWorkspaceAccess,
    pub env_allowlist: Vec<String>,
    pub timeout_seconds: u64,
}

#[non_exhaustive]
pub enum HookTrigger {
    PreTurn,
    PostTurn,
    PreCommit,
    PostCommit,
}

pub enum HookWorkspaceAccess {
    None,
    ReadOnly,
    ReadWrite,
}
```

`HookTrigger` is `#[non_exhaustive]` to allow new trigger points without semver
breakage. The initial set is deliberately small; trigger points such as
`PreToolCall`, `PostToolCall`, `PreMerge`, and `PreDeploy` are deferred until
concrete use cases justify them.

`HookWorkspaceAccess` is intentionally a separate type from MCP
`RepoAccess`.[^2] Although both control volume mounting, they govern different
trust boundaries:

- `RepoAccess` governs whether an MCP helper container can see the agent's
  repository volume. The trust question is: "should this tool server read or
  modify the workspace?"
- `HookWorkspaceAccess` governs whether a governance hook can see or modify
  the workspace. The trust question is: "should this policy check inspect or
  alter the working tree?"

Conflating them would allow a change to MCP access policy to silently alter
hook access policy, or vice versa.

## Suspend-and-acknowledge state machine

The hook lifecycle follows a simple state machine with four states:

```mermaid
stateDiagram-v2
    [*] --> Idle
    Idle --> Triggered : trigger fires\nSessionEvent::HookTriggered emitted\nAgent session suspended
    Triggered --> Executing : orchestrator ack (Continue)
    Triggered --> Aborted : orchestrator ack (Abort)
    Aborted --> [*] : SessionEvent::HookAborted emitted\nTriggering action rolled back\nAgent resumes or session ends
    Executing --> Completed : hook exits\nSessionEvent::HookCompleted emitted\nAgent execution resumes
```

### Event types

```rust,no_run
pub struct HookTriggeredEvent {
    pub invocation_id: HookInvocationId,
    pub hook_name: String,
    pub trigger: HookTrigger,
    pub session_id: SessionId,
}

pub enum HookDecision {
    Continue,
    Abort { reason: Option<String> },
}

pub struct HookCompletedEvent {
    pub invocation_id: HookInvocationId,
    pub hook_name: String,
    pub exit_code: i32,
    pub stdout_capture: Vec<u8>,
    pub stderr_capture: Vec<u8>,
    pub duration_ms: u64,
}

pub struct HookAbortedEvent {
    pub invocation_id: HookInvocationId,
    pub hook_name: String,
    pub reason: Option<String>,
}
```

### Acknowledgement protocol

1. Podbot detects a subscribed trigger and emits `HookTriggered` via the
   session event stream.
2. Podbot suspends the agent session. No agent turns execute while a hook is
   pending.
3. The orchestrator calls
   `session.acknowledge_hook(invocation_id, decision)` (ADR 002).
4. If `Continue`: Podbot executes the hook artefact, captures output, emits
   `HookCompleted`, and resumes agent execution.
5. If `Abort`: Podbot emits `HookAborted`, rolls back or cancels the
   triggering action where possible, and resumes agent execution (or ends the
   session if the abort is terminal).

### Timeout semantics

Each hook subscription specifies a `timeout_seconds` value. If the orchestrator
does not acknowledge within this window:

- Podbot treats the timeout as an implicit `Abort` with reason
  `"acknowledgement timeout"`.
- Podbot emits `HookAborted` with the timeout reason.
- The agent session resumes or terminates according to the trigger's
  abort-on-timeout policy (default: terminate the triggering action, not the
  entire session).

Timeouts must be enforced by Podbot, not delegated to the orchestrator, because
the orchestrator may itself be unresponsive.

### Exit-code semantics

When a hook artefact executes:

- Exit code 0: success. The triggering action proceeds.
- Non-zero exit code: failure. Podbot reports the failure in
  `HookCompleted` but does not automatically abort the session. The
  orchestrator decides how to handle hook failures based on its own policy.

This keeps Podbot's role as a runtime executor, not a policy engine. The
orchestrator owns the decision of whether a failed hook should block agent
progress.

## stdout and stderr capture

Hook artefact stdout and stderr are captured into bounded byte buffers
(default: 1 MiB each). Captured output is included in `HookCompletedEvent`.

- Hook stdout is **never** forwarded to the hosted session's stdout. This
  preserves protocol purity.[^1]
- Hook stderr is **never** forwarded to the hosted session's stderr. Hook
  diagnostics travel via the session event stream as structured events, not as
  raw text.
- If a hook produces output exceeding the capture limit, Podbot truncates
  and records a diagnostic noting the truncation.

## Goals and non-goals

- Goals:
  - Define the hook artefact model, subscription model, and
    suspend-acknowledge state machine.
  - Separate hook workspace access from MCP `RepoAccess`.
  - Keep the initial state machine simple (no `Skip` state).
  - Ensure hooks cannot contaminate the agent protocol stream.
- Non-goals:
  - Define hook subscription policy or approval workflows (orchestrator
    concern).
  - Define the full trigger taxonomy (kept deliberately small; extend via
    `#[non_exhaustive]`).
  - Define hook container image approval or pinning policy (see ADR 008).

## Known risks and limitations

- Suspending the agent session during hook acknowledgement adds latency to
  every hooked trigger. Mitigation: hooks should be subscribed sparingly, and
  timeouts should be short (seconds, not minutes).
- The capture buffer limit (1 MiB) may be insufficient for hooks that produce
  large diagnostic output. Mitigation: hooks should write large output to the
  workspace filesystem rather than stdout, and reference the file path in their
  exit output.
- `HookDecision` deliberately omits a `Skip` variant. If a use case emerges
  where the orchestrator wants to skip a hook without aborting the triggering
  action, a `Skip` variant can be added to the `#[non_exhaustive]` enum without
  semver breakage.

## Outstanding decisions

- Whether hook execution should reuse the agent container's inner Podman
  runtime or run in a sibling container managed by the host-side engine.
  Recommendation: sibling container, to maintain isolation between hook
  execution and agent execution.
- Whether multiple hooks on the same trigger execute sequentially or in
  parallel. Recommendation: sequential in subscription order, to provide
  deterministic execution and allow earlier hooks to influence later ones.
- Recovery semantics for hooks pending at process restart (see ADR 009).

______________________________________________________________________

[^1]: Podbot design document. See `docs/podbot-design.md`, "Dual delivery
    model" and "Execution flow" sections.

[^2]: MCP server hosting design. See `docs/mcp-server-hosting-design.md`,
    `RepoAccess` enum and helper-container trust boundary.
