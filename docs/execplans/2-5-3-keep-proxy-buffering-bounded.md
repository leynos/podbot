# Step 2.5.3: Bounded proxy buffering and stream-purity enforcement

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises and discoveries`,
`Decision log`, and `Outcomes and retrospective` must be kept up to date as
work proceeds.

Status: COMPLETE

No `PLANS.md` file exists in this repository, so this ExecPlan is the governing
implementation document for this task.

## Purpose and big picture

Complete the remaining roadmap tasks for Step 2.5 from
`docs/podbot-roadmap.md`:

1. Keep proxy buffering bounded, so hosted protocols can apply backpressure.
2. Ensure `podbot host` emits no non-protocol bytes to stdout while proxying.
3. Add lifecycle stream-purity tests for startup, steady-state, shutdown, and
   error paths.
4. Add a regression test asserting zero stdout bytes before the first proxied
   protocol byte and after the final proxied byte.

The Step 2.5.2 work (completed in `2-5-2-byte-stream proxy loops.md`)
established a dedicated protocol proxy module
(`src/engine/connection/exec/protocol.rs`) with injected host-IO handles, byte-
stream proxy loops, `StdIn` echo suppression, and shutdown ordering. That
implementation relies on `tokio::io::copy()` for the stdin path and a per-chunk
`write_all()`/`flush()` loop for the output path. While these already avoid
accumulating the full stream in memory, the design document
(`docs/podbot-design.md`) explicitly promises _bounded buffering so backpressure
remains visible to the hosted server_. This step fulfils that promise and adds
the test coverage necessary to guard stream purity across full process lifecycle
transitions.

Observable success for this task:

- the stdin forwarding path uses an explicit bounded buffer rather than relying
  on `tokio::io::copy()`'s internal default, and the bound is documented;
- Bollard's `output_capacity` is set to a tuned value for protocol-mode exec
  sessions, bounding the daemon output stream's per-chunk memory consumption;
- `podbot host` stdout purity is architecturally enforced at the protocol proxy
  seam rather than assumed by convention;
- lifecycle stream-purity tests cover startup, steady-state, shutdown, and error
  paths through the library seam with injected host-IO doubles;
- a regression test asserts zero host stdout bytes before the first proxied
  protocol byte and after the final proxied byte;
- `docs/podbot-design.md` records the bounded-buffering decisions;
- `docs/users-guide.md` documents any observable behaviour change relevant to
  users or embedders;
- all four Step 2.5.3+ roadmap checkboxes are marked done after gates pass.

## Constraints

- Keep scope to the four remaining Step 2.5 roadmap tasks plus required tests
  and documentation.
- Preserve the existing interactive attached-mode behaviour intact.
- Preserve the existing protocol proxy byte-forwarding and shutdown semantics
  from Step 2.5.2.
- Do not implement the `podbot host` CLI subcommand in this step. The CLI
  command is tracked by Step 6.5 in the roadmap. Stdout-purity enforcement must
  operate at the protocol proxy library seam so it is testable without a CLI
  entry point.
- `src/engine/connection/exec/protocol.rs` is 223 lines and
  `src/engine/connection/exec/mod.rs` is 365 lines; avoid growing either beyond
  approximately 400 lines. Extract test helpers and new test modules instead.
- Every new or touched Rust module must retain a `//!` module-level comment.
- Use `rstest` fixtures and parameterized cases for unit tests.
- Use `rstest-bdd` v0.5.0 for behavioural tests, with
  `StepResult<T> = Result<T, String>`.
- Avoid `unwrap()` and `expect()` in production code.
- Use en-GB-oxendict spelling in documentation and comments.
- Run the requested Rust gates before completion: `make check-fmt`,
  `make lint`, and `make test`.

## Tolerances (exception triggers)

- Scope tolerance: stop and escalate if the work needs more than 15 new or
  modified files or more than 600 net lines.
- Interface tolerance: stop and escalate if finishing this task requires a
  public API break beyond additive internal seams.
- Command-surface tolerance: stop and escalate if stream-purity enforcement
  requires adding `podbot host` prematurely.
- Dependency tolerance: stop and escalate before adding any new crate.
- Behaviour tolerance: stop and escalate if bounded-buffering changes cause
  interactive attached-mode regression.
- Iteration tolerance: if any gate still fails after three focused fix passes,
  stop, document the blocker, and escalate.

## Risks

- Risk: replacing `tokio::io::copy()` with a bounded copy loop could change
  backpressure dynamics for stdin forwarding and break the existing EOF shutdown
  path. Severity: medium. Likelihood: medium. Mitigation: use
  `tokio::io::copy_buf()` with a `BufReader` of explicit capacity instead of
  hand-rolling a copy loop, or use `tokio::io::copy()` unchanged while wrapping
  the writer in a `BufWriter` with explicit capacity. Both preserve EOF
  semantics while making the buffer bound explicit.

