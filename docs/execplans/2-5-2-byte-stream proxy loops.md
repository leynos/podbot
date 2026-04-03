# Step 2.5.2: Byte-stream proxy loops

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises and discoveries`,
`Decision log`, and `Outcomes and retrospective` must be kept up to date as
work proceeds.

Status: COMPLETE (2026-03-29)

No `PLANS.md` file exists in this repository as of 2026-03-29, so this ExecPlan
is the governing implementation document for this task.

## Purpose and big picture

Complete roadmap task 2.5.2 from `docs/podbot-roadmap.md`: "Implement
byte-stream proxy loops: stdin -> container stdin, container stdout -> host
stdout, and container stderr -> host stderr."

The current `ExecMode::Protocol` work only guarantees `tty = false`. It still
reuses the interactive attached-session helper, which was written for terminal
operators rather than protocol transports. This plan makes protocol-mode exec
sessions behave like a strict stdio bridge suitable for app-server hosting:

- host stdin bytes are copied to container stdin without TTY framing;
- container stdout bytes are forwarded to host stdout unchanged;
- container stderr bytes are forwarded to host stderr unchanged;
- interactive-only behaviour such as local terminal echo and resize handling is
  kept out of protocol mode;
- daemon-reported exit codes remain accurate.

Observable success for this task:

- `ExecMode::Protocol` uses dedicated proxy-loop handling rather than relying
  on interactive terminal semantics;
- unit tests with `rstest` cover happy, unhappy, and edge paths for stdin,
  stdout, stderr, shutdown, and error propagation;
- behavioural tests with `rstest-bdd` v0.5.0 cover protocol proxy behaviour
  through the library seam;
- `docs/podbot-design.md` records the proxy-loop design decisions;
- `docs/users-guide.md` documents any observable behaviour change relevant to
  users or embedders;
- the Step 2.5.2 checkbox in `docs/podbot-roadmap.md` is marked done after all
  required gates pass.

This task does not complete all of Step 2.5. Bounded buffering policy,
`podbot host` stdout-purity enforcement, and lifecycle purity regression tests
remain tracked by later roadmap items, but this step must leave the code ready
for them.

## Constraints

- Keep scope to roadmap task 2.5.2 plus directly required tests and
  documentation. Do not implement the `podbot host` subcommand in this task.
- Preserve existing interactive attached-mode behaviour, including TTY-based
  resize handling and detached-mode exit-code semantics.
- Preserve the structural `tty = false` guarantee for `ExecMode::Protocol`.
- `src/engine/connection/exec/mod.rs` is already 363 lines and
  `src/engine/connection/exec/attached.rs` is already 316 lines, so new logic
  must be split into new modules or test submodules rather than growing those
  files indiscriminately.
- `tests/bdd_interactive_exec_helpers/steps.rs` is already 247 lines, so any
  major protocol additions should be split into helpers or a new feature file
  before the file becomes unwieldy.
- Every touched Rust module must retain a `//!` module-level comment.
- Use `rstest` fixtures and parameterized cases for unit tests.
- Use `rstest-bdd` v0.5.0 for behavioural tests, with
  `StepResult<T> = Result<T, String>`.
- Avoid `unwrap()` and `expect()` in production code.
- Use en-GB-oxendict spelling in documentation and comments.
- Run documentation gates after doc changes: `make fmt`,
  `make markdownlint`, and `make nixie`.
- Run the requested Rust gates before completion: `make check-fmt`,
  `make lint`, and `make test`.

## Tolerances (exception triggers)

- Scope tolerance: stop and escalate if the work needs more than 18 files or
  more than 700 net lines.
- Interface tolerance: stop and escalate if finishing this task requires a
  public API break beyond additive internal seams for testable IO proxying.
- Command-surface tolerance: stop and escalate if the work cannot be validated
  without adding `podbot host` prematurely.
- Dependency tolerance: stop and escalate before adding any new crate.
- Behaviour tolerance: stop and escalate if protocol proxy correctness appears
  to require changing interactive attached-mode output semantics.
- Iteration tolerance: if any gate still fails after three focused fix passes,
  stop, document the blocker, and escalate.

