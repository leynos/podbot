# Support both Unix sockets and TCP connections (phase 2.1.4)

This ExecPlan is a living document. The sections Constraints, Tolerances,
Risks, Progress, Surprises and Discoveries, Decision Log, and Outcomes and
Retrospective must be kept up to date as work proceeds.

Status: COMPLETE

This document must be maintained in accordance with `AGENTS.md` at the
repository root.

## Purpose / big picture

This is the final task in Phase 2.1 (Engine Connection) of the podbot roadmap.
After this change, the `EngineConnector` has comprehensive test coverage for
TCP endpoint handling alongside the existing Unix socket support. A reader of
the user's guide can see that podbot supports `tcp://`, `http://`, and
`https://` endpoints in addition to `unix://` sockets and `npipe://` named
pipes. The test suite exercises TCP paths through socket classification,
scheme rewriting, socket resolution, fallback behaviour, and error
classification.

Running `make check-fmt && make lint && make test` passes with all 190 tests
green. The roadmap entry is marked complete and Step 2.1 is fully done.

## Constraints

- No changes to the core connection logic in
  `src/engine/connection/mod.rs` (the `connect()` method, `SocketType` enum,
  and supporting functions). The implementation is already correct.
- No changes to `src/engine/connection/error_classification.rs` logic. The
  classification correctly returns `None` from `extract_socket_path` for
  TCP/HTTP URIs, falling back to `ConnectionFailed`.
- Public interfaces (`EngineConnector`, `SocketResolver`, `SocketPath`) remain
  stable.
- Files must remain under 400 lines per `AGENTS.md`.
- en-GB-oxendict spelling and grammar in all documentation and comments.
- `make check-fmt`, `make lint`, `make test` must all pass.
- No `unwrap` or `expect` in production code.
- Environment variable mocking uses the `mockable` crate exclusively.

## Tolerances (exception triggers)

- Scope: if implementation requires changes to more than 12 files or 400 lines
  of code (net), stop and escalate.
- Interface: if a public API signature must change, stop and escalate.
- Dependencies: the only permitted dependency change is upgrading rstest-bdd
  and rstest-bdd-macros from 0.4.0 to 0.5.0.
- Iterations: if tests still fail after 3 attempts at a fix, stop and
  escalate.

## Risks

- Risk: rstest-bdd 0.5.0 may have breaking changes from 0.4.0.
  Severity: medium. Likelihood: low. Mitigation: upgrade first as Task 1;
  the upgrade compiled cleanly with no code changes required.

- Risk: Bollard's `connect_with_http` may change behaviour in future versions.
  Severity: low. Likelihood: low. Mitigation: tests document this assumption
  with comments.

## Progress

- [x] (2026-02-09) Upgrade rstest-bdd to 0.5.0
- [x] (2026-02-09) Create TCP unit tests in `tests_tcp.rs` submodule
- [x] (2026-02-09) Add TCP error classification test cases
- [x] (2026-02-09) Add 4 behaviour-driven development (BDD) scenarios
  for TCP endpoints
- [x] (2026-02-09) Create TCP BDD step definitions module
- [x] (2026-02-09) Add BDD scenario bindings
- [x] (2026-02-09) Update user documentation with TCP examples
- [x] (2026-02-09) Record design decisions in design document
- [x] (2026-02-09) Mark roadmap entry as complete
- [x] (2026-02-09) Run quality gates (all 190 tests pass)

## Surprises and discoveries

- Observation: The `tests.rs` file uses a `#[cfg(test)] mod tests;` pattern
  from `mod.rs`, which means submodules declared inside `tests.rs` look for
  files at `src/engine/connection/tests/tests_tcp.rs` rather than
  `src/engine/connection/tests_tcp.rs`.
  Evidence: Compilation error `file not found for module tests_tcp`.
  Impact: Used `#[path = "tests_tcp.rs"] mod tcp;` attribute to redirect the
  module path to the sibling file.

- Observation: The `expect_used` clippy lint is set to `deny` globally,
  including test code outside `#[cfg(test)]` modules (such as integration test
  files).
  Evidence: Clippy rejected `result.expect_err("already matched Err")` in the
  TCP BDD step definitions.
  Impact: Refactored to use `ref container_err @` binding patterns to access
  inner error fields without calling `expect_err`, matching the pattern
  established in `permission_error_steps.rs`.

## Decision log