- Risk: setting Bollard `output_capacity` too low could cause partial
  protocol messages to be split into multiple `LogOutput` chunks, increasing
  per-chunk overhead. Setting it too high defeats the backpressure goal.
  Severity: medium. Likelihood: low. Mitigation: choose a value that matches
  typical protocol message sizes (64 KiB is common for JSON Remote Procedure
  Call (JSON-RPC) frame buffers). Document the choice in the decision log and
  design document so it can be tuned later.

- Risk: lifecycle tests need to simulate multi-phase session execution (startup
  with no output, steady-state output, then shutdown) through the library seam.
  Constructing realistic multi-phase output streams may add test-helper
  complexity. Severity: low. Likelihood: medium. Mitigation: use `tokio::sync`
  channels or `futures_util::stream` combinators to create staged output streams
  that yield chunks in ordered phases.

- Risk: the "zero stdout bytes before/after proxied bytes" regression test must
  prove a negative (no bytes outside the proxy window). Severity: low.
  Likelihood: low. Mitigation: use `RecordingWriter` doubles that accumulate
  all bytes, run the proxy session with known input, and assert the captured
  bytes match the proxied output exactly with no prefix or suffix.

## Context and orientation

### Current implementation anchors

- `src/engine/connection/exec/protocol.rs` (223 lines): the dedicated protocol
  proxy module introduced in Step 2.5.2. Contains `ProtocolProxyIo`,
  `run_protocol_session_with_io_async()`, stdin forwarding, output loop, and
  chunk routing.

- `src/engine/connection/exec/mod.rs` (365 lines): dispatches
  `ExecMode::Protocol` to `run_protocol_session_async()` at line 282.
  `build_start_exec_options()` at line 320 currently sets
  `output_capacity: None`, which lets Bollard default to 8 KiB.

- `src/engine/connection/exec/tests/proxy_helpers/` (forwarding, routing,
  error_mapping submodules): existing `rstest` unit coverage for protocol proxy
  behaviour.

- `src/engine/connection/exec/tests/protocol_proxy_bdd.rs` (268 lines):
  `rstest-bdd` behavioural tests driven by
  `tests/features/protocol_proxy.feature`.

- `docs/podbot-design.md` line 153: promises "bounded buffering so backpressure
  remains visible to the hosted server".

### Current buffering behaviour

Stdin forwarding (`forward_host_stdin_to_exec_async` at protocol.rs:149) uses
`tokio::io::copy()`, which internally allocates a buffer (typically 8 KiB in
current Tokio versions) and calls `poll_read`/`poll_write` in a loop. This is
effectively bounded at runtime, but the bound is an implementation detail of
Tokio, not an explicit design contract.

Output chunks from Bollard arrive as `LogOutput` items from a `Stream`. The
current code calls `write_all()` + `flush()` for each chunk (protocol.rs:210-
221). This per-chunk flush provides natural backpressure: if the host stdout
writer blocks on flush, the output loop yields, and the Bollard stream stops
being polled. The Bollard `output_capacity` controls the maximum bytes per
`LogOutput` chunk. The default (8 KiB) means a 1 MiB protocol message arrives
as approximately 128 chunks. Setting a larger value reduces per-chunk overhead
for large messages while still keeping memory bounded per chunk.

### What "bounded buffering" means for this step

Making buffering explicitly bounded requires:

1. **Stdin path**: wrap the copy operation in an explicit bounded buffer so the
   buffer size is a documented constant rather than a Tokio implementation
   detail.

2. **Output path**: set Bollard `output_capacity` to an explicit value for
   protocol-mode exec sessions. The per-chunk `write_all()` + `flush()` pattern
   already provides backpressure; the missing piece is controlling the maximum
   chunk size the daemon can deliver.

3. **Documentation**: record buffer sizes and backpressure semantics in the
   design document so future maintainers can find and tune them.

## Agent team execution model

Use a four-lane team during implementation so the work stays reviewable and the
proxy contract remains explicit.

Lane A (bounded-buffering owner):

- Own the stdin buffer-size constant and `BufReader` or `BufWriter` wrapping.
- Own the Bollard `output_capacity` configuration for protocol mode.
- Own the design-document buffering updates.

Lane B (stream-purity enforcement owner):

- Own the architectural assertion that no non-protocol bytes reach host stdout
  in protocol mode.
- Own any structural changes needed to make stdout purity provable at the seam
  level.