## Risks

- Risk: protocol mode currently shares the interactive helper that routes
  `LogOutput::StdIn` to host stdout to preserve terminal echo. That behaviour
  would contaminate protocol stdout. Severity: high. Likelihood: high.
  Mitigation: split protocol output handling from interactive echo handling and
  make `StdIn` handling explicit for protocol mode.

- Risk: the current attached helper hard-codes `tokio::io::stdin()`,
  `stdout()`, and `stderr()`, which makes stream-purity tests and unhappy-path
  IO tests awkward. Severity: high. Likelihood: high. Mitigation: introduce a
  narrow injected IO seam for proxy-loop tests and future `podbot host`
  integration.

- Risk: shutdown ordering between stdin forwarding, daemon output completion,
  and exit-code inspection can deadlock or truncate trailing bytes if handled
  informally. Severity: high. Likelihood: medium. Mitigation: define explicit
  task ownership and shutdown ordering in the proxy helper, then test EOF,
  writer failure, and stream-closure paths.

- Risk: file-size pressure in the exec module will turn a small protocol change
  into an oversized mixed-responsibility file. Severity: medium. Likelihood:
  high. Mitigation: extract a dedicated protocol proxy module and dedicated
  test submodule(s).

- Risk: there is no `podbot host` CLI path yet, so end-to-end purity can only
  be validated through library seams for now. Severity: medium. Likelihood:
  certain. Mitigation: validate host stdin/stdout/stderr behaviour through
  injected readers/writers now and leave CLI purity assertions to later roadmap
  items.

## Context and orientation

Current relevant implementation and design anchors:

- `docs/podbot-roadmap.md` Step 2.5 tracks protocol-safe execution and shows
  2.5.1 complete while 2.5.2 and later purity tasks remain open.
- `docs/podbot-design.md` already commits Podbot hosting mode to strict stdout
  purity, `tty = false`, stderr-only diagnostics, and bounded buffering.
- `docs/adr-002-define-the-hosted-session-api-and-control-channel.md` requires
  protocol IO to stay separate from diagnostics at the type level.
- `docs/mcp-server-hosting-design.md` reiterates that bridged stdio servers
  must preserve framing and keep stdout free of host-generated noise.

Current code shape:

- `src/engine/connection/exec/mod.rs` dispatches both `ExecMode::Attached` and
  `ExecMode::Protocol` through `run_attached_session_async(...)`.
- `src/engine/connection/exec/attached.rs` currently:
  - spawns a stdin-forwarding task using `tokio::io::stdin()`;
  - writes stdout/stderr directly to process stdio handles;
  - forwards `LogOutput::StdIn` to stdout to preserve interactive echo;
  - initializes resize handling conditionally via `request.tty()`.
- `src/engine/connection/exec/terminal.rs` already skips resize work when
  `tty()` is false, so protocol mode should continue to avoid `SIGWINCH`
  handling naturally.
- `src/main.rs` has no `host` subcommand yet; only `run`, `token-daemon`,
  `ps`, `stop`, and `exec` are wired.

Current test shape:

- `src/engine/connection/exec/tests/protocol_helpers.rs` proves TTY
  enforcement and basic exit-code behaviour, but not host-IO byte purity.
- `tests/features/interactive_exec.feature` and
  `tests/bdd_interactive_exec_helpers/` cover attached, detached, and basic
  protocol execution mode semantics, but not stdin/stdout/stderr proxy-loop
  behaviour.

The implementation should therefore target a library-level protocol proxy seam
first, with tests that simulate host stdio using injected readers/writers.

## Agent team execution model

Use a four-lane team during implementation so the work stays reviewable and the
proxy contract remains explicit.

Lane A (protocol exec core owner):

- Own the new protocol proxy module and the exec-mode dispatch updates.
- Keep interactive attached-mode behaviour stable.

Lane B (IO seam and shutdown owner):

- Own injected host-IO abstractions, task orchestration, and shutdown ordering.
- Own error mapping for stdin/stdout/stderr failures.

Lane C (test owner):

