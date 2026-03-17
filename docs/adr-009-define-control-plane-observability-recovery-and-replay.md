# Architectural decision record (ADR) 009: Define control plane observability, recovery, and replay

## Status

Proposed.

## Date

2026-03-16.

## Context and problem statement

Once hooks can suspend an agent session until the orchestrator responds (ADR
003), recovery semantics stop being optional. If the Podbot host process
crashes or restarts while a hook acknowledgement is pending, the system must
define what happens: does the session resume, does the hook re-trigger, or does
the session abort?

Additionally, `podbot host` already promises stderr-only diagnostics and
protocol-pure stdout.[^1] That promise must survive failure paths, reconnection
attempts, and diagnostic capture — not only sunny-day flows.

This ADR defines session-scoped event identifiers, ordering guarantees,
reconnect behaviour, pending-hook recovery after process restart, duplicate
delivery handling, and diagnostic capture that does not contaminate stdout.

## Decision drivers

- The hook suspend/acknowledge protocol (ADR 003) creates a state where both
  Podbot and the orchestrator hold in-flight obligations. Recovery must resolve
  that state deterministically.
- The session event stream (ADR 002) must provide enough metadata for the
  orchestrator to detect gaps, duplicates, and ordering anomalies.
- Stdout purity must survive crashes. A half-written diagnostic must never
  appear on stdout, even during an abnormal exit.[^1]
- Podbot is both a CLI binary and an embeddable library. Recovery semantics
  must work for both delivery modes, but the library surface is the normative
  API.[^1]

## Requirements

### Functional requirements

- Every session event carries a monotonic event identifier that is unique
  within the session and strictly ordered.
- The orchestrator can detect event gaps by comparing consecutive event
  identifiers.
- Recovery after Podbot process restart resolves pending hooks
  deterministically.
- Duplicate event delivery is detectable by the orchestrator using event
  identifiers.
- Diagnostic and lifecycle logs are captured without contaminating stdout.

### Technical requirements

- Event identifiers are unsigned 64-bit integers, starting at 1,
  incremented by 1 for each event emitted within a session.
- Session state sufficient for recovery is persisted to a durable session
  state file within the session runtime directory.
- Stdout writes are performed exclusively by the protocol IO bridge
  (ADR 002), never by diagnostic, event, or recovery code paths.
- The session state file is cleaned up during normal teardown (ADR 007)
  and left in place after abnormal exit for recovery inspection.

## Event identification and ordering

### Event envelope

Every `SessionEvent` (ADR 002) is wrapped in an envelope that adds ordering
metadata:

```rust,no_run
pub struct SessionEventEnvelope {
    pub event_id: u64,
    pub session_id: SessionId,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub event: SessionEvent,
}
```

- `event_id`: Monotonically increasing within the session. Starts at 1.
  Never reused. Gaps indicate lost events (for example, after a crash).
- `session_id`: Unique identifier for the session, generated at launch.
- `timestamp`: Wall-clock time of event emission.

### Ordering guarantees

- Events are emitted in the order they occur within the session.
- The event stream is single-producer (Podbot session loop), so no
  cross-task ordering ambiguity exists.
- If the orchestrator receives event N+2 without receiving N+1, it knows
  at least one event was lost.
- Podbot does not guarantee exactly-once delivery. It guarantees
  at-most-once delivery per event ID in the normal case, and at-least-once
  delivery after recovery (see below).

## Recovery model

### Session state persistence

Podbot persists minimal session state to a file at
`$XDG_RUNTIME_DIR/podbot/<session_id>/state.json`. The state file contains:

```rust,no_run
pub struct PersistedSessionState {
    pub session_id: SessionId,
    pub last_emitted_event_id: u64,
    pub pending_hooks: Vec<PendingHookState>,
    pub agent_status: AgentStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub struct PendingHookState {
    pub invocation_id: HookInvocationId,
    pub hook_name: String,
    pub trigger: HookTrigger,
    pub suspended_at: chrono::DateTime<chrono::Utc>,
    pub timeout_seconds: u64,
}

pub enum AgentStatus {
    Running,
    Suspended,
    Stopped,
}
```

State is written atomically (write to temporary file, then rename) after each
significant state transition:

- Hook triggered (agent suspended).
- Hook acknowledged.
- Hook completed.
- Session stopped.

### Recovery scenarios

#### Scenario 1: Crash with no pending hooks

The session state file shows no pending hooks. Recovery action:

- Log the incomplete session as abandoned.
- Clean up the container (if still running) and staging area.
- No events to re-emit.

The orchestrator detects the session ended by observing the event stream close
or by polling session status.

#### Scenario 2: Crash with pending hook (agent suspended)

The session state file shows one or more pending hooks with the agent in
`Suspended` status. Recovery action:

1. Check whether the hook timeout has elapsed.
2. If timeout elapsed: treat as implicit `Abort` (same as timeout during
   normal operation, per ADR 003). Emit `HookAborted` with reason
   `"recovery: acknowledgement timeout"`. Resume or stop the agent according to
   abort policy.
3. If timeout has not elapsed: re-emit `HookTriggered` for each pending
   hook with the same `invocation_id` but a new `event_id`. The orchestrator
   can detect the duplicate `invocation_id` and either re-acknowledge or
   recognize its previous acknowledgement was lost.

Re-emitting `HookTriggered` with the original `invocation_id` but a new
`event_id` ensures:

- The orchestrator can correlate the re-emitted trigger with its previous
  state.
- The event ordering sequence remains monotonic (no reused `event_id`
  values).

#### Scenario 3: Crash during hook execution

The session state file shows a hook in acknowledged/executing state. Recovery
action:

- Treat the hook execution as failed with exit code -1 (interrupted).
- Emit `HookCompleted` with `exit_code: -1` and empty captures.
- Resume agent execution.

