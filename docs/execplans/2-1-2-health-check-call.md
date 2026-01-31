# Execplan: Health check that verifies the engine responds

**Task:** Add a health check that verifies the container engine responds after
connection.

**Phase:** 2.1 (Engine Connection) from `docs/podbot-roadmap.md`

**Status:** Complete

---

## Big picture

Add a health check capability that verifies the container engine (Docker or
Podman) is operational after establishing a connection. The health check uses
Bollard's asynchronous `ping()` method with timeout handling to confirm the
engine responds to API requests, not just that the socket is reachable.

---

## Constraints

1. **No `unwrap`/`expect`** in production code (clippy denies these)
2. **Environment variable mocking** via the `mockable` crate for testability
3. **Semantic error types** using `thiserror` (`ContainerError` variants)
4. **Files < 400 lines**; split into submodules if needed
5. **rstest fixtures** for unit tests; **rstest-bdd** for behaviour-driven
   development (BDD) tests
6. **All checks must pass:** `make check-fmt`, `make lint`, `make test`

---

## Implementation tasks

### Task 1: Add error variants ✓

**File:** `src/error.rs`

Added two new variants to `ContainerError`:

```rust
/// Health check failed - engine did not respond correctly.
#[error("container engine health check failed: {message}")]
HealthCheckFailed {
    /// A description of the health check failure.
    message: String,
},

/// Health check timed out.
#[error("container engine health check timed out after {seconds} seconds")]
HealthCheckTimeout {
    /// The timeout duration in seconds.
    seconds: u64,
},
```

Added unit tests for error display formatting.

---

### Task 2: Implement health check methods ✓

**File:** `src/engine/connection/mod.rs`

Added constant:

```rust
const HEALTH_CHECK_TIMEOUT_SECS: u64 = 10;
```

Added three methods to `EngineConnector`:

1. `health_check(docker: &Docker) -> Result<(), PodbotError>` - Standalone
   health check that sends a ping request and waits for a response
2. `connect_and_verify(socket: &str) -> Result<Docker, PodbotError>` - Combined
   connect and health check in a single operation
3. `connect_with_fallback_and_verify<E>(...)` - Combined with socket resolution

Implementation uses:

- `tokio::runtime::Handle::current().block_on()` to call async `docker.ping()`
- `tokio::time::timeout()` to wrap the ping call with a 10-second timeout
- Maps Bollard errors to `ContainerError::HealthCheckFailed`
- Maps timeout to `ContainerError::HealthCheckTimeout`

---

### Task 3: Add unit tests ✓

**File:** `src/engine/connection/tests.rs`

Added tests for:

- `connect_and_verify` propagates connection errors correctly
- `connect_with_fallback_and_verify` uses resolved socket
- `connect_with_fallback_and_verify` falls back to environment variable

---

### Task 4: Add BDD scenarios ✓

**File:** `tests/features/engine_connection.feature`

Added three health check scenarios:

```gherkin
Scenario: Health check succeeds when engine is responsive
  Given a container engine is available
  When a health check is performed
  Then the health check succeeds

Scenario: Health check fails when engine does not respond
  Given the container engine is not responding
  When a health check is attempted
  Then a health check failure error is returned

Scenario: Health check times out on slow engine
  Given the container engine is slow to respond
  When a health check is attempted
  Then a health check timeout error is returned
```

---

### Task 5: Add BDD step definitions ✓

**Files:**

- `tests/bdd_engine_connection.rs` - Added scenario bindings
- `tests/bdd_engine_connection_helpers.rs` - Added step definitions

Added `HealthCheckOutcome` enum to track health check results.

Step definitions use `rstest_bdd::skip!()` when:

- No container daemon is available for the success scenario
- Timeout simulation is needed (requires controllable slow endpoint)

---

### Task 6: Update documentation ✓

**File:** `docs/users-guide.md`

Added "Engine health check" section documenting:

- Health check behaviour (ping request, 10-second timeout)
- Possible error messages users might see

---

### Task 7: Run verification ✓

Execute all checks before committing:

```bash
make check-fmt && make lint && make test
```

---

### Task 8: Update roadmap ✓

**File:** `docs/podbot-roadmap.md`

Mark task as done:

```markdown
- [x] Add a health check that verifies the engine responds.
```

---

## Design decisions

### Decision 1: Async strategy

**Chosen:** Use `tokio::runtime::Handle::current().block_on()` within
synchronous public methods.

**Rationale:** The application already depends on tokio with full features.
Bollard's `ping()` is an async method. Using `block_on` allows the existing
synchronous API surface to remain unchanged while supporting async operations
internally. The caller must be within a tokio runtime context, which is
documented in the function's panics section.

### Decision 2: Separate health check method

**Chosen:** Provide both a standalone `health_check()` and combined
`connect_and_verify()` methods.

**Rationale:** Separating the health check from connection allows callers to:

- Check health at any time after connection
- Decide whether to perform the check (useful for performance-sensitive
  scenarios)
- Re-verify connectivity after potential network issues

The combined method provides a convenient one-call solution for most use cases.

### Decision 3: Timeout handling

**Chosen:** Use `tokio::time::timeout()` with a 10-second default.

**Rationale:** Bollard's connection timeout applies to the HTTP/socket
connection, not to API response time. A separate timeout prevents indefinite
blocking when the daemon is unresponsive. Ten seconds provides sufficient time
for a normal ping response while catching truly hung daemons.

### Decision 4: Error variants

**Chosen:** Add two separate error variants: `HealthCheckFailed` and
`HealthCheckTimeout`.

**Rationale:** Distinguishing between a failed response and a timeout allows
callers to handle these cases differently. For example, a timeout might warrant
a retry with a longer timeout, while a failed response indicates the daemon is
responding but unhealthy.

---

## Files modified

Table: Files modified in this implementation

| File | Action |
|------|--------|
| `src/error.rs` | Added `HealthCheckFailed` and `HealthCheckTimeout` variants |
| `src/engine/connection/mod.rs` | Added health check methods |
| `src/engine/connection/tests.rs` | Added unit tests for health check flow |
| `tests/features/engine_connection.feature` | Added health check BDD scenarios |
| `tests/bdd_engine_connection.rs` | Added scenario bindings |
| `tests/bdd_engine_connection_helpers.rs` | Added step definitions |
| `docs/users-guide.md` | Documented health check behaviour |
| `docs/podbot-roadmap.md` | Marked task as done |

---

## Progress log

Table: Progress log for this implementation

| Date | Status | Notes |
|------|--------|-------|
| 2026-01-31 | Complete | All tasks implemented and verified |
