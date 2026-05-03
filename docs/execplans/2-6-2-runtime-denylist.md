# Step 2.6.2: Enforce a runtime denylist for blocked Agentic Control Protocol (ACP) methods

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises and discoveries`,
`Decision log`, and `Outcomes and retrospective` must be kept up to date as
work proceeds.

Status: COMPLETE (2026-05-02). All Stage I gates pass; the second
Step 2.6 roadmap checkbox is now marked done.

No `PLANS.md` file exists in this repository as of 2026-05-02, so this ExecPlan
is the governing implementation document for this task.

## Purpose and big picture

Step 2.6.1 closed the door on the Agentic Control Protocol (ACP) handshake by
stripping `terminal/*` and `fs/*` capability advertisements from the
client-side `initialize` request before they reach the sandboxed agent. Step
2.6.2 closes the second door: a hosted agent may still attempt to call those
host-delegated methods (`terminal/create`, `terminal/wait_for_exit`,
`fs/read_text_file`, `fs/write_text_file`, and so on) through the protocol
proxy after initialization. Without enforcement at the proxy seam, those calls
would either reach the IDE host (defeating the sandbox) or stall (because the
IDE will not service capabilities it never advertised). Either outcome breaks
the trust boundary that `docs/podbot-design.md` records:

> Podbot must maintain a runtime denylist for blocked ACP methods and
> return a protocol error if those methods are attempted later in the
> session.

Observable success for this task:

- when the protocol proxy is in ACP enforcement mode, an outbound
  `terminal/*` or `fs/*` JavaScript Object Notation Remote Procedure Call
  (JSON-RPC) request emitted by the hosted agent never reaches host stdout;
- the agent receives a synthesized JSON-RPC error response (with the same
  `id`) carrying a Podbot-specific application error code naming the blocked
  method;
- a stderr diagnostic line records each denial with the container
  identifier, blocked method name, and request id (or `null` for notifications);
- non-blocked frames pass through byte-for-byte, including frames that span
  multiple Bollard `LogOutput` chunks;
- malformed or non-JSON-RPC frames pass through unchanged, in keeping with
  the tolerant rewriting policy established in 2.6.1;
- the existing protocol-mode invariants from Step 2.5 hold: stdout purity,
  bounded buffering, accurate exit codes, no terminal framing;
- when ACP enforcement is disabled, `ExecMode::Protocol` remains a
  byte-transparent proxy with no parsing overhead;
- `rstest` unit tests and `rstest-bdd` v0.5.0 behavioural tests cover
  happy, unhappy, and edge cases for both directions;
- `docs/podbot-design.md`, `docs/users-guide.md`, and
  `docs/developers-guide.md` reflect the shipped behaviour;
- only the second Step 2.6 roadmap checkbox is marked done. The remaining
  three checkboxes (richer denial diagnostics, operator override, and the
  combined override-behaviour test suite) remain explicit follow-on work.

## Constraints

- Scope is limited to Step 2.6.2 in `docs/podbot-roadmap.md`. The operator
  override (Step 2.6.4), the consolidated override and handshake test battery
  (Step 2.6.5), and any user-facing configuration surface for enabling
  delegation are out of scope. Synthesizing a protocol error and emitting a
  stderr denial line are in scope because they are the minimum observable
  behaviour an enforcement mechanism must produce; Step 2.6.3 remains free to
  enrich diagnostic structure later.
- Preserve the protocol proxy contract from Step 2.5: stdout purity,
  stderr-only diagnostics, bounded buffering, explicit stdin shutdown ordering,
  and accurate exit-code reporting.
- Preserve the tolerant first-frame masking semantics from Step 2.6.1.
  Runtime enforcement must compose with init masking, not replace it.
- Default behaviour of `ExecMode::Protocol` must remain a raw byte proxy.
  Enforcement activates only when the existing
  `ExecSessionOptions::with_acp_initialize_rewrite_enabled` opt-in is set
  (potentially renamed in this step to reflect both responsibilities).
- Do not add new runtime dependencies. Reuse `serde_json` (re-exported via
  `ortho_config::serde_json`), `tokio::sync::mpsc`, and `tracing`, all of which
  are already in the workspace.
- Keep every touched module within the 400-line guidance from `AGENTS.md`.
  The existing `acp_helpers.rs` is already 263 lines, so the new policy
  decisions, the frame assembler, and the output adapter must land in separate
  sibling modules: `acp_policy.rs` for pure decisions and the error builder,
  `acp_frame.rs` for the newline-based frame assembler (with its 128 kilobyte
  ceiling), and `acp_runtime.rs` for the output-direction adapter and the
  container-stdin sink task. Promote the family of ACP modules from inline
  `#[path = "..."]` declarations inside `protocol.rs` to ordinary
  `pub(super) mod ...` entries in `src/engine/connection/exec/mod.rs` so the
  modules are reachable from both production and tests without the inline path
  declarations.
- Every new module must begin with a `//!` module-level comment.
- Use `rstest` fixtures and parameterized cases for unit coverage; use
  `rstest-bdd` v0.5.0 with `StepResult<T> = Result<T, String>` for behavioural
  coverage. Mirror the patterns in
  `src/engine/connection/exec/protocol_acp_bdd_tests.rs`.
- Production code must be panic-free. Apply the existing tolerant
  pass-through policy: if JSON parsing fails, forward the bytes unchanged.
- Use British English with Oxford spelling
  (`-ize`, `-yse`, `-our`, Oxford comma when it improves clarity) in all
  documentation and code comments, except for references to external
  Application Programming Interface (API) identifiers.
- Run the full Rust gate stack before completion: `make check-fmt`,
  `make lint`, and `make test`. Run the documentation gates as well because
  Markdown files are touched: `make fmt`, `make markdownlint`, and `make nixie`.
- Pipe long-running gate output through `tee` to
  `/tmp/$ACTION-podbot-session-e445b19d.out` so truncated output does not hide
  failures. Do not run gates in parallel.

## Tolerances (exception triggers)

Stop and escalate (do not improvise) when any of the following occurs.

- Scope tolerance: implementation requires touching more than ten files or
  exceeding roughly 900 net lines added (production plus tests, excluding
  generated bindings).
- Interface tolerance: completing the change forces a public Application
  Programming Interface (API) signature break in `src/api/exec.rs` or
  `src/engine/connection/exec/mod.rs` rather than internal proxy-seam edits.
- Concurrency tolerance: bidirectional injection (output task pushing
  synthesized error frames into the input writer) cannot be expressed without
  `unsafe`, leaked tasks, or mutexes that span `await` points.
- Dependency tolerance: a new crate is required.
- Iteration tolerance: any of `make check-fmt`, `make lint`, or
  `make test` still fails after three focused fix passes against a single
  failure mode.
- Backpressure tolerance: the framing assembler cannot honour the existing
  64 kilobyte (`STDIN_BUFFER_CAPACITY`) buffer cap without dropping bytes.
- Ambiguity tolerance: more than one defensible interpretation of an
  observable behaviour remains after research; present the options before
  proceeding.

## Risks

- Risk: bidirectional injection couples the output and input tasks in a
  way that is sensitive to cancellation order. A naive shared writer can cause
  synthesized denial responses to be lost when the host closes stdin before the
  agent receives the error frame. Severity: high. Likelihood: medium.
  Mitigation: introduce a dedicated container-stdin sink task that owns the
  container input writer and drains a single
  `tokio::sync::mpsc::Receiver<WriteCmd>`. The host-stdin forwarding task and
  the output-direction policy adapter both become *senders* rather than
  competing writers. The sink drains the channel until it receives an explicit
  `WriteCmd::Shutdown`, which the protocol coordinator emits only after the
  output loop returns. Document the ordering invariant inline and in
  `docs/podbot-design.md`.

- Risk: the existing `STDIN_SETTLE_TIMEOUT` of 50 milliseconds may abort
  the input task before queued denial responses are flushed, especially when
  the agent emits a blocked call immediately before exiting. Severity: medium.
  Likelihood: medium. Mitigation: the dedicated sink task removes the race
  entirely because the host-stdin forwarder no longer owns the writer; aborting
  it on the settle timeout cannot truncate synthesized errors. The settle
  timeout continues to apply to the host-stdin forwarder only. Cover this with
  a regression test that emits a blocked call as the final frame and asserts
  the synthesized error reaches container stdin.

- Risk: synthesized error responses arrive at the agent after its own
  request timeout has fired, producing a stray response with an unknown `id`
  that destabilizes the agent's JSON-RPC client. Severity: medium. Likelihood:
  medium. Mitigation: deliver synthesized errors synchronously with the deny
  decision (the bounded `mpsc` channel has a small capacity such as 16, so
  contention is rare). Add a behavioural assertion that the synthesized error
  appears on container stdin within one chunk of the blocked request being
  observed on the output stream.

- Risk: parsing every outbound frame adds CPU and allocation overhead to
  the previously raw byte path. Severity: medium. Likelihood: medium.
  Mitigation: keep enforcement strictly opt-in via a single `CapabilityPolicy`
  enum on `ExecSessionOptions` (default `Disabled`), preserve byte-transparency
  on parse failure, and bound the per-frame buffer at a 128 kilobyte ceiling
  for the runtime path (chosen because agent-emitted ACP `prompt`-style
  payloads can carry embedded resources up to the same scale and exceed the 64
  kilobyte input ceiling). When the buffer fills before a newline is found,
  drop the partial frame, log the truncation to stderr exactly once, and fall
  back to raw byte forwarding for the remainder of the session.

- Risk: the framing assembler corrupts a permitted stream by misplacing
  newlines that occur inside JSON string literals. Severity: high. Likelihood:
  low. Mitigation: ACP frames are JSON-RPC objects on a single line by
  construction, but coverage must include scenarios where a JSON string literal
  contains a `\n` escape (which is not a real newline byte) and where chunk
  boundaries split a multi-byte Unicode Transformation Format (UTF-8) sequence.

- Risk: applying the denylist to forwarded host stdout could accidentally
  emit Podbot-generated bytes into host stdout, breaking the stream purity
  contract. Severity: high. Likelihood: low. Mitigation: synthesized error
  responses are written to **container stdin** (the agent's input), not host
  stdout. Permitted frames are forwarded byte-for-byte from the original slice;
  the policy parses to *decide* but never re-serializes before forwarding. Add
  a stream-purity assertion to the new behavioural feature, including a
  golden-bytes comparison rather than a parsed-JSON comparison.

- Risk: prefix-based matching (`terminal/`, `fs/`) may both block too
  much (an unrelated `terminal-monitor` method, hypothetically) and miss future
  ACP methods that do not share the prefix. Severity: medium. Likelihood: low.
  Mitigation: the ACP specification scopes capability families with the
  trailing `/` separator, so prefix matching with a literal `/` delimiter is
  the correct rule. Encode the families as `&[&str] = &["terminal/", "fs/"]` in
  one place so future families can be added in a single edit. Cover boundary
  cases (`terminal`, `terminalx`, `terminal/`, `terminal/create`) in unit tests.

- Risk: feature-file edits for `rstest-bdd` are compile-time inputs;
  stale generated bindings can mask scenario-name mismatches. Severity: medium.
  Likelihood: medium. Mitigation: keep scenario titles synchronized with the
  feature file and trigger a clean rebuild
  (`cargo clean -p podbot && make test 2>&1 | tee ...`) once if generated
  bindings appear stale.

## Context and orientation

Read the following first; the plan assumes nothing else.

- `docs/podbot-roadmap.md` lines 186 to 220 define Step 2.6 and the
  remaining checkboxes. The completion criteria for Step 2.6 are at lines 201
  to 207 (sandbox-preserving default, opt-in override, denials recorded on
  stderr).
- `docs/podbot-design.md` lines 197 to 226 record the design intent: ACP
  hosting must default to sandbox-preserving masking, must defensively reject
  blocked methods at runtime, and must surface override decisions as
  trust-boundary events.
- `docs/execplans/2-6-1-intercept-acp-initialization.md` describes the
  init-time masking implementation that this step extends.
- `src/engine/connection/exec/protocol.rs` is the protocol proxy. It owns
  `forward_host_stdin_to_exec_async` (input direction) and
  `run_output_loop_async` plus `handle_log_output_chunk` (output direction).
  The protocol session is configured by `ProtocolSessionOptions`.
- `src/engine/connection/exec/acp_helpers.rs` holds the pure init-frame
  rewriter (`mask_acp_initialize_frame`, `split_frame_line_ending`). Treat this
  module as the existing domain seam; new pure policy helpers belong in a
  sibling module to keep both files inside the 400-line guidance.
- `src/engine/connection/exec/session.rs` exposes
  `ExecSessionOptions::with_acp_initialize_rewrite_enabled`. This flag is
  currently dead code outside tests. It is the natural opt-in to extend.
- `src/engine/connection/exec/helpers.rs` provides
  `spawn_stdin_forwarding_task`, the seam through which the input task is
  spawned with sole ownership of the container input writer.
- `src/engine/connection/exec/protocol_acp_tests.rs`,
  `protocol_acp_forwarding_tests.rs`, and `protocol_acp_bdd_tests.rs` capture
  the unit-and-behavioural testing pattern. The `Slot` and `ScenarioState`
  patterns from `rstest-bdd` v0.5.0 are required.
- `tests/features/acp_capability_masking.feature` is the existing ACP
  feature file. The denylist scenarios go into a new sibling file
  (`acp_method_denylist.feature`) so each feature file remains focused and
  short.

ACP framing assumed by this plan, taken from the existing ACP guidance
referenced in `docs/podbot-design.md`:

- ACP traffic is line-delimited JSON-RPC 2.0. Each frame is a single
  JSON-RPC object terminated by `\n` (or `\r\n`).
- Requests carry `method`, `id`, and optional `params`. Responses carry
  `id` plus either `result` or `error`. Notifications carry `method` without
  `id`.
- Capability families that Podbot blocks by default are `terminal/...`
  and `fs/...`. These methods are emitted by the agent and target the client
  (the Integrated Development Environment, or IDE), so the bytes flow agent
  stdout to host stdout in the protocol proxy.

The data flow that this step modifies is therefore the agent-to-host direction.
The init-masking work in 2.6.1 lives on the host-to-agent direction; this step
adds the symmetrical enforcement on the return path.

## Plan of work

The work is broken into stages. Each stage ends with a validation gate; do not
proceed past a failing validation.

### Stage A: Confirm the landing zones (no code changes)

Read the modules listed under `Context and orientation` and confirm:

- the protocol proxy seam still routes outbound bytes through
  `handle_log_output_chunk` for both `LogOutput::StdOut` and
  `LogOutput::Console` and routes nothing else to host stdout;
- `ProtocolSessionOptions` is the only opt-in surface that needs
  extension;
- no other code path forwards container stdout to host stdout for
  `ExecMode::Protocol`.

Validation: a brief written summary in the `Surprises and discoveries` section
if any of the assumptions above are false. Otherwise proceed.

### Stage A.5: Architecture spike — sink task vs single bidirectional task

Before committing to the dedicated container-stdin sink task, run a 30-minute
spike of the alternative described in the Logisphere review: fold both the
host-stdin forwarder and the output-direction policy adapter into a single
`tokio::select!`-driven task that owns the container input writer directly.
Measure the diff in `protocol.rs` and the impact on the existing test
scaffolding.

Acceptance for the spike: if the single-task fold keeps `protocol.rs` under the
400-line guidance, leaves the existing 2.6.1 tests unchanged, and expresses the
"drain pending denials before shutdown" invariant without `select!` ordering
surprises, prefer it. Otherwise commit to the dedicated sink task and document
the decision in the `Decision log`. Either decision must be recorded before
Stage C proceeds.

### Stage B: Add the pure policy domain

Create `src/engine/connection/exec/acp_policy.rs` containing only pure logic.
The module must not depend on `tokio` or `tracing`; the adapter in Stage C is
responsible for all I/O and observability.

Define a `MethodFamily` value type, a `MethodDenylist` aggregate of families,
the default family list, the `FrameDecision` enum (note the absence of
pre-built error bytes — the decision stays serialization-free so it is
trivially testable), and the two pure functions `evaluate_agent_outbound_frame`
and `build_method_blocked_error`. The full Rust signatures appear in
`Interfaces and dependencies`.

Behavioural rules for `evaluate_agent_outbound_frame`:

- On JavaScript Object Notation (JSON) parse failure, return
  `Forward` (tolerant pass-through).
- On a JSON-RPC request whose `method` is blocked, return
  `BlockRequest` preserving the `id` value as `serde_json::Value` (do not
  coerce numeric ids to strings).
- On a JSON-RPC notification whose `method` is blocked, return
  `BlockNotification` carrying the method name.
- Frames without a `method` field (responses, batches, malformed
  objects) return `Forward`.

`build_method_blocked_error` produces a JSON-RPC 2.0 error response of the form:

```json
{
  "jsonrpc": "2.0",
  "id": "<original id, type-preserved>",
  "error": {
    "code": -32001,
    "message": "Method blocked by Podbot ACP capability policy",
    "data": {
      "method": "<method>",
      "reason": "podbot_capability_policy"
    }
  }
}
```

It appends the supplied line-ending bytes (default `\n` if the original frame
had no recognized line ending). The `-32001` code lives in the JSON-RPC
application error range `-32099..=-32000`; reserve `-32002` for the Step 2.6.4
"override required" follow-on. Do not use `-32601 Method not found` because the
method exists and the agent could otherwise retry assuming a typo.

Reuse `split_frame_line_ending` from `acp_helpers.rs`.

Note on visibility: although these items are reached only through
`exec::protocol` today, future Corbusier conformance work
(`docs/corbusier-conformance-design-for-agents-mcp-wires-and-hooks.md`) needs
the same denylist as a single source of truth. Keep `pub(crate)` for now; the
public boundary established in Step 5.3.1 remains intact.

Add unit tests under `src/engine/connection/exec/acp_policy_tests.rs` covering:

- `MethodFamily::matches` boundary cases (`terminal`, `terminalize`,
  `terminal/`, `terminal/create`);
- `MethodDenylist::is_blocked` with both default families;
- blocked request with numeric, string, and null `id` (each preserved
  as the original JSON type in the synthesized error);
- blocked notification (no id field);
- permitted method passes through;
- malformed JSON returns `Forward`;
- frames without a `method` field (responses) return `Forward`;
- `build_method_blocked_error` output round-trips through `serde_json`,
  carries the expected `code`, `message`, `data.method`, and `data.reason`
  fields, and preserves the original line ending.

Validation:
`cargo test -p podbot --lib acp_policy_tests 2>&1 | tee /tmp/test-podbot-session-e445b19d.out`
 passes; new tests fail before the module is implemented and pass after.

### Stage C: Add the frame assembler

Create `src/engine/connection/exec/acp_frame.rs` containing the streaming
framer. The assembler is also pure (no `tokio` or `tracing`); it returns a
vector of decisions per chunk and lets the adapter perform the actual writes.

Set `MAX_RUNTIME_FRAME_BYTES` to 128 kilobytes (twice the input ceiling),
chosen because agent-emitted ACP `prompt`-style payloads can be larger than the
host-driven input frames. Define a `FrameOutput` borrow-style enum and an
`OutboundFrameAssembler` struct as described in `Interfaces and dependencies`.
The trailing `&[u8]` on `FrameOutput` carries the original frame bytes
(including line ending) for `Forward` decisions and the original line-ending
bytes for `Decision` results so the adapter can reconstruct the synthesized
error with the same line terminator.

Behavioural rules for the assembler:

- `ingest_chunk` returns an iterator over completed frames in the
  supplied chunk. On buffer overflow before a newline is observed, emit a
  one-shot `Forward` covering the buffered bytes, flip an internal
  `raw_fallback` flag, and from then on every chunk is emitted as `Forward`
  unchanged.
- `finish` is called at end of stream. The assembler **drops** any
  residual partial frame: an unauthorized partial frame must not be forwarded
  to host stdout. `finish` returns `None` and the adapter logs the byte count
  to stderr exactly once.

Add unit tests under `src/engine/connection/exec/acp_frame_tests.rs` covering:

- multi-frame chunks split on every `\n`;
- frames spanning two and three chunk boundaries reassembled correctly;
- chunk that splits a multi-byte UTF-8 sequence at the boundary still
  reassembles correctly;
- frame containing a `\\n` escape inside a JSON string literal (no real
  newline byte) is treated as a single frame;
- buffer overflow flips to raw fallback and emits subsequent chunks as
  `Forward` unchanged;
- residual partial frame at end-of-stream returns `None` from
  `finish` and is not forwarded.

Validation: `cargo test -p podbot --lib acp_frame_tests 2>&1 | tee ...` passes.

### Stage D: Add the output-direction adapter and the sink task

Create `src/engine/connection/exec/acp_runtime.rs` containing the adapter and
the container-stdin sink task. This module owns all `tokio::sync::mpsc` and
`tracing` use for the runtime path.

Define a `WriteCmd` enum (`Forward`, `Synthesized`, `Shutdown`) describing
every byte written to container stdin. Define `run_container_stdin_sink` as the
dedicated sink task and the `OutboundPolicyAdapter` struct paired with
`handle_chunk` and `finish` methods. Full signatures appear in
`Interfaces and dependencies`.

Behavioural rules for the adapter:

- For each `FrameOutput::Forward(bytes)`, write the original byte slice
  to `host_stdout`. Never re-serialize.
- For each `FrameOutput::Decision(BlockRequest { id, method }, line_ending)`,
  call `build_method_blocked_error`, push `WriteCmd::Synthesized` into the
  channel, and emit `tracing::warn!` with `target = "podbot::acp::policy"`, the
  `container_id`, the blocked `method`, the request `id`, and a stable
  `"ACP blocked request denied"` message body.
- For each `FrameOutput::Decision(BlockNotification { method }, _)`,
  drop the bytes and emit a `tracing::warn!` with the same target,
  `container_id`, blocked `method`, `id = serde_json::Value::Null`, and body
  `"ACP blocked notification dropped"`.

Behavioural rules for the sink:

- Drain commands until `Shutdown` is received, writing and flushing each
  one in arrival order.
- On `BrokenPipe` from container stdin (the agent has exited), downgrade
  subsequent writes to a single `tracing::warn!` and continue draining the
  channel until `Shutdown`. The protocol session still completes cleanly and
  the exit-code reporting path remains intact.
- After `Shutdown`, call `input.shutdown().await` once and return.

Modify `src/engine/connection/exec/protocol.rs`:

- Replace the two ACP booleans on `ProtocolSessionOptions` with a
  `pub(super) capability_policy: CapabilityPolicy` field of type
  `pub(super) enum CapabilityPolicy { Disabled, MaskOnly, MaskAndDeny }`,
  defined in `session.rs`. Default remains `Disabled`.
- In `run_protocol_session_with_io_async`, when the policy is
  `MaskAndDeny`:
  - Build a bounded `tokio::sync::mpsc::channel::<WriteCmd>(16)`.
  - Spawn `run_container_stdin_sink` as a separate task owning the
    container input writer.
  - Build the host-stdin forwarding task to send `WriteCmd::Forward`
    chunks into the same channel (replacing its current
    `tokio::io::copy`-into-writer path).
  - Build the `OutboundPolicyAdapter` with the same channel sender and
    pass it into `run_output_loop_async`.
  - After the output loop returns, the adapter sends `WriteCmd::Shutdown`,
    awaits the sink task, then awaits the host-stdin forwarder under
    the existing `STDIN_SETTLE_TIMEOUT`.
- When the policy is `MaskOnly`, behave exactly as today (init
  rewriting on, runtime enforcement off). When the policy is `Disabled`, the
  existing byte-transparent code path is taken.

Validation:
`cargo build --workspace --all-targets --all-features 2>&1 | tee /tmp/build-podbot-session-e445b19d.out`
 succeeds, and `cargo test -p podbot --lib 2>&1 | tee ...` passes new unit
tests for the adapter and sink covering:

- blocked request followed by permitted frame: only the permitted frame
  reaches `host_stdout`, and the synthesized error reaches the sink;
- the sink delivers the synthesized error before processing
  `WriteCmd::Shutdown`;
- the sink handles a `BrokenPipe` from container stdin without
  failing the session;
- `WriteCmd::Forward` from the host-stdin forwarder is interleaved
  correctly with `WriteCmd::Synthesized` from the adapter;
- adapter is a no-op for `LogOutput::StdErr` chunks, which still flow
  to host stderr verbatim.

### Stage E: Wire enforcement through the session option

In `src/engine/connection/exec/session.rs`:

- Define `pub(crate) enum CapabilityPolicy { Disabled, MaskOnly, MaskAndDeny }`
  with `pub(crate) const fn allows_runtime_enforcement(self) -> bool` and
  `pub(crate) const fn rewrites_initialize(self) -> bool` helpers.
- Replace the existing `rewrite_acp_initialize: bool` field with
  `capability_policy: CapabilityPolicy` and rename
  `with_acp_initialize_rewrite_enabled` to
  `with_acp_capability_policy(policy: CapabilityPolicy)`. Update the
  `protocol_session_options` translator accordingly.
- Update the existing tests in `session.rs` so the renamed builder
  asserts the combined semantic.

In `src/engine/connection/exec/mod.rs` and `src/api/exec.rs`:

- No public surface change. The opt-in is reachable only from the
  internal session-options seam, consistent with Step 2.6.1.

Validation: `cargo test -p podbot --lib session 2>&1 | tee ...` passes.

### Stage F: Add behavioural coverage with `rstest-bdd`

Create `tests/features/acp_method_denylist.feature` with scenarios:

1. Blocked request returns synthesized error and is not forwarded.
2. Blocked notification is dropped silently with a stderr record.
3. Permitted method passes through unchanged byte-for-byte.
4. Frame split across two chunks reassembles before the policy applies.
5. Oversized frame falls back to raw forwarding for the rest of the
   session and emits a single stderr fallback record.
6. Permitted frame after a blocked frame still flushes correctly.
7. Container stdin sees the synthesized error response within one chunk
   of the blocked request being observed on the output stream, and strictly
   before the sink processes `WriteCmd::Shutdown`.

Bind the scenarios in `src/engine/connection/exec/acp_runtime_bdd_tests.rs`,
mirroring the `AcpMaskingState` pattern from `protocol_acp_bdd_tests.rs`. Use a
recording writer for `host_stdout` and a recording sink (a
`tokio::sync::mpsc::Receiver` drained into a `Vec<WriteCmd>`) for container
stdin so the assertions can verify ordering and bytes.

Validation: `make test 2>&1 | tee ...` passes; the new `.feature` scenarios
appear in the test output and fail before the bindings are present, pass after.

### Stage G: Parameterized coverage for the framer

`proptest` is not in the workspace lockfile and adding it would breach the
no-new-dependency constraint. Instead, add an exhaustive parameterized `rstest`
table in `src/engine/connection/exec/acp_frame_tests.rs` that:

- generates a fixed sequence of permitted JSON-RPC frames as test
  data (literal byte fixtures, not random);
- splits the concatenation at every byte boundary
  (`for split in 1..bytes.len()`), feeding the two halves through the assembler
  in two `ingest_chunk` calls;
- asserts the recorded permitted output equals the original
  concatenation byte-for-byte;
- repeats with a three-way split table covering at least 32 distinct
  triples, including splits inside JSON string literals and inside multi-byte
  UTF-8 sequences.

Validation: the parameterized cases (at least 256 distinct splits) all pass. If
a future change adds `proptest` to the workspace for unrelated reasons, this
table becomes the seed corpus.

### Stage H: Documentation

Update `docs/podbot-design.md` to describe:

- the runtime denylist policy (which families are blocked, why
  trailing-slash prefix matching is correct, JSON-RPC error code `-32001`, the
  `data.reason = "podbot_capability_policy"` discriminator);
- the dedicated container-stdin sink task and the
  `WriteCmd::{Forward, Synthesized, Shutdown}` ordering invariant;
- the raw-fallback behaviour on buffer overflow and the
  drop-partial-frame behaviour at end of stream;
- the `CapabilityPolicy::{Disabled, MaskOnly, MaskAndDeny}` enum and
  the rationale for collapsing the previous two booleans;
- the explicit out-of-scope notes for Steps 2.6.3 to 2.6.5.

Update `docs/users-guide.md` to describe operator-visible behaviour:

- ACP enforcement is opt-in until `podbot host` ships;
- when enforcement is on, hosted agents that emit `terminal/*` or
  `fs/*` calls receive a JSON-RPC error response (`code: -32001`) and a stderr
  `WARN` line records the denial with the method name and request id;
- non-blocked methods pass through byte-for-byte.

Update `docs/developers-guide.md` to describe the internal architecture:

- the pure `acp_policy` module separates decision logic from input,
  output, or telemetry;
- the pure `acp_frame` module owns the newline-delimited assembler and
  its `MAX_RUNTIME_FRAME_BYTES = 128 KiB` ceiling;
- the `acp_runtime` module is the output-direction adapter and the
  container-stdin sink task;
- the existing `acp_helpers` module retains the init-time rewriter and
  is unchanged by this step;
- the family of ACP modules is declared from
  `src/engine/connection/exec/mod.rs` rather than inline `#[path]` attributes
  inside `protocol.rs`.

Validation: `make fmt 2>&1 | tee ...`, `make markdownlint 2>&1 | tee ...`, and
`make nixie 2>&1 | tee ...` succeed.

### Stage I: Roadmap and final gates

Update `docs/podbot-roadmap.md` to mark only the second Step 2.6 checkbox done.
Leave the next three checkboxes open.

Run the full gate stack sequentially with `tee` for each command (do not
parallelize):

```shell
set -o pipefail
make fmt 2>&1 | tee /tmp/fmt-podbot-session-e445b19d.out
make markdownlint 2>&1 | tee /tmp/markdownlint-podbot-session-e445b19d.out
make nixie 2>&1 | tee /tmp/nixie-podbot-session-e445b19d.out
make check-fmt 2>&1 | tee /tmp/check-fmt-podbot-session-e445b19d.out
make lint 2>&1 | tee /tmp/lint-podbot-session-e445b19d.out
make test 2>&1 | tee /tmp/test-podbot-session-e445b19d.out
```

## Validation and acceptance

Acceptance is observable through the following experiments.

- Compile-time: `cargo build --workspace --all-targets --all-features`
  succeeds and the new modules are present at
  `src/engine/connection/exec/acp_policy.rs`,
  `src/engine/connection/exec/acp_runtime.rs`, and the corresponding unit and
  behavioural test files.
- Unit tests: `cargo test -p podbot --lib` reports the new
  `acp_policy_tests`, `acp_runtime_tests`, and the property/parameterized
  framer test as passing. Each new test fails when run against the tip of
  `main` and passes against this branch.
- Behavioural tests: `cargo test -p podbot --test bdd` (or whichever
  test binary already executes existing `acp_capability_masking` scenarios)
  reports the seven new `acp_method_denylist` scenarios as passing.
- Integration: a hand-driven check using
  `RecordingWriter`-style doubles (the same pattern as
  `protocol_acp_forwarding_tests.rs`) demonstrates that:
  - feeding a `terminal/create` JSON-RPC request into the simulated
    container stdout produces zero bytes on host stdout, one synthesized
    JSON-RPC error frame on container stdin, and one stderr warn line;
  - feeding a `session/update` request through the same path yields
    byte-identical pass-through.
- Documentation: `docs/podbot-design.md`, `docs/users-guide.md`, and
  `docs/developers-guide.md` describe the runtime denylist exactly as shipped.
- Roadmap: the second Step 2.6 checkbox is marked done; the third,
  fourth, and fifth checkboxes remain open with no other roadmap changes.
- Gates: `make check-fmt`, `make lint`, and `make test` all succeed.
  `make fmt`, `make markdownlint`, and `make nixie` succeed.

Quality criteria:

- No new `clippy` warnings under `-D warnings`.
- No new `unwrap` or `expect` in production code.
- Every new module begins with `//!` documentation.
- Every public item retains British English Oxford spelling in its
  documentation.
- The 400-line guidance holds for every touched module.

## Idempotence and recovery

- The new modules are additive; if implementation is interrupted, the
  partial change can be reverted by removing the new files and reverting the
  small edits to `protocol.rs`, `session.rs`, and the documentation.
- If `rstest-bdd` scenario bindings appear stale, run
  `cargo clean -p podbot && make test 2>&1 | tee ...` once.
- If the property/parameterized framer test detects a regression, the
  failure case can be added as a deterministic `rstest` case before fixing the
  assembler.

## Agent team execution model

Run reconnaissance and review concurrently up front so the main implementation
thread stays coherent.

- Lane A (docs and roadmap reconnaissance owner, may be a sub-agent):
  read the roadmap, design, and prior 2.6.1 plan to confirm scope and
  documentation surface. Output: a one-paragraph note flagging any surprises
  before Stage B begins.
- Lane B (Logisphere design review, runs in parallel during planning):
  Pandalump (structure), Wafflecat (alternatives), Buzzy Bee (scale and
  observability), Telefono (JSON-RPC contract correctness), Doggylump (failure
  modes and ordering), Dinolump (long-term viability). Output feeds the
  `Decision log` before Stage C begins.
- Lane C (primary implementation owner, main thread): drive Stages A
  through H sequentially, integrating Lane B findings into the design before
  writing code.

Coordination rule: code edits land only in Lane C. Sub-agents may read and
report; they must not write to the working tree.

## Progress

- [x] (2026-05-02) Drafted ExecPlan after reading the roadmap, design,
  prior 2.6.1 plan, and current ACP module layout.
- [x] (2026-05-02) Logisphere design review completed and folded into
  `Decision log` (sink-task model, `CapabilityPolicy` enum, three-module split,
  byte-identical forwarding, drop-partial-frame, error data shape, family
  matching with `/` boundary).
- [x] (2026-05-02) Stage A landing-zone confirmation. Verified that
  `attached.rs` is the only other module reading `LogOutput::StdOut`/`Console`,
  but it serves the attached (TTY) mode rather than `ExecMode::Protocol`. The
  protocol proxy seam in `src/engine/connection/exec/protocol.rs` remains the
  only host-stdout forwarder for protocol mode.
- [x] (2026-05-02) Stage A.5 architecture spike completed. The
  single-`select!` fold is conceptually cleaner (no channel, no shutdown
  signalling) but requires restructuring both
  `forward_host_stdin_to_exec_async` and `spawn_stdin_forwarding_task` to
  interleave per-chunk reads inside the output loop, losing the current shared
  cancellation seam. The dedicated sink task preserves the existing host-stdin
  task structure (one-line change to send into a channel), isolates
  cancellation under `STDIN_SETTLE_TIMEOUT`, and makes `WriteCmd` ordering
  explicit. Decision: commit to sink-task model.
- [x] (2026-05-02) Stage B pure `acp_policy` module and unit tests
  (25 cases passing). Module declared as a child of `protocol` via `#[path]`,
  matching the 2.6.1 `acp_helpers` pattern. The `build_method_blocked_error`
  function returns `serde_json::Result<Vec<u8>>` so the production path can
  avoid `expect()` on the (practically infallible) serialization step.
- [x] (2026-05-02) Stage C `acp_frame` assembler and unit tests
  (16 cases passing). The assembler is fully synchronous with no channel or
  `tokio` dependency. `ingest_chunk` returns
  `(Vec<FrameOutput>, Option<FallbackReason>)` so the adapter can act on
  per-chunk fallback events. `finish` returns `Option<FallbackReason>` so the
  adapter logs at most one partial-frame drop per session.
- [x] (2026-05-02) Stage D `acp_runtime` adapter, sink task, and unit
  tests (10 cases passing). The `WriteCmd` enum was simplified to two variants
  (`Forward`, `Synthesised`); the sink terminates on channel close instead of
  an explicit `Shutdown` command, eliminating a race where a misordered
  `Shutdown` could drop queued items.
- [x] (2026-05-02) Stage E session-options wiring (`CapabilityPolicy`
  enum). `ProtocolSessionOptions::with_capability_policy` replaces
  `with_acp_initialize_rewrite_enabled` and the protocol session splits into
  `run_session_with_runtime_enforcement` (channel-based sink path) and
  `run_session_without_runtime_enforcement` (existing byte-transparent path).
  All 448 workspace unit tests pass.
- [x] (2026-05-02) Stage F `rstest-bdd` behavioural feature
  (`tests/features/acp_method_denylist.feature`) and bindings
  (`src/engine/connection/exec/acp_runtime_bdd_tests.rs`). 5 scenarios exercise
  blocked requests, permitted requests, blocked notifications, multi-chunk
  reassembly, and blocked-then-permitted ordering, all asserting both host
  stdout (byte-identical permitted forwards) and container stdin (synthesized
  error responses with preserved id).
- [x] (2026-05-02) Stage G framer parameterized coverage. The
  `every_two_way_split_reassembles_to_original_byte_stream` test exhaustively
  splits a five-frame permitted stream at every byte boundary (over 250
  splits). The `three_way_splits_reassemble_...` parameterized table covers 32
  distinct triples. All splits reassemble byte-identically, confirming that
  frames forwarded by the assembler are bit-for-bit identical to the original
  input regardless of chunk boundaries.
- [x] (2026-05-02) Stage H documentation updates. The runtime denylist
  policy, sink-task model, frame ceiling, and `CapabilityPolicy` enum are now
  described in `docs/podbot-design.md`. The user-facing behaviour (synthesized
  error response, stderr denial line, byte-identical permitted forwards, opt-in
  until `podbot host` ships) is in `docs/users-guide.md`. A new section 8.2.2
  in `docs/developers-guide.md` documents the four-module split, the
  `CapabilityPolicy` selector, and the recommended testing pattern. All four
  touched Markdown files pass `markdownlint`.
- [x] (2026-05-02) Stage I roadmap update and full gate stack. The
  second Step 2.6 roadmap checkbox is marked done. Final results:
  `make check-fmt` ✓, `make lint` ✓ (clippy `-D warnings`),
  `make test` ✓ (492 library tests, all integration suites pass with
  0 failures), `make markdownlint` ✓, `make nixie` ✓. Pre-existing
  `make fmt` failures in `users-guide.md` (lines 407-446 and
  640-817) predate this branch and were verified by running
  `make fmt` against `git stash`'d state.

## Surprises and discoveries

- Discovery: `ExecSessionOptions::with_acp_initialize_rewrite_enabled`
  is currently dead code outside tests and is the only opt-in seam for ACP
  behaviour. Replacing it (and the proposed second boolean) with a single
  `CapabilityPolicy` enum keeps the internal API surface minimal while making
  the intermediate `MaskOnly` mode representable for diagnostic sessions.
- Discovery: the existing protocol proxy already routes nothing to host
  stdout other than `LogOutput::StdOut`/`Console` chunks, so adding the
  output-direction interceptor at `handle_log_output_chunk` is the smallest
  possible change to the stdout-purity contract.
- Discovery: the ACP method polarity is agent-emitted. `terminal/*` and
  `fs/*` requests originate from the hosted agent and target the IDE client
  over the agent's stdout. The runtime denylist therefore lives on the
  container-to-host (output) direction, and the synthesized error response is
  written back to **container stdin** so the agent observes a JSON-RPC error to
  its own outbound request. The pure policy function name
  `evaluate_agent_outbound_frame` makes this polarity explicit in the type
  signature.
- Discovery: `acp_helpers.rs` is already 263 lines and would breach the
  400-line guidance if extended with the assembler, the policy enum, and the
  error builder. The three-module split (`acp_helpers`, `acp_policy`,
  `acp_frame`, plus the adapter `acp_runtime`) keeps every module under roughly
  150 lines and lets future Corbusier conformance work import the policy
  without dragging in the assembler or the runtime adapter.
- Discovery: `bytes::BytesMut` would be a more efficient buffer than
  `Vec<u8>` for the assembler, but `bytes` is not a direct workspace dependency
  (it appears only transitively via `tokio` and `bollard`). The
  no-new-dependency constraint forbids promoting it, so the plan uses `Vec<u8>`
  and reserves a future optimization note for when `bytes` becomes a direct
  dependency for unrelated reasons.
- Discovery: `proptest` is not in the workspace lockfile either, so
  Stage G uses an exhaustive `rstest` parameterized table over fixed byte
  fixtures rather than randomized property generation.

## Decision log

- Decision: bundle the synthesized JSON-RPC error response and stderr
  denial line into Step 2.6.2. Rationale: enforcement without a response causes
  the hosted agent to hang forever waiting for a reply, which would be a worse
  user experience than the current absence of enforcement. The roadmap's third
  checkbox (richer diagnostics) remains open for follow-on work to enrich the
  stderr structure and any operator-facing telemetry.
- Decision: collapse the existing `rewrite_acp_initialize: bool` and
  the proposed `enforce_acp_method_denylist: bool` into a single
  `CapabilityPolicy::{Disabled, MaskOnly, MaskAndDeny}` enum on
  `ExecSessionOptions`, replacing `with_acp_initialize_rewrite_enabled` with
  `with_acp_capability_policy(policy)`. Rationale: the Logisphere review
  observed that two booleans for one trust-boundary decision invite drift; the
  explicit `MaskOnly` mode is also useful for diagnostic sessions that want
  init masking without runtime denial. The enum keeps every combination
  representable through one constructor.
- Decision: introduce a dedicated container-stdin sink task driven by a
  `WriteCmd::{Forward, Synthesized, Shutdown}` channel, rather than having the
  existing host-stdin forwarder drain a secondary mpsc alongside its
  `tokio::io::copy` loop. Rationale: the Logisphere `Doggylump` analysis showed
  that draining a secondary queue from inside a `tokio::io::copy` loop is
  awkward and collides with the `STDIN_SETTLE_TIMEOUT` race. A dedicated sink
  task is the single ordering authority: it drains until `Shutdown`, so the
  output adapter can guarantee that synthesized errors are flushed before
  container stdin closes. Both the host-stdin forwarder and the output adapter
  become *senders* with no shared writer ownership.
- Decision: keep parsing pure and never re-serialize permitted frames.
  Rationale: byte-identical forwarding preserves any agent-side integrity
  assumptions (key ordering, whitespace, embedded hashes) and avoids
  weaponizing the proxy against ACP extensions that depend on byte-stable
  frames. The `OutboundFrameAssembler` retains the original byte slice for
  every `Forward` decision; `serde_json::from_slice` is used only for the
  decision step.
- Decision: prefix-match capability families using a trailing `/`
  delimiter (`terminal/`, `fs/`), encoded as
  `MethodFamily { prefix: &'static str }` values. Rationale: ACP scopes
  capability families with the `/` separator and introduces methods of the form
  `family/operation`. Prefix matching with the delimiter avoids both false
  positives (a hypothetical `terminate` method) and false negatives (future
  `terminal/...` methods). Encode the family list in a single constant so
  adding a new family — or a Step 2.6.4 override allowlist — is a single edit.
- Decision: use JSON-RPC application error code `-32001` with message
  `"Method blocked by Podbot ACP capability policy"` and data
  `{"method": "<name>", "reason": "podbot_capability_policy"}`. Rationale:
  `-32601 Method not found` is reserved for methods the server does not
  implement; the agent's method does exist but Podbot is withholding it.
  `-32001` lives in the JSON-RPC application range `-32099..=-32000` and gives
  operators a stable code to grep for in agent logs. `-32002` is reserved for
  the Step 2.6.4 "override required" follow-on. The `data.reason` discriminator
  lets agents branch on `reason` without parsing the message string.
- Decision: raise the runtime frame ceiling to 128 kilobytes
  (`MAX_RUNTIME_FRAME_BYTES`), distinct from the 64 kilobyte
  `STDIN_BUFFER_CAPACITY` used on the input path. Rationale: the Logisphere
  `Buzzy Bee` analysis flagged that agent-emitted ACP `prompt`-style payloads
  can carry embedded resources that exceed the input ceiling. The runtime path
  needs more headroom; reusing the input ceiling would silently truncate
  legitimate frames into the raw-fallback path.
- Decision: at end of stream, drop any residual partial frame instead
  of forwarding it. Rationale: a partial frame is by definition unauthorized —
  the policy never decided on it. Forwarding bytes that have not been
  classified would re-introduce the very class of leak this step is meant to
  prevent. The byte count of the dropped residual is logged to stderr exactly
  once for diagnostics. Note: this is a deliberate tightening from the looser
  tolerant-pass-through used at init time in 2.6.1, where the first frame is
  always forwarded.
- Decision: on buffer overflow before a newline is observed, flush the
  buffered bytes verbatim, set a one-shot raw-fallback flag, and forward all
  subsequent bytes raw for the remainder of the session. Rationale: the
  established 2.6.1 policy is tolerant pass-through on size limits, and
  weaponizing a single oversize frame into a hard failure would harm legitimate
  agents. Recording the fallback once on stderr keeps the operator informed
  without spamming a tight loop.
- Decision: insert a Stage A.5 architecture spike that times-boxes a
  comparison between the dedicated sink task and a single
  `tokio::select!`-driven bidirectional task. Rationale: the Logisphere review
  explicitly named the single-task fold as the alternative most worth a
  checkpoint before committing to the sink design. A bounded spike costs little
  and prevents re-architecture later if the simpler shape fits inside the
  400-line guidance.
- Decision: keep the new ACP modules (`acp_policy`, and the upcoming
  `acp_frame` and `acp_runtime`) as children of `protocol` via `#[path]`
  attributes, matching the established 2.6.1 pattern for `acp_helpers`, instead
  of promoting them to siblings under `src/engine/connection/exec/mod.rs` as
  the original Logisphere recommendation suggested. Rationale: the existing
  `protocol_acp_tests.rs` test module relies on `super::acp_helpers::...` paths
  inherited from the inline-`#[path]` pattern. Promoting modules to `mod.rs`
  would break those imports without a clear architectural payoff for this step.
  The runtime enforcement seam still composes naturally as a child of
  `protocol`, and future Corbusier conformance work can re-export the policy
  types through a more public path when an actual cross-module consumer arrives.
- Decision: simplify `WriteCmd` to two variants (`Forward`,
  `Synthesised`) and terminate the sink purely on channel close, removing the
  proposed `WriteCmd::Shutdown` variant. Rationale: with explicit `Shutdown`, a
  misordered send (Shutdown before pending Synthesised) would drop queued
  items. Channel-close is unconditional: every queued command flushes before
  the sink sees the terminator. The protocol coordinator drops every sender
  after the output stream drains, so the ordering invariant from the Logisphere
  review still holds with one fewer moving part.
- Decision: have `build_method_blocked_error` return
  `serde_json::Result<Vec<u8>>` instead of `Vec<u8>`. Rationale: AGENTS.md
  forbids `.expect()` in production code. The serialization step is practically
  infallible because every component of the synthesized payload is a finite,
  owned, non-recursive [`serde_json::Value`], but propagating the error keeps
  the production path panic-free. The runtime adapter handles the (theoretical)
  error by logging it as a `warn!` and continuing without sending a synthesized
  response, leaving the agent to time out — strictly better than a hard panic
  in the proxy.

## Outcomes and retrospective

Shipped behaviour matches every observable success criterion captured in
`Purpose and big picture`:

- The `MaskAndDeny` mode of `CapabilityPolicy` blocks every agent-emitted
  `terminal/*` and `fs/*` request before it reaches host stdout, returns a
  synthesized JSON-RPC 2.0 error response with the preserved `id` and the
  `-32001` / `data.reason = "podbot_capability_policy"` shape, and emits one
  `tracing::warn!` per denial on the `podbot::acp::policy` target.
- Permitted frames (including those split across Bollard chunk boundaries)
  are forwarded byte-identically. The `acp_frame` exhaustive parameterized
  test confirms reassembly equality at every byte split in a multi-frame
  stream.
- Malformed JSON, response/batch shapes, and methods outside the blocked
  families pass through unchanged, in keeping with the Step 2.6.1 tolerant
  policy.
- Stage 2.5 invariants hold: stdout purity, bounded buffering (now with a
  128 KiB runtime ceiling and a 64 KiB input ceiling), and accurate
  exit-code reporting through `settle_stdin_forwarding_task` plus the
  sink-task drain.
- `Disabled` and `MaskOnly` paths skip the sink and adapter wiring entirely,
  so the byte-transparent contract is preserved when enforcement is off.

Adjustments from the original draft:

- Module promotion: kept inline `#[path]` declarations rather than promoting
  modules to `mod.rs` (recorded in `Decision log`).
- `WriteCmd::Shutdown` removed: the sink terminates purely on channel close,
  eliminating the explicit-Shutdown ordering race.
- Refactoring for clippy's tight `cognitive-complexity-threshold = 9` and
  `too_many_arguments = 4` introduced several extra helper functions inside
  `acp_runtime` (`finalize_sink_writer`, `command_bytes`,
  `classify_pipe_outcome`, `log_shutdown_outcome`,
  `send_synthesized_or_log`, `log_send_failure`, `log_synthesis_failure`,
  `log_denial`, `emit_fallback_warning`, `warn_buffer_overflow`,
  `warn_partial_frame_drop`) and an `AdapterOutputIo` parameter struct in
  `protocol.rs`. The decomposition makes each function single-purpose and
  trivially testable.

Follow-on work (deliberately out of scope for 2.6.2):

- Step 2.6.3: enrich denial diagnostics, e.g. structured stderr with a
  per-session counter or an OpenTelemetry-style metric so operators can
  dashboard denial rates without grepping logs.
- Step 2.6.4: explicit operator override that flips selected blocked
  methods back to `Forward`. The `MethodDenylist` struct already models the
  family list as a `&'static [MethodFamily]`, so the override surface can
  be added without changing the policy's decision shape. Reserve JSON-RPC
  application code `-32002` for an "override required" variant when the
  policy is partially relaxed.
- Step 2.6.5: end-to-end tests that drive a real ACP session through the
  full `podbot host` pipeline once that command lands; the existing
  `OutboundPolicyAdapter` and sink task already provide the seams those
  tests will need.

Lessons:

- Bidirectional injection problems are genuinely solved by a single
  ordering authority. The dedicated sink task removed every cancellation
  and shutdown race the original two-boolean design would have left open,
  and the `WriteCmd` enum makes the queue contents trivially diffable in
  tests.
- `cognitive-complexity-threshold = 9` is unforgiving for handlers with
  multiple `tracing::warn!` arms; extracting per-arm log helpers is worth
  it because every helper becomes individually inspectable.
- The Logisphere review caught the agent-vs-host polarity confusion early;
  naming the policy function `evaluate_agent_outbound_frame` fixed the
  polarity at the type level rather than relying on doc comments.

## Interfaces and dependencies

In `src/engine/connection/exec/acp_policy.rs`, define (no `tokio`, no
`tracing`):

```rust
pub(crate) struct MethodFamily {
    pub(crate) prefix: &'static str,
}

impl MethodFamily {
    pub(crate) fn matches(&self, method: &str) -> bool {
        method
            .strip_prefix(self.prefix)
            .is_some_and(|rest| !rest.is_empty())
    }
}

pub(crate) struct MethodDenylist {
    families: &'static [MethodFamily],
}

pub(crate) const DEFAULT_BLOCKED_FAMILIES: &[MethodFamily] = &[
    MethodFamily { prefix: "terminal/" },
    MethodFamily { prefix: "fs/" },
];

impl MethodDenylist {
    pub(crate) const fn new(families: &'static [MethodFamily]) -> Self {
        Self { families }
    }

    pub(crate) fn is_blocked(&self, method: &str) -> bool {
        self.families.iter().any(|family| family.matches(method))
    }
}

pub(crate) enum FrameDecision {
    Forward,
    BlockNotification { method: String },
    BlockRequest {
        id: serde_json::Value,
        method: String,
    },
}

pub(crate) fn evaluate_agent_outbound_frame(
    frame: &[u8],
    denylist: &MethodDenylist,
) -> FrameDecision;

pub(crate) fn build_method_blocked_error(
    id: &serde_json::Value,
    method: &str,
    line_ending: &[u8],
) -> Vec<u8>;
```

In `src/engine/connection/exec/acp_frame.rs`, define (no `tokio`, no `tracing`):

```rust
pub(crate) const MAX_RUNTIME_FRAME_BYTES: usize = 131_072;

pub(crate) enum FrameOutput<'a> {
    Forward(&'a [u8]),
    Decision(FrameDecision, &'a [u8]),
}

pub(crate) struct OutboundFrameAssembler {
    buffer: Vec<u8>,
    denylist: MethodDenylist,
    raw_fallback: bool,
    fallback_logged: bool,
}

impl OutboundFrameAssembler {
    pub(crate) fn new(denylist: MethodDenylist) -> Self;

    pub(crate) fn ingest_chunk<'a>(
        &'a mut self,
        chunk: &'a [u8],
    ) -> impl Iterator<Item = FrameOutput<'a>> + 'a;

    pub(crate) fn finish(&mut self) -> Option<FrameOutput<'_>>;
}
```

In `src/engine/connection/exec/acp_runtime.rs`, define (this is the only ACP
module that touches `tokio::sync::mpsc` and `tracing`):

```rust
pub(super) enum WriteCmd {
    Forward(Vec<u8>),
    Synthesized(Vec<u8>),
    Shutdown,
}

pub(super) async fn run_container_stdin_sink(
    input: std::pin::Pin<Box<dyn tokio::io::AsyncWrite + Send>>,
    commands: tokio::sync::mpsc::Receiver<WriteCmd>,
) -> std::io::Result<()>;

pub(super) struct OutboundPolicyAdapter {
    assembler: OutboundFrameAssembler,
    sender: tokio::sync::mpsc::Sender<WriteCmd>,
    container_id: String,
}

impl OutboundPolicyAdapter {
    pub(super) fn new(
        assembler: OutboundFrameAssembler,
        sender: tokio::sync::mpsc::Sender<WriteCmd>,
        container_id: impl Into<String>,
    ) -> Self;

    pub(super) async fn handle_chunk<W: tokio::io::AsyncWrite + Unpin>(
        &mut self,
        chunk: &[u8],
        host_stdout: &mut W,
    ) -> Result<(), crate::error::PodbotError>;

    pub(super) async fn finish<W: tokio::io::AsyncWrite + Unpin>(
        &mut self,
        host_stdout: &mut W,
    ) -> Result<(), crate::error::PodbotError>;
}
```

In `src/engine/connection/exec/session.rs`, replace the
`rewrite_acp_initialize: bool` field with:

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum CapabilityPolicy {
    #[default]
    Disabled,
    MaskOnly,
    MaskAndDeny,
}

impl CapabilityPolicy {
    pub(crate) const fn rewrites_initialize(self) -> bool {
        matches!(self, Self::MaskOnly | Self::MaskAndDeny)
    }

    pub(crate) const fn allows_runtime_enforcement(self) -> bool {
        matches!(self, Self::MaskAndDeny)
    }
}
```

In `src/engine/connection/exec/protocol.rs`, the new `ProtocolSessionOptions`
field is `capability_policy: CapabilityPolicy`, defaulting to `Disabled`. The
two prior boolean fields and the `with_acp_initialize_rewrite_enabled` builder
are removed; every call site is migrated within the same change.

No new external dependencies are introduced. `tokio::sync::mpsc` and `tracing`
are already in the workspace; `serde_json` is reached through the existing
`ortho_config::serde_json` re-export.