- Own `rstest` coverage for happy, unhappy, and edge cases.
- Own `rstest-bdd` scenario updates and helper-module extraction.

Lane D (docs and roadmap owner):

- Own `docs/podbot-design.md` and `docs/users-guide.md` updates.
- Mark only the Step 2.5.2 roadmap checkbox done after all gates pass.

Coordination rule:

- Merge Lane A first, then Lane B, then Lane C, then Lane D, replaying the
  full gate stack at the end.

## Plan of work

### Stage A: Split protocol proxying from interactive terminal attachment

Create a dedicated internal protocol proxy helper rather than adding more
branching to `attached.rs`.

Target shape:

- Keep the existing interactive helper focused on terminal-facing sessions.
- Add a new module such as `src/engine/connection/exec/protocol.rs` or a
  similarly named sibling focused on byte-preserving stdio proxying.
- Update `src/engine/connection/exec/mod.rs` so `ExecMode::Protocol` dispatches
  to the new protocol helper while `ExecMode::Attached` keeps using the
  interactive helper.

The protocol helper must not depend on terminal-only concepts such as local
echo or resize listeners.

### Stage B: Introduce a testable host-IO seam for proxy loops

Add a narrow internal abstraction that lets protocol proxy code operate on
injected host stdin/stdout/stderr handles instead of hard-coded process stdio.

Candidate approaches:

- an internal `ProtocolProxyIo` struct holding generic `AsyncRead`/`AsyncWrite`
  handles; or
- helper functions generic over reader/writer traits and called by a thin
  process-stdio wrapper.

Requirements for the seam:

- production code can still wire real process stdio cheaply;
- tests can inject in-memory readers/writers and failure-inducing doubles;
- the seam does not leak unnecessary public API surface.

### Stage C: Implement protocol byte-stream loops and shutdown ordering

Implement the three proxy loops required by the roadmap:

- host stdin -> container stdin;
- container stdout -> host stdout;
- container stderr -> host stderr.

Protocol-specific rules:

- keep `tty = false`;
- never register resize handling;
- never emit interactive echo bytes derived from `LogOutput::StdIn`;
- preserve daemon chunk ordering within each output stream;
- propagate read/write failures as `ContainerError::ExecFailed`.

Shutdown rules to make explicit in code and tests:

- host stdin EOF should flush and close container stdin cleanly;
- daemon output completion should not lose trailing bytes already received;
- exit-code inspection must still run after stream forwarding completes;
- stdin-forwarding task shutdown must not mask a prior output or daemon error.

For bounded buffering, prefer copy/write patterns that do not accumulate the
full stream in memory. This stage should leave the control flow compatible with
the later Step 2.5.3 buffering task, even if final buffer-policy tuning lands
there.

### Stage D: Wire protocol-mode exec orchestration and error mapping

Update engine orchestration so protocol-mode exec uses the new proxy helper and
still returns accurate exit codes.

Expected touchpoints:

- `src/engine/connection/exec/mod.rs`
- new internal protocol proxy module(s)
- possibly shared helpers extracted from `attached.rs` if they serve both paths

Behaviour to preserve:

- create/start exec options for protocol mode remain attached with
  `tty = false`;
- detached-mode mismatch checks remain intact;
- `wait_for_exit_code_async(...)` remains the single source of truth for
  reported exit codes.

### Stage E: Unit tests with `rstest`

Add dedicated protocol proxy-loop unit coverage, likely in a new test submodule
such as `src/engine/connection/exec/tests/proxy_helpers.rs`.

Required coverage:

- happy: protocol stdin bytes reach container stdin exactly;
- happy: container stdout bytes reach host stdout exactly;
- happy: container stderr bytes reach host stderr exactly;
- happy: interleaved stdout/stderr chunks are routed to the correct host
  streams;
- unhappy: host stdout write failure maps to exec failure;
- unhappy: host stderr write failure maps to exec failure;
- unhappy: stdin forwarding or container-input flush failure maps to exec
  failure;
