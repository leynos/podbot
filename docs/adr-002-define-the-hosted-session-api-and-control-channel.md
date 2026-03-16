# Architectural decision record (ADR) 002: Define the hosted session API and control channel

## Status

Proposed.

## Date

2026-03-16.

## Context and problem statement

`podbot host` is committed to non-TTY protocol hosting with stdout as a pure
bridge of container stdout and all diagnostics on stderr.[^1] Once hooks (ADR
003), validation events, and lifecycle signals are added, the library surface
needs an explicit channel for control-plane communication that does not
contaminate the protocol stream.

Today the Podbot library returns typed results from orchestration functions,
but there is no long-lived session handle that separates protocol input/output
(IO) from control events. Without such a handle, hook notifications, validation
warnings, wire lifecycle events, and session diagnostics have nowhere to go
except stderr text — which is unstructured, unparseable, and invisible to
library embedders that do not scrape process output.

This ADR must choose the shape of the hosted session API before hooks land,
because the hook suspend-and-acknowledge protocol (ADR 003) requires a
bidirectional control channel between Podbot and the orchestrator.

## Decision drivers

- Library functions must return typed results and must not write directly to
  stdout or stderr.[^1]
- `podbot host` must preserve stdout purity: protocol bytes only.[^1]
- An embedding orchestrator (such as Corbusier) needs structured, typed
  control events to coordinate hooks, monitor lifecycle, and audit session
  behaviour.
- The CLI adapter must remain a thin layer; session complexity belongs in the
  library.
- The control channel must support the hook suspend/acknowledge protocol
  defined in ADR 003 without requiring the orchestrator to parse stderr.

## Requirements

### Functional requirements

- The library exposes a session handle that provides separate access to
  protocol IO and control events.
- Control events are typed and structured, not free-text log lines.
- The session handle supports bidirectional control: Podbot emits events, and
  the orchestrator sends commands (such as hook acknowledgements).
- The CLI adapter can consume the same handle, rendering control events to
  stderr as human-readable diagnostics.

### Technical requirements

- Protocol IO (stdin/stdout bridge) is separated from the control event
  stream at the type level, not merely by convention.
- Control events are delivered via an async `Stream` that the orchestrator
  polls independently of protocol IO.
- Orchestrator commands (hook acks, session stop) are sent via typed methods
  on the session handle, not by writing to stdin.
- The session handle is `Send` and can be held across `await` points.

## Options considered

### Option A: Handle-based API with async event stream

Return a `HostedSession` from the launch function. The handle exposes:

- Protocol IO: async readers/writers for stdin and stdout bridging.
- Control events: `Stream<Item = SessionEvent>` for lifecycle, hook, and
  diagnostic events.
- Commands: typed methods such as `acknowledge_hook(invocation_id, decision)`.

The orchestrator owns the polling loop and decides how to process events.

```rust,no_run
let session = podbot::launch::host(plan).await?;

// Protocol IO — hand to the upstream protocol client.
let (proto_stdin, proto_stdout) = session.protocol_io();

// Control events — poll in a separate task.
let mut events = session.events();
while let Some(event) = events.next().await {
    match event {
        SessionEvent::HookTriggered { invocation_id, .. } => {
            session.acknowledge_hook(invocation_id, HookDecision::Continue).await?;
        }
        SessionEvent::Lifecycle(msg) => { /* log or audit */ }
        _ => {}
    }
}
```

Consequences: clean separation, composable, testable with mock streams.
Slightly more complex than a callback API but avoids inversion-of-control
problems.

### Option B: Callback-based API

The orchestrator registers closures or trait implementations for each event
category at session creation time. Podbot invokes callbacks synchronously or on
a dedicated executor.

Consequences: familiar pattern, but callbacks complicate lifetime management in
async Rust, make testing harder (mock closures are less ergonomic than mock
streams), and couple the orchestrator's execution model to Podbot's internal
scheduling.

### Option C: Secondary Unix domain socket

Podbot opens a Unix domain socket (UDS) alongside the protocol session. Control
events and commands flow as JSON-RPC messages over this socket.

Consequences: transport-agnostic and language-neutral, but introduces
serialisation overhead, a second connection to manage, and a filesystem
artefact that needs cleanup. Unnecessary complexity when the primary consumer
is an in-process Rust embedder.