- Decision: Split TCP tests into a submodule (`tests_tcp.rs`) rather than
  keeping them inline in `tests.rs`.
  Rationale: `tests.rs` was 366 lines; adding 70+ lines of TCP tests would
  exceed the 400-line limit. The `#[path]` attribute keeps the submodule file
  as a sibling rather than requiring a directory restructure.
  Date: 2026-02-09

- Decision: TCP BDD step definitions go in a new
  `tcp_connection_steps.rs` module rather than being added to the existing
  permission error steps.
  Rationale: TCP connection testing has distinct concerns (lazy connection
  semantics, health check failure classification) that warrant a separate
  module for clarity and maintainability.
  Date: 2026-02-09

- Decision: Use RFC 5737 documentation-reserved IP address (`192.0.2.1`)
  for the TCP endpoint that should fail health check.
  Rationale: This IP range (192.0.2.0/24) is reserved by IANA for
  documentation and will never route to a real service, making the test
  deterministic without risk of accidental connection.
  Date: 2026-02-09

- Decision: No changes to core connection logic.
  Rationale: The `EngineConnector::connect()` method already correctly
  handles Unix sockets, named pipes, TCP (via http rewriting), HTTP, and
  HTTPS endpoints. The `SocketType` enum classifies all schemes. Error
  classification correctly returns `ConnectionFailed` for TCP endpoints.
  The remaining work was testing, documentation, and dependency upgrade.
  Date: 2026-02-09

## Outcomes and retrospective

All objectives achieved. The task required no changes to production code
beyond the rstest-bdd version upgrade in `Cargo.toml`. The work was entirely
additive: new test files, new BDD scenarios, and documentation updates. This
confirms that the original `EngineConnector::connect()` implementation was
already complete for TCP support; what was missing was validation and
documentation.

The rstest-bdd 0.5.0 upgrade was seamless with no breaking changes. The
`#[path]` attribute workaround for the test submodule is slightly unusual but
well-supported by Rust and avoids a more invasive directory restructure.

______________________________________________________________________

## Context and orientation

The podbot project is a sandboxed execution environment for AI coding agents.
It connects to Docker or Podman container engines via the Bollard library. The
connection module lives at `src/engine/connection/` and contains:

- `mod.rs` (339 lines): Core types (`SocketPath`, `SocketResolver`,
  `SocketType`, `EngineConnector`) and the `connect()` method which dispatches
  to `Docker::connect_with_socket()` for Unix/named-pipe endpoints or
  `Docker::connect_with_http()` for TCP/HTTP/HTTPS endpoints. TCP URIs
  (`tcp://`) are rewritten to `http://` before passing to Bollard.
- `error_classification.rs`: Classifies Bollard errors into semantic
  `ContainerError` variants. Returns `None` from `extract_socket_path()` for
  TCP/HTTP URIs, causing errors to fall back to `ConnectionFailed`.
- `health_check.rs`: Health check and connect-and-verify operations.
- `tests.rs`: Unit tests for socket resolution, with TCP tests split into
  `tests_tcp.rs` submodule.

BDD tests live at `tests/bdd_engine_connection.rs` with helpers in
`tests/bdd_engine_connection_helpers/`.

Key design insight: `Docker::connect_with_http()` (used for TCP endpoints) is
lazy. It creates the client configuration synchronously without validating
connectivity. Failures surface only during the first API call (typically the
health check ping). This is fundamentally different from Unix sockets, where
`Docker::connect_with_socket()` for a nonexistent path fails immediately.

### Image versioning strategy

Sandbox images consumed by `EngineConnector` should use immutable digests
(e.g. `podbot-sandbox@sha256:abc123…`) in production to guarantee
reproducibility. Mutable tags (e.g. `podbot-sandbox:latest`) are acceptable
in development and testing environments.

The expected update cadence is: weekly base-image security patches, ad-hoc
updates for critical vulnerabilities, and quarterly feature releases. To
roll back, reference an earlier digest or tagged release; CI/CD pipelines
should retain at least three prior digests so that rollback is immediate.

Image version metadata is recorded as OCI container labels (accessible via
`docker inspect` or `podman inspect`) and summarized in the project
`README.md`. This ensures `EngineConnector` consumers can verify the
running image version programmatically.

## Plan of work

### Task 1: Upgrade rstest-bdd to 0.5.0

**File:** `Cargo.toml`

Change rstest-bdd and rstest-bdd-macros versions from `"0.4.0"` to `"0.5.0"`.

### Task 2: Create TCP unit tests