Lane C (test owner):

- Own lifecycle stream-purity tests (startup, steady-state, shutdown, error).
- Own the regression test for zero stdout bytes before the first proxied byte
  and after the final proxied byte.
- Own `rstest` unit coverage for bounded-buffering behaviour.
- Own `rstest-bdd` scenario additions.

Lane D (docs and roadmap owner):

- Own `docs/podbot-design.md` and `docs/users-guide.md` updates.
- Mark all four Step 2.5 roadmap checkboxes done after all gates pass.

Coordination rule:

- Merge Lane A first, then Lane B, then Lane C, then Lane D, replaying the
  full gate stack at the end.

## Plan of work

### Stage A: Make stdin forwarding buffer explicitly bounded

Replace the implicit buffer in `tokio::io::copy()` with an explicitly sized
buffer by wrapping the host stdin reader in a `tokio::io::BufReader` with a
documented capacity constant before passing it to the copy operation.

Target changes:

- Add a constant `STDIN_BUFFER_CAPACITY: usize` in `protocol.rs` (suggested
  value: 64 KiB). Document the rationale in a comment: this bounds the maximum
  memory consumed by the stdin forwarding path per read cycle and provides
  backpressure by limiting how many bytes can be in flight between host stdin
  reads and container input writes.

- Modify `forward_host_stdin_to_exec_async()` to wrap `host_stdin` in
  `tokio::io::BufReader::with_capacity(STDIN_BUFFER_CAPACITY, host_stdin)`
  before calling `tokio::io::copy()`. The existing `copy()` call remains
  unchanged; the explicit buffer replaces the internal default.

- Add a corresponding `OUTPUT_BUFFER_CAPACITY: usize` constant and wrap the
  container input writer in `tokio::io::BufWriter::with_capacity()` so the
  write side is also explicitly bounded.

### Stage B: Set Bollard `output_capacity` for protocol-mode exec

Set Bollard's `output_capacity` in `build_start_exec_options()` to an explicit
value for `ExecMode::Protocol` sessions, bounding the maximum bytes per
`LogOutput` chunk from the daemon.

Target changes:

- Add a constant `PROTOCOL_OUTPUT_CAPACITY: usize` in the exec module
  (suggested value: 65_536 / 64 KiB). This matches common JSON-RPC frame
  buffer sizes and keeps per-chunk memory bounded while reducing overhead
  compared to the 8 KiB default for large protocol messages.

- Modify `build_start_exec_options()` to set `output_capacity` conditionally:
  - `ExecMode::Protocol`: `Some(PROTOCOL_OUTPUT_CAPACITY)`
  - `ExecMode::Attached` and `ExecMode::Detached`: `None` (preserve current
    behaviour).

- Update the existing protocol-mode start-options unit test in
  `protocol_helpers.rs` to assert the new `output_capacity` value.

### Stage C: Enforce stdout purity at the protocol proxy seam

Ensure that the protocol proxy path architecturally prevents non-protocol bytes
from reaching host stdout. The current implementation already achieves this by
construction:

- `run_protocol_session_with_io_async()` accepts injected host-IO handles and
  only writes to `host_stdout` in `handle_log_output_chunk()` for
  `LogOutput::StdOut` and `LogOutput::Console` messages from the container.
- `LogOutput::StdIn` echo records are silently dropped.
- No banner, progress, or diagnostic text is written to `host_stdout` anywhere
  in the protocol proxy path.

The enforcement step for this task is to add an explicit documentation-level
assertion in the code (a module-level comment expansion in `protocol.rs`) and a
test-level assertion (the Stage E regression test) that together make the purity
contract provable.

Target changes:

- Expand the `//!` module-level comment in `protocol.rs` to explicitly state
  the stdout-purity contract: "The protocol proxy must never write bytes to host
  stdout that did not originate from container stdout or console output. This
  means no banners, no progress indicators, no diagnostic messages, and no
  echoed stdin bytes."

- No structural code changes are needed for purity enforcement itself. The
  existing `handle_log_output_chunk()` routing already guarantees this by
  construction, and Step 2.5.2 added `LogOutput::StdIn` suppression. The test
  coverage in Stages D and E makes this contract regression-proof.

### Stage D: Add lifecycle stream-purity tests

Add unit tests covering protocol proxy stream purity across four lifecycle
phases: startup, steady-state, shutdown, and error paths.

Target: create a new test submodule
`src/engine/connection/exec/tests/proxy_helpers/lifecycle_purity.rs`.

Required test cases (unit tests with `rstest`):

1. **Startup purity**: run a protocol proxy session where the container emits a
   single stdout chunk. Assert that host stdout contains exactly those bytes and
   nothing else (no prefix bytes from session setup).