- unhappy: daemon output stream error maps to exec failure;
- edge: `LogOutput::StdIn` does not contaminate host stdout in protocol mode;
- edge: resize calls are never attempted in protocol mode;
- edge: EOF on host stdin still permits clean exit-code capture.

Prefer small purpose-built writer doubles over large fixture scaffolding so the
tests stay readable and under the 400-line file budget.

### Stage F: Behavioural tests with `rstest-bdd` v0.5.0

Extend behavioural coverage so protocol-mode behaviour is described in scenario
terms, not only unit tests.

Options:

- extend `tests/features/interactive_exec.feature` and split helpers if size
  pressure increases; or
- introduce a dedicated protocol-exec feature file and helper directory if that
  gives cleaner isolation.

Required scenario coverage:

- protocol execution proxies stdout bytes to host stdout;
- protocol execution proxies stderr bytes to host stderr;
- protocol execution forwards stdin bytes to container stdin;
- protocol execution fails when the proxy loop encounters a write error;
- protocol execution does not enable resize handling or TTY.

Because `podbot host` does not yet exist, these scenarios should drive the
library seam with mocked exec client behaviour and injected host-IO doubles.

### Stage G: Documentation and roadmap updates

Update `docs/podbot-design.md` to record:

- the decision to split protocol proxying from interactive terminal handling;
- the rule that protocol mode never forwards interactive echo records to
  stdout;
- the shutdown-ordering and bounded-buffering rationale adopted here.

Update `docs/users-guide.md` for any user-visible or embedder-visible changes,
especially:

- that protocol mode is a raw non-TTY stdio bridge rather than a terminal
  session;
- that resize handling remains interactive-only;
- any error or shutdown behaviour users should expect when hosting protocols.

After all validation passes, mark only the Step 2.5.2 checkbox done in
`docs/podbot-roadmap.md`. Leave the remaining Step 2.5 items open.

## Validation and acceptance

Implementation is accepted only when all of the following are true:

- protocol-mode exec forwards stdin, stdout, and stderr through dedicated
  byte-stream proxy loops;
- protocol mode keeps `tty = false` and performs no resize calls;
- interactive attached mode still behaves as before;
- unit tests and behavioural tests cover happy, unhappy, and relevant edge
  cases;
- design and user documentation are updated to reflect the final behaviour;
- the Step 2.5.2 roadmap checkbox is marked done;
- the full gate stack passes.

Run gates with `tee` and `set -o pipefail` so failures are reviewable:

```shell
set -o pipefail
make fmt 2>&1 | tee /tmp/podbot-make-fmt.log
make markdownlint 2>&1 | tee /tmp/podbot-make-markdownlint.log
make nixie 2>&1 | tee /tmp/podbot-make-nixie.log
make check-fmt 2>&1 | tee /tmp/podbot-make-check-fmt.log
make lint 2>&1 | tee /tmp/podbot-make-lint.log
make test 2>&1 | tee /tmp/podbot-make-test.log
```

If any gate fails, fix the failure and rerun the affected gate until all six
pass.

## Idempotence and recovery

- Keep the new protocol proxy module additive until tests pass, so attached and
  detached execution remain bisectable.
- Update the roadmap checkbox only after docs, tests, and all gates succeed.
- If the implementation is interrupted after module extraction but before full
  wiring, leave the existing interactive path intact and document the partial
  state in `Progress`.
- If stream-purity bugs appear during implementation, prefer disabling only the
  new protocol branch temporarily rather than regressing attached-mode exec.

## Progress

- [x] (2026-03-29) Reviewed roadmap Step 2.5, design documents, existing exec
  implementation, and adjacent execplans; drafted this execution plan.
- [x] (2026-03-29) Stage A: split protocol proxying into
  `src/engine/connection/exec/protocol.rs` and kept interactive attachment in
  `attached.rs`.
- [x] (2026-03-29) Stage B: introduced `ProtocolProxyIo` so protocol sessions
  can run against injected host stdin/stdout/stderr handles in tests.
- [x] (2026-03-29) Stage C: implemented dedicated stdin/stdout/stderr proxy
  loops, ignored `LogOutput::StdIn` echo records in protocol mode, and made
  stdin-task shutdown explicit with a short completion grace period.