**Files:** `src/engine/connection/tests.rs`, `src/engine/connection/tests_tcp.rs`
(new)

Move the two existing TCP tests (`connect_tcp_endpoint_creates_client` and
`connect_tcp_endpoint_with_ip_creates_client`) from `tests.rs` to a new
`tests_tcp.rs` submodule, included via `#[path = "tests_tcp.rs"] mod tcp;`.
Make the `env_with_docker_host` helper `pub(super)` for submodule access.

Add new tests:

- `connect_http_compatible_endpoints_creates_client` — parameterized with
  http, https, tcp hostname, and tcp IPv4 cases
- `connect_tcp_rewrites_scheme_to_http` — verifies tcp-to-http rewriting
- `resolver_returns_tcp_endpoint_from_docker_host` — resolver with TCP value
- `resolve_socket_uses_tcp_endpoint_from_config` — config resolution
- `resolve_socket_uses_tcp_endpoint_from_env` — env resolution
- `connect_with_fallback_uses_tcp_from_config` — fallback with TCP config
- `connect_with_fallback_uses_tcp_from_env` — fallback with TCP env

### Task 3: Add TCP error classification test cases

**File:** `src/engine/connection/error_classification.rs`

Add to existing parameterized tests:

- `extract_socket_path_parses_correctly`: add `https` case (returns `None`)
- `classify_connection_error_falls_back_for_unmapped_or_non_socket_context`:
  add `not_found_tcp` and `permission_denied_tcp` cases (both produce
  `ConnectionFailed`)

### Task 4: Add BDD scenarios

**File:** `tests/features/engine_connection.feature`

Add four scenarios:

- "TCP endpoint resolved from DOCKER_HOST" — reuses existing step definitions
- "Config socket as TCP endpoint takes precedence" — reuses existing steps
- "TCP endpoint connection succeeds without daemon" — new TCP steps
- "TCP connection errors are classified as connection failures" — new TCP steps

### Task 5: Create TCP BDD step definitions

**File:** `tests/bdd_engine_connection_helpers/tcp_connection_steps.rs` (new)

Step definitions:

- `given("a TCP endpoint is configured")` — sets `tcp://localhost:2375`
- `given("a TCP endpoint that will fail health check")` — sets
  `tcp://192.0.2.1:2375` (RFC 5737 reserved)
- `when("a TCP connection is attempted")` — connect only, no health check
- `when("a TCP connection with health check is attempted")` — connect and
  verify, classifying errors
- `then("the connection client is created successfully")` — asserts `Success`
- `then("a connection failure error is returned")` — asserts `OtherError`,
  rejects `SocketNotFound` and `PermissionDenied`

### Task 6: Add scenario bindings

**File:** `tests/bdd_engine_connection.rs`

Add four `#[scenario]` bindings for the new TCP scenarios.

### Task 7: Update user documentation

**File:** `docs/users-guide.md`

Add "TCP endpoint support" section documenting supported formats (table),
configuration examples (CLI, env var, config file), TCP-specific
troubleshooting (table), and a security note about unencrypted connections.
Update the config file example to show a TCP alternative.

### Task 8: Record design decisions

**File:** `docs/podbot-design.md`

Add "Engine connection protocol support" section documenting:

- Supported protocols table
- TCP-to-HTTP rewriting rationale
- Lazy versus eager connection semantics
- Bare path normalization behaviour

### Task 9: Mark roadmap entry

**File:** `docs/podbot-roadmap.md`

Mark "Support both Unix sockets and TCP connections" as `[x]`. Add completion
marker to Step 2.1 heading.

______________________________________________________________________

## Concrete steps

All commands run from `/home/user/project`.

1. Edit `Cargo.toml`: upgrade rstest-bdd 0.4.0 → 0.5.0.
2. Run `cargo check --tests` to verify compilation.
3. Create `src/engine/connection/tests_tcp.rs` with TCP tests.
4. Edit `src/engine/connection/tests.rs`: move TCP tests out, add
   `#[path = "tests_tcp.rs"] mod tcp;`, make `env_with_docker_host`
   `pub(super)`.
5. Add test cases to `src/engine/connection/error_classification.rs`.
6. Add scenarios to `tests/features/engine_connection.feature`.
7. Create `tests/bdd_engine_connection_helpers/tcp_connection_steps.rs`.
8. Update `tests/bdd_engine_connection_helpers/mod.rs` with module and
   re-export.