| Topic                    | Option A (handle)    | Option B (callbacks) | Option C (UDS)         |
| ------------------------ | -------------------- | -------------------- | ---------------------- |
| Async Rust ergonomics    | Native streams       | Lifetime friction    | Serialisation overhead |
| Protocol IO separation   | Type-level           | Convention-level     | Transport-level        |
| Testability              | Mock stream + handle | Mock closures        | Mock socket server     |
| CLI adapter complexity   | Low (poll + print)   | Low (print in CB)    | Medium (socket client) |
| Cross-language embedding | Not supported        | Not supported        | Supported              |
| Hook ack round-trip      | Method call          | Return value in CB   | JSON-RPC exchange      |

_Table 1: Comparison of session API strategies._

## Decision outcome / proposed direction

**Option A: Handle-based API with async event stream.**

The library exposes a `HostedSession` type from `podbot::session` (see ADR 001
for module boundary). The handle provides:

- `protocol_io() -> (AsyncRead, AsyncWrite)` — the raw protocol byte streams
  for the hosted agent's stdin and stdout. The orchestrator or CLI adapter
  bridges these to the upstream client.
- `events() -> impl Stream<Item = SessionEvent>` — a bounded async channel
  delivering typed control events. Events include lifecycle transitions, hook
  triggers, validation diagnostics, and wire status changes.
- `acknowledge_hook(invocation_id, HookDecision) -> Result<(), SessionError>`
  — sends a typed hook decision back to the session (see ADR 003).
- `stop() -> Result<(), SessionError>` — requests graceful session shutdown.

`SessionEvent` is a non-exhaustive enum:

```rust,no_run
#[non_exhaustive]
pub enum SessionEvent {
    Lifecycle(LifecycleEvent),
    HookTriggered(HookTriggeredEvent),
    HookCompleted(HookCompletedEvent),
    Diagnostic(DiagnosticEvent),
    WireStatus(WireStatusEvent),
}
```

The `#[non_exhaustive]` annotation allows new event categories to be added
without a semver-breaking change, which is important for a surface that will
grow as hooks, validation, and MCP wiring mature.

### CLI adapter behaviour

The CLI adapter for `podbot host`:

1. Bridges `protocol_io()` to process stdin and stdout.
2. Spawns a task that polls `events()` and renders each event to stderr as
   structured human-readable text.
3. Does not acknowledge hooks automatically; in CLI mode, hooks that require
   acknowledgement cause the session to emit a diagnostic and time out
   according to the configured policy. (Interactive hook acknowledgement via
   CLI is a potential future enhancement but not part of this ADR.)

### Stderr contract

Podbot's existing stderr diagnostic promise is preserved. The library never
writes directly to stderr; the CLI adapter is solely responsible for stderr
formatting. Library embedders receive events via the typed stream and decide
independently how (or whether) to render them.

## Goals and non-goals

- Goals:
  - Provide a typed, async session handle that cleanly separates protocol IO
    from control events.
  - Enable the hook suspend/acknowledge protocol (ADR 003) without
    contaminating stdout.
  - Keep the CLI adapter thin: bridge IO, render events, exit.
- Non-goals:
  - Define the full event taxonomy (individual ADRs add their own event
    types).
  - Provide cross-language bindings (Option C's UDS approach could be
    revisited later if a non-Rust consumer emerges).
  - Define reconnection or recovery semantics (see ADR 009).

## Known risks and limitations

- The bounded event channel introduces backpressure. If the orchestrator
  falls behind, Podbot must decide between dropping events (lossy) or blocking
  session progress (lossless). Recommendation: use a bounded channel with a
  reasonable capacity (for example, 256 events) and log a warning on slow
  consumers, but do not block the protocol IO path.
- `AsyncRead` / `AsyncWrite` for protocol IO ties the surface to Tokio's
  async model. This is acceptable because Podbot already depends on Tokio as
  its async runtime.

## Outstanding decisions

- Exact channel capacity for the event stream.
- Whether `SessionEvent` should carry a monotonic event ID from session start
  (see ADR 009 for ordering and recovery).
- Whether the handle should expose a `stderr() -> impl AsyncRead` for raw
  container stderr, or whether all container stderr should be parsed into
  `DiagnosticEvent` variants.

______________________________________________________________________

[^1]: Podbot design document. See `docs/podbot-design.md`, "Dual delivery
    model" and "Execution flow" sections.