2. **Steady-state purity**: run a protocol proxy session with multiple
   interleaved stdout, stderr, and `StdIn` chunks. Assert that host stdout
   contains only the concatenated stdout and console bytes in order, host stderr
   contains only stderr bytes, and no `StdIn` bytes leak to either.

3. **Shutdown purity**: run a protocol proxy session where the output stream
   ends (daemon closes the stream) after delivering chunks. Assert that host
   stdout contains exactly the proxied bytes with no trailing bytes added by
   shutdown logic.

4. **Error-path purity**: run a protocol proxy session where the output stream
   fails midway (daemon stream error after one successful chunk). Assert that
   host stdout contains only the bytes from the chunk that succeeded, with no
   error-related bytes written to stdout. The error should surface as a
   `PodbotError`.

Required behaviour-driven development (BDD) test scenarios (add to
`tests/features/protocol_proxy.feature`):

1. **Lifecycle purity scenario**: protocol proxy delivers only container output
   bytes to host stdout during a complete startup-to-shutdown lifecycle.

2. **Error purity scenario**: protocol proxy fails without contaminating host
   stdout when the daemon stream errors.

### Stage E: Add regression test for zero stdout bytes before/after proxied bytes

Add a focused regression test that asserts zero host stdout bytes before the
first proxied protocol byte and after the final proxied byte.

Target: add to the `lifecycle_purity.rs` test submodule.

Test design:

- Construct a protocol proxy session with a known output: one stdout chunk
  containing exactly `b"PROTOCOL_OUTPUT"`.
- Run the session with `RecordingWriter` doubles.
- After the session completes, assert:
  - host stdout bytes equal exactly `b"PROTOCOL_OUTPUT"` (no prefix bytes, no
    suffix bytes);
  - no bytes were written to host stdout before the chunk arrived (the
    `RecordingWriter` captures all writes atomically, so the total captured
    bytes must match the known output);
  - no bytes were written to host stdout after the chunk was delivered (the
    total must still match).

This test serves as a regression guard for the stdout-purity contract stated in
`docs/podbot-design.md` and prevents future code from accidentally adding
banners, diagnostics, or framing bytes to the protocol stdout path.

### Stage F: Documentation and roadmap updates

Update `docs/podbot-design.md` to record:

- the bounded-buffering constants chosen for stdin forwarding
  (`STDIN_BUFFER_CAPACITY`) and output chunk size
  (`PROTOCOL_OUTPUT_CAPACITY`);
- the rationale for each buffer size;
- confirmation that per-chunk `write_all()` + `flush()` provides the
  backpressure contract for the output path;
- confirmation that the stdin path uses an explicitly bounded reader buffer.

Update `docs/users-guide.md` to document:

- that protocol mode uses bounded buffering so hosted protocols can apply
  backpressure;
- any observable behaviour difference from the buffering changes (in practice
  there should be none for users, since the underlying semantics are
  preserved, but the section on protocol mode execution behaviour should
  mention bounded buffering).

After all gates pass, mark the four remaining Step 2.5 roadmap checkboxes done
in `docs/podbot-roadmap.md`.

## Validation and acceptance

Implementation is accepted only when all of the following are true:

- stdin forwarding uses an explicitly bounded buffer with a documented capacity
  constant;
- Bollard `output_capacity` is set to an explicit value for protocol-mode
  exec sessions;
- lifecycle stream-purity tests cover startup, steady-state, shutdown, and
  error paths;
- a regression test asserts zero stdout bytes before/after proxied bytes;
- the `//!` module comment in `protocol.rs` states the stdout-purity contract;
- design and user documentation are updated;
- the four remaining Step 2.5 roadmap checkboxes are marked done;
- the full gate stack passes.

Run gates with `tee` and `set -o pipefail` so failures are reviewable:

```shell
set -o pipefail
make check-fmt 2>&1 | tee /tmp/podbot-make-check-fmt.log
make lint 2>&1 | tee /tmp/podbot-make-lint.log
make test 2>&1 | tee /tmp/podbot-make-test.log
```

If any gate fails, fix the failure and rerun the affected gate until all three
pass.

## Idempotence and recovery

- Keep bounded-buffering changes additive until tests pass, so the existing
  protocol proxy behaviour remains bisectable.
- If interrupted after Stage A but before Stage D, the protocol proxy will have
  explicit buffer bounds but not yet have lifecycle tests. The existing test
  coverage from Step 2.5.2 still protects basic functionality.
- Update roadmap checkboxes only after docs, tests, and all gates succeed.
- If lifecycle tests reveal unexpected purity violations, prefer fixing the
  source of the violation rather than weakening the test assertion.