- [x] (2026-03-29) Stage D: wired `ExecMode::Protocol` through the dedicated
  protocol helper while preserving attached and detached semantics elsewhere.
- [x] (2026-03-29) Stage E: added `rstest` unit coverage for byte forwarding,
  stream routing, stdout/stderr write failures, stdin flush failure, daemon
  stream failure, `StdIn` suppression, and EOF shutdown.
- [x] (2026-03-29) Stage F: added `rstest-bdd` protocol proxy scenarios and a
  dedicated `tests/features/protocol_proxy.feature` feature file.
- [x] (2026-03-29) Stage G: updated design and user documentation, marked the
  Step 2.5.2 roadmap checkbox done, and replayed the full gate stack to green.

## Surprises and discoveries

- Discovery: protocol mode already exists, but it currently rides the same
  attached-session helper as interactive TTY sessions rather than a distinct
  protocol-safe proxy path.
- Discovery: the current attached helper forwards `LogOutput::StdIn` to host
  stdout for interactive echo, which is a direct stdout-purity hazard for
  protocol mode.
- Discovery: there is no `podbot host` CLI entry point yet, so Step 2.5.2 must
  be validated through library seams and tests rather than a user-facing host
  command.
- Discovery: waiting indefinitely for host stdin shutdown is unsafe in
  protocol mode because a hosted server may exit while the host side keeps
  stdin open. A short grace period before aborting the stdin-forwarding task
  captures EOF and flush failures without hanging shutdown.
- Discovery: `make fmt` depends on an `fd` executable that is not present on
  this machine's default `PATH`, so gate execution required a temporary local
  `fd` shim that delegates to `find`. The repository Make target itself was
  preserved unchanged.

## Decision log

- Decision: keep Step 2.5.2 scoped to the library/engine exec layer and do not
  pull `podbot host` command work forward. Rationale: the roadmap reserves host
  command wiring for later steps, and the protocol bridge can be validated now
  through injected IO seams.

- Decision: plan for a dedicated protocol proxy module instead of adding more
  branching inside `attached.rs`. Rationale: interactive and protocol sessions
  now have meaningfully different invariants, and the current file is already
  close to the repository's size guidance.

- Decision: treat `LogOutput::StdIn` as interactive-only and do not forward it
  to host stdout in protocol mode. Rationale: protocol mode must preserve host
  stdout as container protocol output, not local terminal echo.
- Decision: allow a short timeout while waiting for stdin forwarding to finish
  before aborting it during session teardown. Rationale: this preserves
  deterministic EOF and flush-failure reporting without allowing a live host
  stdin reader to block container-exit handling forever.

## Outcomes and retrospective

- Shipped: `ExecMode::Protocol` now dispatches through
  `src/engine/connection/exec/protocol.rs`, which implements dedicated proxy
  loops for host stdin -> container stdin, container stdout -> host stdout, and
  container stderr -> host stderr.
- Shipped: protocol mode now ignores daemon `LogOutput::StdIn` echo records,
  keeps resize handling out of the session path, and preserves exit-code
  reporting through the existing inspect loop.
- Shipped: injected `ProtocolProxyIo` test seams plus `rstest` and
  `rstest-bdd` coverage for forwarding, stream routing, failure mapping, and
  EOF shutdown behaviour.
- Documentation: updated `docs/podbot-design.md`,
  `docs/users-guide.md`, and `docs/podbot-roadmap.md` to describe the final
  protocol proxy behaviour and mark Step 2.5.2 complete.
- Gates passed: `make fmt`, `make markdownlint`, `make nixie`,
  `make check-fmt`, `make lint`, and `make test`.
- Deviation: behavioural coverage landed as an internal `rstest-bdd` module
  plus `tests/features/protocol_proxy.feature`, rather than extending the
  existing interactive exec BDD helpers with more injected-IO machinery.
- Follow-up: Step 2.5.3 and later roadmap items still need bounded buffering
  policy hardening, `podbot host` stdout-purity enforcement, and lifecycle
  stream-purity regression coverage at the CLI boundary.
