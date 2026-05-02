# Step 2.6.1: Intercept Agentic Control Protocol (ACP) initialization

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises and discoveries`,
`Decision log`, and `Outcomes and retrospective` must be kept up to date as
work proceeds.

Status: COMPLETE (2026-04-21)

No `PLANS.md` file exists in this repository as of 2026-04-21, so this ExecPlan
is the governing implementation document for this task.

## Purpose and big picture

Complete the first implementation task under Step 2.6 from
`docs/podbot-roadmap.md`: intercept ACP initialization and mask `terminal/*`
and `fs/*` capabilities before forwarding capability metadata into the
container.

Podbot already had a protocol-safe exec path in
`src/engine/connection/exec/protocol.rs`, but that path was intentionally a raw
byte proxy. That was correct for generic stdio protocols, yet insufficient for
ACP hosting because an IDE-side ACP client can advertise host terminal and
filesystem delegation features during `initialize`. Forwarding those
capabilities unchanged would let the hosted agent discover authority that
Podbot is meant to withhold across the sandbox boundary.

Observable success for this task:

- the protocol stdin proxy inspects the first newline-delimited ACP frame;
- an ACP `initialize` request has `params.clientCapabilities.terminal` and
  `params.clientCapabilities.fs` removed before the bytes reach the container;
- unrelated capabilities remain intact;
- malformed or non-ACP first frames pass through unchanged so protocol
  transparency remains intact for other transports;
- existing protocol-mode guarantees remain intact: no TTY, no resize
  propagation, and accurate exit-code reporting;
- unit tests with `rstest` and behavioural tests with `rstest-bdd` v0.5.0
  cover happy, unhappy, and edge paths;
- `docs/podbot-design.md` and `docs/users-guide.md` reflect the shipped
  behaviour;
- only the relevant Step 2.6 roadmap checkbox is marked done.

## Constraints

- Scope is limited to Step 2.6.1, not the full Step 2.6 feature set. Do not
  implement the later runtime denylist or delegation override here.
- Preserve the existing protocol proxy contract from Step 2.5:
  stdout purity, stderr-only diagnostics, bounded buffering, and explicit stdin
  shutdown ordering.
- Preserve attached-mode terminal handling, including initial resize, Unix
  `SIGWINCH` propagation, and current exit-code reporting.
- Do not add a new dependency. Reuse existing workspace crates and reexports.
- Keep touched Rust modules below the repository's 400-line guidance; prefer
  small focused test modules over inflating an existing file.
- Use `rstest` fixtures and parameterized cases for unit coverage.
- Use `rstest-bdd` v0.5.0 for behavioural coverage.
- Keep production code panic-free; use tolerant pass-through behaviour when ACP
  parsing fails.
- Use en-GB-oxendict spelling in comments and documentation.
- Run documentation gates because this change updates Markdown files:
  `make fmt`, `make markdownlint`, and `make nixie`.
- Run the requested Rust gates before completion:
  `make check-fmt`, `make lint`, and `make test`.

## Tolerances (exception triggers)

- Scope tolerance: stop and escalate if the work requires the `podbot host`
  command to exist before masking can be validated.
- Interface tolerance: stop and escalate if finishing the task requires a
  public API break rather than an internal proxy-seam change.
- Protocol tolerance: stop and escalate if masking generic protocol mode would
  corrupt non-ACP traffic rather than safely identifying ACP `initialize`.
- Dependency tolerance: stop and escalate before adding a new crate.
- Iteration tolerance: if the full gate stack still fails after three focused
  fix passes, document the blocker and escalate.

## Risks

- Risk: `ExecMode::Protocol` is generic, not ACP-specific. Blindly rewriting
  all protocol stdin would be too broad. Severity: high. Likelihood: high.
  Mitigation: only inspect the first newline-delimited frame and only rewrite a
  message that actually looks like ACP `initialize` with
  `params.clientCapabilities`.

- Risk: parsing and rewriting the first frame could break the byte-preserving
  proxy contract. Severity: medium. Likelihood: medium. Mitigation: limit the
  rewrite to the first frame only, preserve line endings, and leave malformed
  or non-ACP frames unchanged.

- Risk: feature-file edits for `rstest-bdd` are compile-time inputs, so stale
  generated code can make scenario matching look broken. Severity: medium.
  Likelihood: medium. Mitigation: keep scenario titles synchronized with the
  feature file and use a clean rebuild only if the generated bindings appear
  stale.

- Risk: adding ACP-specific coverage on top of the existing protocol proxy
  tests could duplicate too much helper code. Severity: low. Likelihood:
  medium. Mitigation: keep focused masking tests in
  `src/engine/connection/exec/protocol_acp_tests.rs`.

## Context and orientation

Relevant design and implementation anchors:

- `docs/podbot-roadmap.md` Step 2.6 defines ACP capability masking as the next
  container-engine integration task.
- `docs/podbot-design.md` already commits Podbot to masking ACP
  `terminal/*` and `fs/*` capabilities by default.
- `src/engine/connection/exec/protocol.rs` is the current library seam for
  protocol-safe stdin/stdout/stderr proxying.
- `src/engine/connection/exec/mod.rs` dispatches `ExecMode::Protocol` through
  that proxy and remains the single place that waits for daemon exit codes.
- `src/engine/connection/exec/attached.rs` and
  `src/engine/connection/exec/terminal.rs` own interactive terminal-only
  behaviour and must remain unchanged in semantics.

Before this change, the protocol stdin path was:

1. Wrap host stdin in a bounded `BufReader`.
2. Copy bytes directly into container stdin.
3. Flush and shut down container stdin.

After this change, the stdin path becomes:

1. Wrap host stdin in the existing bounded `BufReader`.
2. Read the first newline-delimited frame.
3. If it is an ACP `initialize` request with `clientCapabilities.terminal` or
   `clientCapabilities.fs`, rewrite that JSON frame with those keys removed.
4. Write the first frame to container stdin.
5. Resume the normal byte-transparent copy loop for the remainder of stdin.

This keeps the ACP-specific logic at the narrowest seam that can be tested
without waiting for the future `podbot host` command implementation.

## Agent team execution model

Use a small agent team so planning and implementation stay parallel but
reviewable.

Lane A (docs and roadmap reconnaissance owner):

- Read the roadmap, design, and testing guidance.
- Confirm what behaviour and documentation updates are required.

Lane B (code-path reconnaissance owner):

- Trace `ExecMode::Protocol`, terminal handling, resize propagation, and exit
  code reporting.
- Confirm where ACP masking can land without disturbing interactive sessions.

Lane C (primary implementation owner in the main thread):

- Draft this ExecPlan.
- Implement ACP masking at the protocol proxy seam.
- Add unit and behavioural coverage.
- Update documentation and run all gates.

Coordination rule:

- Use the reconnaissance lanes to reduce uncertainty up front, then keep all
  code edits and final integration in the main thread so the change remains
  coherent and atomic.

## Plan of work

### Stage A: Confirm the correct landing zone

Verify that the protocol proxy seam, not the interactive terminal path, is the
correct place to intercept ACP `initialize`.

Completed result:

- Confirmed `ExecMode::Protocol` dispatches through
  `src/engine/connection/exec/protocol.rs`.
- Confirmed attached-mode resize propagation and terminal handling are isolated
  in `attached.rs` and `terminal.rs`.
- Confirmed no ACP-specific logic existed anywhere else in the codebase.

### Stage B: Rewrite only ACP initialize capability metadata

Add a narrow helper pipeline in
`src/engine/connection/exec/acp_helpers.rs` that:

- reads the first newline-delimited frame from host stdin;
- parses it as JSON if possible;
- rewrites only ACP `initialize` requests that expose
  `params.clientCapabilities`;
- removes `terminal` and `fs` capability families;
- preserves unrelated capabilities and original line endings;
- forwards malformed or unrelated frames unchanged.

Completed result:

- Added ACP masking helpers in
  `src/engine/connection/exec/acp_helpers.rs`.
- Kept the rewrite on the first frame only, then resumed the existing raw
  proxy loop for the remainder of stdin.

### Stage C: Add focused unit coverage

Add `rstest` unit coverage at two levels:

- direct helper tests for frame masking and line-ending preservation in
  `src/engine/connection/exec/protocol_acp_tests.rs`;
- session-level proxy tests in
  `src/engine/connection/exec/protocol_acp_tests.rs`.

Coverage requirements:

- happy: ACP initialize removes `terminal` and `fs`;
- happy: unrelated capabilities remain;
- edge: empty `clientCapabilities` is removed after masking if nothing remains;
- edge: non-initialize and malformed messages are unchanged;
- happy: trailing protocol bytes after the masked frame remain unchanged;
- unhappy: input flush failures still surface correctly after ACP masking.

### Stage D: Add behavioural coverage with `rstest-bdd`

Add a dedicated ACP feature file and scenario bindings describing the operator-
visible behaviour in plain language.

Completed result:

- `tests/features/acp_capability_masking.feature` covers:
  - blocked-capability masking;
  - malformed initialize pass-through;
  - initialize without blocked capabilities remaining unchanged.
- `src/engine/connection/exec/protocol_acp_bdd_tests.rs` binds those scenarios
  via `rstest-bdd` v0.5.0.

### Stage E: Update documentation and roadmap state

Update:

- `docs/podbot-design.md` to record the first-frame rewrite decision and the
  unchanged pass-through rule for malformed or non-ACP frames;
- `docs/users-guide.md` to explain ACP masking within protocol mode;
- `docs/podbot-roadmap.md` to mark only the first Step 2.6 checkbox done.

### Stage F: Validate the complete change

Run the full gate stack with `tee` and `set -o pipefail`.

```shell
set -o pipefail
make fmt 2>&1 | tee /tmp/podbot-make-fmt.log
make markdownlint 2>&1 | tee /tmp/podbot-make-markdownlint.log
make nixie 2>&1 | tee /tmp/podbot-make-nixie.log
make check-fmt 2>&1 | tee /tmp/podbot-make-check-fmt.log
make lint 2>&1 | tee /tmp/podbot-make-lint.log
make test 2>&1 | tee /tmp/podbot-make-test.log
```

## Validation and acceptance

The task is complete only when all of the following are true:

- ACP `initialize` capability metadata is masked before reaching the
  containerized agent;
- malformed and non-ACP first frames remain unchanged;
- protocol stdout purity, non-TTY behaviour, and exit-code reporting remain
  intact;
- `rstest` and `rstest-bdd` cover happy, unhappy, and edge paths;
- `docs/podbot-design.md` and `docs/users-guide.md` document the final
  behaviour;
- only the relevant roadmap checkbox is marked done;
- the full documentation and Rust gate stack passes.

## Idempotence and recovery

- The masking logic is additive and local to
  `src/engine/connection/exec/acp_helpers.rs`, with regression coverage in
  `src/engine/connection/exec/protocol_acp_tests.rs`. If interrupted, revert
  only that seam rather than touching interactive exec code.
- If `.feature` edits appear to be ignored, do a clean rebuild before retrying
  tests.
- Keep future Step 2.6 work separate: runtime denylist and explicit delegation
  overrides should land in follow-on changes rather than stretching this one.

## Progress

- [x] (2026-04-21) Retrieved project memory, loaded repo guidance, and reviewed
  roadmap/design/testing documents.
- [x] (2026-04-21) Used a lane-based agent-team plan inside the ExecPlan to
  separate documentation, code-path, implementation, and validation work.
- [x] (2026-04-21) Confirmed `protocol.rs` is the correct seam for ACP
  initialization masking.
- [x] (2026-04-21) Implemented first-frame ACP masking helpers in
  `src/engine/connection/exec/acp_helpers.rs`.
- [x] (2026-04-21) Added focused `rstest` coverage in
  `src/engine/connection/exec/protocol_acp_tests.rs`.
- [x] (2026-04-21) Added `rstest-bdd` coverage in
  `tests/features/acp_capability_masking.feature` with bindings in
  `src/engine/connection/exec/protocol_acp_bdd_tests.rs`.
- [x] (2026-04-21) Updated `docs/podbot-design.md`, `docs/users-guide.md`, and
  `docs/podbot-roadmap.md`.
- [x] (2026-04-21) Ran `make fmt`, `make markdownlint`, `make nixie`,
  `make check-fmt`, `make lint`, and `make test` successfully.

## Surprises and discoveries

- Discovery: `rstest-bdd` feature files are compile-time inputs. Scenario-name
  mismatches can reflect stale generated code rather than wrong Rust bindings,
  so a clean rebuild is the recovery step if they ever diverge.
- Discovery: Step 2.6 can be delivered meaningfully before `podbot host`
  exists by treating the protocol proxy as the library-owned ACP seam.

## Decision log

- Decision: keep the ACP interception seam in
  `src/engine/connection/exec/protocol.rs` while moving masking helpers into
  `src/engine/connection/exec/acp_helpers.rs`, instead of waiting for
  `podbot host`. Rationale: the protocol proxy is the narrowest seam through
  which ACP bytes already flow, and the helper module keeps that seam fully
  testable without overloading the proxy loop.

- Decision: rewrite only the first newline-delimited frame. Rationale: ACP
  initialization occurs once at the start of the session, and limiting the
  parser scope preserves the existing raw-proxy contract for later traffic.

- Decision: treat masking failure as transparent pass-through, not a hard
  protocol error. Rationale: Step 2.6.1 is about removing known capabilities,
  not blocking or diagnosing later ACP misuse. A malformed first frame might
  belong to another protocol, and guessing would be riskier than forwarding it.

- Decision: remove `params.clientCapabilities` entirely when masking leaves it
  empty. Rationale: ACP treats omitted capabilities as unsupported, so dropping
  an empty object is semantically accurate and keeps the forwarded frame tidy.

- Decision: mark only the first Step 2.6 roadmap checkbox done. Rationale: the
  runtime denylist, stderr denials, and explicit override are still future work.

## Outcomes and retrospective

- Shipped: ACP capability masking now occurs before the container sees the
  hosted client's `initialize` request.
- Shipped: protocol sessions still preserve non-TTY execution, avoid resize
  handling, and report exit codes through the existing inspect loop.
- Shipped: happy, unhappy, and edge coverage now exists at both unit and
  behavioural levels.
- Documentation: the design document and users guide now describe the actual
  masking behaviour, including unchanged forwarding for malformed or unrelated
  first frames.
- Roadmap: the Step 2.6 checkbox for intercepting ACP initialization is now
  marked done, while later Step 2.6 work remains open.
- Gates passed: `make fmt`, `make markdownlint`, `make nixie`,
  `make check-fmt`, `make lint`, and `make test`.
- Remaining follow-up: implement the runtime denylist, protocol errors for
  blocked methods, and an explicit delegation override in subsequent Step 2.6
  changes.