## Progress

- [x] Reviewed roadmap Step 2.5, design documents, existing exec
  implementation, previous execplan 2.5.2, and existing test coverage; drafted
  this execution plan.
- [x] Stage A: make stdin forwarding buffer explicitly bounded.
- [x] Stage B: set Bollard `output_capacity` for protocol-mode exec.
- [x] Stage C: enforce stdout purity at the protocol proxy seam.
- [x] Stage D: add lifecycle stream-purity tests.
- [x] Stage E: add regression test for zero stdout bytes before/after proxied
  bytes.
- [x] Stage F: documentation and roadmap updates.

## Surprises and discoveries

- The existing `tokio::io::copy()` implementation was already effectively
  bounded at runtime through Tokio's internal buffer, but the bound was an
  implementation detail rather than an explicit contract. Making it explicit
  required wrapping both the reader and writer in `BufReader` and `BufWriter`
  with documented capacity constants.

- The lifecycle purity tests revealed that the test helper functions needed to
  accept `RuntimeFixture` directly rather than unwrapping it, as the fixture
  type is already a `Result`. This pattern maintains consistency with other
  test helpers in the codebase.

- The BDD step definitions required careful naming to avoid ambiguity in the
  rstest-bdd framework. The step "host stdout receives {text1} followed by
  {text2}" conflicted with "host stdout receives {text}", requiring a rename to
  "host stdout concatenates {text1} and {text2}" to make the pattern distinct.

## Decision log

### 2026-04-07: Chose 64 KiB for all buffer constants

Set `STDIN_BUFFER_CAPACITY`, `OUTPUT_BUFFER_CAPACITY`, and
`PROTOCOL_OUTPUT_CAPACITY` to 65,536 bytes (64 KiB). Rationale:

- Aligns with common protocol message sizes (JSON-RPC frame buffers typically
  use 64 KiB).
- Matches typical OS pipe buffer defaults on Linux and macOS.
- Provides a good balance between latency (small enough) and throughput
  (large enough to amortize syscall overhead).
- Keeps all three constants symmetrical for consistent backpressure behaviour.

**2026-04-07: Made `build_start_exec_options` const**

Changed the function signature from `fn` to `const fn` to satisfy clippy's
`missing_const_for_fn` lint. The function body already supported const
evaluation (simple match and struct construction), so this was a zero-cost
improvement to enable compile-time evaluation where possible.

**2026-04-07: Unit tests do not return `Result`**

Converted lifecycle purity tests from returning `TestResult` to returning `()`
to satisfy clippy's `panic_in_result_fn` and `unnecessary_wraps` lints. Since
the tests use `assert!` and `assert_eq!` macros which panic on failure, there's
no need for `Result`-based error propagation. This pattern matches the existing
test style in the codebase.

## Outcomes and retrospective

All six stages completed successfully with all gates passing:

- **Stage A** added explicit 64 KiB bounded buffers for stdin forwarding,
  replacing the implicit Tokio internal buffer with documented capacity
  constants.
- **Stage B** set Bollard's `output_capacity` to 64 KiB for protocol-mode exec
  sessions, bounding the maximum bytes per `LogOutput` chunk from the daemon.
- **Stage C** documented the stdout-purity contract at the module level,
  establishing an explicit architectural guarantee that no non-protocol bytes
  reach host stdout.
- **Stage D** added lifecycle stream-purity unit tests covering startup,
  steady-state, shutdown, and error paths, plus two new BDD scenarios for
  lifecycle purity and error purity.
- **Stage E** added a regression test asserting zero stdout bytes before the
  first proxied protocol byte and after the final proxied byte, guarding the
  purity contract against future regressions.
- **Stage F** updated `docs/podbot-design.md` with a detailed "Bounded
  buffering implementation" section, updated `docs/users-guide.md` to document
  the bounded-buffering behaviour, and marked all four remaining Step 2.5
  roadmap tasks as complete.

The implementation required 7 modified files (protocol.rs, mod.rs,
protocol_helpers.rs, lifecycle_purity.rs, protocol_proxy_bdd.rs,
protocol_proxy.feature, steps.rs) and stayed well within the 15-file and
600-line tolerances. No API breaks were introduced. All existing tests
continued to pass after updating expectations for the new `output_capacity`
value.

The bounded-buffering changes are additive and do not alter the observable
behaviour of the protocol proxy beyond making buffer sizes explicit and
tunable. Backpressure semantics remain unchanged: if the host stdout writer
blocks on flush, the output loop yields, the Bollard stream stops being polled,
and backpressure propagates to the container.