9. Add bindings to `tests/bdd_engine_connection.rs`.
10. Update `docs/users-guide.md` with TCP section.
11. Update `docs/podbot-design.md` with protocol support section.
12. Update `docs/podbot-roadmap.md` to mark complete.
13. Run quality gates:

```bash
set -o pipefail
make check-fmt 2>&1 | tee /tmp/fmt.log && echo "FMT OK"
make lint 2>&1 | tee /tmp/lint.log && echo "LINT OK"
make test 2>&1 | tee /tmp/test.log && echo "TEST OK"
```

## Validation and acceptance

Run `make test` and expect all 190 tests to pass. New tests include:

**Unit tests (in `tests_tcp.rs`):**

- `connect_tcp_endpoint_creates_client`
- `connect_tcp_endpoint_with_ip_creates_client`
- `connect_http_compatible_endpoints_creates_client` (4 cases)
- `connect_tcp_rewrites_scheme_to_http`
- `resolver_returns_tcp_endpoint_from_docker_host`
- `resolve_socket_uses_tcp_endpoint_from_config`
- `resolve_socket_uses_tcp_endpoint_from_env`
- `connect_with_fallback_uses_tcp_from_config`
- `connect_with_fallback_uses_tcp_from_env`

**Error classification tests (in `error_classification.rs`):**

- `extract_socket_path_parses_correctly::case_6_https`
- `classify_connection_error_falls_back...::case_3_not_found_tcp`
- `classify_connection_error_falls_back...::case_4_permission_denied_tcp`

**BDD scenarios:**

- `tcp_endpoint_from_docker_host`
- `config_tcp_takes_precedence`
- `tcp_connection_succeeds`
- `tcp_connection_errors_classified`

Quality criteria:

- `make check-fmt` passes
- `make lint` passes (zero clippy warnings)
- `make test` passes (190 tests, 0 failures)
- No file exceeds 400 lines
- Roadmap task marked `[x]`

Quality method:

```bash
set -o pipefail
make check-fmt 2>&1 | tee /tmp/fmt.log && echo "FMT OK"
make lint 2>&1 | tee /tmp/lint.log && echo "LINT OK"
make test 2>&1 | tee /tmp/test.log && echo "TEST OK"
```

## Idempotence and recovery

All steps are idempotent. Test file additions are additive. Documentation
changes are additive. The quality gate commands can be re-run at any time.

## Artifacts and notes

**Files created:**

| File | Purpose |
| ---- | ------- |
| `src/engine/connection/tests_tcp.rs` | TCP unit tests (12 tests) |
| `tests/bdd_engine_connection_helpers/tcp_connection_steps.rs` | TCP BDD step definitions |
| `docs/execplans/2-1-4-support-both-unix-sockets-and-tcp-connections.md` | This execution plan |

_Table 1: Files created by this task._

**Files modified:**

| File | Change |
| ---- | ------ |
| `Cargo.toml` | Upgrade rstest-bdd 0.4.0 → 0.5.0 |
| `src/engine/connection/tests.rs` | Move TCP tests out, add `mod tcp`, make helper `pub(super)` |
| `src/engine/connection/error_classification.rs` | Add 3 TCP test cases |
| `tests/features/engine_connection.feature` | Add 4 TCP scenarios |
| `tests/bdd_engine_connection_helpers/mod.rs` | Add tcp_connection_steps module |
| `tests/bdd_engine_connection.rs` | Add 4 scenario bindings |
| `docs/users-guide.md` | Add TCP endpoint support section |
| `docs/podbot-design.md` | Add engine connection protocol support section |
| `docs/podbot-roadmap.md` | Mark task and step complete |

_Table 2: Files modified by this task._

## Interfaces and dependencies

No new public interfaces introduced. No new dependencies beyond the rstest-bdd
version upgrade. Existing public API unchanged:

- `EngineConnector::connect(socket: impl AsRef<str>)
  -> Result<Docker, PodbotError>`
- `EngineConnector::connect_with_fallback(
  config_socket, resolver)
  -> Result<Docker, PodbotError>`
- `EngineConnector::connect_and_verify_async(socket)
  -> Result<Docker, PodbotError>`
- `podbot::engine::SocketResolver::resolve_from_env() -> Option<String>`

______________________________________________________________________

## Final status: complete

All tasks completed successfully. Quality gates passed:

- `make check-fmt` ✓
- `make lint` ✓
- `make test` ✓ (all 190 tests pass)