The orchestrator can distinguish an interrupted hook from a normal failure by
the exit code and the gap in event IDs.

### Recovery initiation

Recovery is initiated by the embedding host (orchestrator or CLI). Podbot does
not automatically restart sessions. The library surface exposes:

```rust,no_run
pub async fn recover_session(
    session_id: SessionId,
) -> Result<RecoveryOutcome, RecoveryError> { /* ... */ }

pub enum RecoveryOutcome {
    Resumed(HostedSession),
    Abandoned { reason: String },
    NotFound,
}
```

The CLI adapter can implement a `podbot recover <session-id>` subcommand that
calls this function and reconnects protocol IO if the session resumes.

## Duplicate delivery handling

The orchestrator must handle duplicate `HookTriggered` events (same
`invocation_id`, different `event_id`) idempotently:

- If the orchestrator has already acknowledged the `invocation_id`, it
  should re-send the same acknowledgement.
- If the orchestrator has not yet decided, it can proceed with its normal
  decision logic.
- Podbot accepts duplicate acknowledgements for the same `invocation_id`
  idempotently: the second acknowledgement is logged but has no effect.

## Stdout purity under failure

### Normal operation

Only the protocol IO bridge (ADR 002) writes to process stdout. All other
output paths (events, diagnostics, lifecycle) are routed through the session
event stream or stderr (CLI adapter only).

### Crash paths

- Panic handlers must not write to stdout. Podbot should install a panic
  hook that writes to stderr only.
- Signal handlers (SIGTERM, SIGINT) must flush the protocol IO bridge
  cleanly before exiting, but must not emit non-protocol bytes to stdout.
- Out-of-memory (OOM) kills and SIGKILL are uncontrollable; stdout purity
  cannot be guaranteed in these cases, but no Podbot code path initiates stdout
  writes that could be interrupted mid-byte.

### Diagnostic capture

Session diagnostics (container engine errors, hook failures, validation
warnings) are captured exclusively via the session event stream. The CLI
adapter renders them to stderr. At no point does any diagnostic code path
reference stdout or a stdout writer.

## Options considered

### Option A: Persistent session state with monotonic event IDs

Persist minimal session state to a durable file. Use monotonic event IDs for
ordering and gap detection. Re-emit pending hook triggers on recovery.

Consequences: deterministic recovery, auditable event history, small
persistence footprint. Requires atomic file writes on every state transition.

### Option B: Event log replay

Persist a complete event log. On recovery, replay the log to reconstruct
session state and resume from the last event.

Consequences: full audit trail, but significantly larger persistence footprint,
more complex replay logic, and risk of replaying side effects (such as hook
executions) unintentionally.

### Option C: No recovery; crash aborts the session

Podbot does not attempt recovery. A crash means the session is lost. The
orchestrator must start a new session from scratch.

Consequences: simplest implementation, but unacceptable for long-running
sessions or sessions with expensive workspace setup. The orchestrator has no
way to resume in-progress work.

| Topic                 | Option A (state file)   | Option B (event log)       | Option C (no recovery) |
| --------------------- | ----------------------- | -------------------------- | ---------------------- |
| Persistence footprint | Small (last state only) | Large (all events)         | None                   |
| Recovery determinism  | High                    | High                       | N/A                    |
| Implementation cost   | Medium                  | High                       | None                   |
| Side-effect safety    | Re-emit triggers only   | Risk of side-effect replay | N/A                    |
| Long-session support  | Yes                     | Yes                        | No                     |

_Table 1: Comparison of recovery strategies._

## Decision outcome / proposed direction

**Option A: Persistent session state with monotonic event IDs.**

This provides deterministic recovery for the critical pending-hook scenario
without the cost and complexity of full event log replay. The small state file
is cheap to write atomically on each transition, and the monotonic event IDs
give the orchestrator enough information to detect gaps and duplicates.

## Goals and non-goals

- Goals:
  - Define session-scoped event identification and ordering.
  - Define recovery semantics for pending hooks after process restart.
  - Define duplicate delivery handling for orchestrators.
  - Ensure stdout purity survives failure paths.
- Non-goals:
  - Define full session replay or time-travel debugging (Option B could be
    revisited as a future enhancement).
  - Define orchestrator-side recovery logic (the orchestrator decides how to
    handle `RecoveryOutcome`).
  - Define cross-session event correlation (session IDs are sufficient for
    orchestrator-side join operations).

## Known risks and limitations

- Atomic file writes add latency to every state transition. Mitigation: the
  state file is small (a few hundred bytes of JSON) and writes are infrequent
  (only on hook state transitions, not on every event emission).
- Recovery after a crash during state file write could produce a corrupt or
  truncated state file. Mitigation: atomic rename ensures the state file is
  either the old state or the new state, never a partial write.
- Re-emitting `HookTriggered` after recovery may cause the orchestrator to
  re-run its authorization logic. This is intentional: the orchestrator should
  confirm its decision after a disruption rather than assuming the pre-crash
  decision is still valid.

## Outstanding decisions

- Whether event IDs should be included in the session event stream for all
  events or only for events that the orchestrator needs for correlation (hook
  triggers, completions, aborts). Recommendation: include event IDs on all
  events for uniform ordering, even if the orchestrator ignores them for some
  event types.
- Whether the session state file should be human-readable JSON or a compact
  binary format. Recommendation: JSON, for debuggability and because the file
  is small enough that parse performance is irrelevant.
- Whether Podbot should provide a `podbot inspect <session-id>` CLI command
  that reads the state file and reports session status without attempting
  recovery. Recommendation: yes, as a diagnostic tool.

______________________________________________________________________

[^1]: Podbot design document. See `docs/podbot-design.md`, "Dual delivery
    model" and "Execution flow" sections.
