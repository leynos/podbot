# Execution Plan: Handle Socket Permission Errors (Phase 2.1.3)

## Overview

Implement actionable error handling for container engine socket permission
errors, as specified in `docs/podbot-roadmap.md` Phase 2.1.

**Roadmap task:** Handle socket permission errors with actionable error messages.

## Current State

- `src/engine/connection/mod.rs` (lines 254-258): All Bollard errors wrapped as
  `ContainerError::ConnectionFailed`
- `src/error.rs`: `ContainerError::PermissionDenied` and `SocketNotFound`
  variants exist but are unused
- rstest-bdd version: 0.3.2 (upgrade to 0.4.0 requested)

## Design Decisions

1. **Error detection:** Inspect Bollard's `IOError` variant and underlying
   `std::io::ErrorKind` (more reliable than string matching)
2. **Socket path extraction:** Parse `unix://`/`npipe://` prefixes
3. **Error classification:** Dedicated `classify_connection_error()` function
4. **Actionable messages:** Enhanced `#[error(...)]` with remediation hints
5. **rstest-bdd 0.4.0:** Use existing `Slot<T>` pattern (compatible with v0.4.0)

---

## Implementation Tasks

### Task 1: Upgrade rstest-bdd to 0.4.0

**File:** `Cargo.toml`

```toml
rstest-bdd = "0.4.0"
rstest-bdd-macros = { version = "0.4.0", features = ["compile-time-validation"] }
```

---

### Task 2: Add error classification helpers

**File:** `src/engine/connection/mod.rs`

Add import at top:

```rust
use std::path::PathBuf;
```

Add after `EngineConnector` impl block (~line 323):

```rust
/// Extract filesystem path from a socket URI.
fn extract_socket_path(socket_uri: &str) -> Option<PathBuf> {
    socket_uri
        .strip_prefix("unix://")
        .or_else(|| socket_uri.strip_prefix("npipe://"))
        .map(PathBuf::from)
}

/// Classify a Bollard connection error into a semantic `ContainerError`.
fn classify_connection_error(
    bollard_error: bollard::errors::Error,
    socket_uri: &str,
) -> ContainerError {
    use std::io::ErrorKind;

    let socket_path = extract_socket_path(socket_uri);

    // Check for Bollard's SocketNotFoundError variant
    if let bollard::errors::Error::SocketNotFoundError(_) = &bollard_error {
        if let Some(path) = socket_path {
            return ContainerError::SocketNotFound { path };
        }
    }

    // Check for wrapped io::Error
    if let Some(io_err) = find_io_error_in_chain(&bollard_error) {
        return match io_err.kind() {
            ErrorKind::PermissionDenied => socket_path
                .map(|path| ContainerError::PermissionDenied { path })
                .unwrap_or_else(|| ContainerError::ConnectionFailed {
                    message: bollard_error.to_string(),
                }),
            ErrorKind::NotFound => socket_path
                .map(|path| ContainerError::SocketNotFound { path })
                .unwrap_or_else(|| ContainerError::ConnectionFailed {
                    message: bollard_error.to_string(),
                }),
            _ => ContainerError::ConnectionFailed {
                message: bollard_error.to_string(),
            },
        };
    }

    ContainerError::ConnectionFailed {
        message: bollard_error.to_string(),
    }
}

/// Walk the error source chain looking for an io::Error.
fn find_io_error_in_chain(error: &dyn std::error::Error) -> Option<std::io::Error> {
    let mut current: Option<&(dyn std::error::Error + 'static)> = error.source();
    while let Some(err) = current {
        if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
            return Some(std::io::Error::new(io_err.kind(), io_err.to_string()));
        }
        current = err.source();
    }
    None
}
```

---

### Task 3: Update `connect()` to use classification

**File:** `src/engine/connection/mod.rs` (lines 254-258)

Replace:

```rust
.map_err(|e| {
    PodbotError::from(ContainerError::ConnectionFailed {
        message: e.to_string(),
    })
})?;
```

With:

```rust
.map_err(|e| PodbotError::from(classify_connection_error(e, socket_str)))?;
```

---

### Task 4: Enhance error messages

**File:** `src/error.rs` (lines 67-79)

Update `SocketNotFound`:

```rust
#[error(concat!(
    "container engine socket not found: {path}\n",
    "Hint: Verify the socket path exists and the container daemon is running.\n",
    "For Docker: sudo systemctl start docker\n",
    "For Podman: systemctl --user start podman.socket"
))]
SocketNotFound {
    /// The path where the socket was expected.
    path: PathBuf,
},
```

Update `PermissionDenied`:

```rust
#[error(concat!(
    "permission denied accessing container socket: {path}\n",
    "Hint: Add your user to the docker group or use rootless Podman.\n",
    "For Docker: sudo usermod -aG docker $USER && newgrp docker\n",
    "For Podman: Use socket at /run/user/$UID/podman/podman.sock"
))]
PermissionDenied {
    /// The path to the socket.
    path: PathBuf,
},
```

---

### Task 5: Add unit tests

**File:** `src/engine/connection/tests.rs`

Add test section:

```rust
// =============================================================================
// Error classification tests
// =============================================================================

#[rstest]
#[case::unix_socket("unix:///var/run/docker.sock", Some("/var/run/docker.sock"))]
#[case::npipe("npipe:////./pipe/docker_engine", Some("//./pipe/docker_engine"))]
#[case::http("http://localhost:2375", None)]
#[case::tcp("tcp://localhost:2375", None)]
#[case::bare_path("/var/run/docker.sock", None)]
fn extract_socket_path_parses_correctly(
    #[case] uri: &str,
    #[case] expected: Option<&str>,
) {
    let result = super::extract_socket_path(uri);
    assert_eq!(result.as_ref().map(|p| p.to_str().expect("valid UTF-8")), expected);
}

#[rstest]
fn classify_connection_error_handles_permission_denied() {
    use std::io::{Error as IoError, ErrorKind};

    let io_err = IoError::new(ErrorKind::PermissionDenied, "permission denied");
    let bollard_err = bollard::errors::Error::IOError(io_err);

    let result = super::classify_connection_error(
        bollard_err,
        "unix:///var/run/docker.sock",
    );

    assert!(
        matches!(result, ContainerError::PermissionDenied { path } if path.to_str() == Some("/var/run/docker.sock")),
        "expected PermissionDenied with path, got: {result:?}"
    );
}

#[rstest]
fn classify_connection_error_handles_not_found() {
    use std::io::{Error as IoError, ErrorKind};

    let io_err = IoError::new(ErrorKind::NotFound, "no such file");
    let bollard_err = bollard::errors::Error::IOError(io_err);

    let result = super::classify_connection_error(
        bollard_err,
        "unix:///nonexistent.sock",
    );

    assert!(
        matches!(result, ContainerError::SocketNotFound { path } if path.to_str() == Some("/nonexistent.sock")),
        "expected SocketNotFound with path, got: {result:?}"
    );
}

#[rstest]
fn classify_connection_error_falls_back_for_other_errors() {
    use std::io::{Error as IoError, ErrorKind};

    let io_err = IoError::new(ErrorKind::ConnectionRefused, "connection refused");
    let bollard_err = bollard::errors::Error::IOError(io_err);

    let result = super::classify_connection_error(
        bollard_err,
        "unix:///var/run/docker.sock",
    );

    assert!(
        matches!(result, ContainerError::ConnectionFailed { .. }),
        "expected ConnectionFailed, got: {result:?}"
    );
}

#[rstest]
fn classify_connection_error_falls_back_for_http_endpoints() {
    use std::io::{Error as IoError, ErrorKind};

    let io_err = IoError::new(ErrorKind::PermissionDenied, "permission denied");
    let bollard_err = bollard::errors::Error::IOError(io_err);

    let result = super::classify_connection_error(bollard_err, "http://localhost:2375");

    assert!(
        matches!(result, ContainerError::ConnectionFailed { .. }),
        "expected ConnectionFailed for HTTP endpoint, got: {result:?}"
    );
}
```

---

### Task 6: Add BDD feature scenarios

**File:** `tests/features/engine_connection.feature`

Add after line 82:

```gherkin

  # Socket permission error scenarios

  Scenario: Permission denied error provides actionable guidance
    Given a socket path that requires elevated permissions
    When a connection is attempted
    Then a permission denied error is returned
    And the error message includes the socket path

  Scenario: Socket not found error provides actionable guidance
    Given a socket path that does not exist
    When a connection is attempted
    Then a socket not found error is returned
    And the error message includes the socket path
```

---

### Task 7: Add BDD step definitions

**File:** `tests/bdd_engine_connection_helpers/permission_error_steps.rs` (NEW)

```rust
//! Permission error step definitions for BDD tests.

use podbot::engine::EngineConnector;
use podbot::error::{ContainerError, PodbotError};
use rstest_bdd_macros::{given, then, when};

use super::{EngineConnectionState, StepResult};

#[given("a socket path that requires elevated permissions")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
fn socket_requires_elevated_permissions(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    engine_connection_state
        .test_socket_path
        .set(String::from("unix:///var/run/docker.sock"));
    Ok(())
}

#[given("a socket path that does not exist")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "rstest-bdd step functions must return StepResult"
)]
fn socket_does_not_exist(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    engine_connection_state
        .test_socket_path
        .set(String::from("unix:///nonexistent/podbot-test-socket.sock"));
    Ok(())
}

#[when("a connection is attempted")]
fn connection_is_attempted(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    let socket = engine_connection_state
        .test_socket_path
        .get()
        .ok_or("test socket path should be set")?;

    let result = EngineConnector::connect(&socket);
    engine_connection_state.connection_result.set(result);
    Ok(())
}

#[then("a permission denied error is returned")]
fn permission_denied_error_returned(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    let result = engine_connection_state
        .connection_result
        .get()
        .ok_or("connection result should be set")?;

    match result {
        Err(PodbotError::Container(ContainerError::PermissionDenied { .. })) => Ok(()),
        Ok(_) => {
            rstest_bdd::skip!("user has permission to access the socket");
        }
        Err(PodbotError::Container(ContainerError::SocketNotFound { .. })) => {
            rstest_bdd::skip!("socket not found; daemon may not be running");
        }
        Err(e) => Err(Box::leak(
            format!("expected PermissionDenied, got: {e}").into_boxed_str(),
        )),
    }
}

#[then("a socket not found error is returned")]
fn socket_not_found_error_returned(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    let result = engine_connection_state
        .connection_result
        .get()
        .ok_or("connection result should be set")?;

    match result {
        Err(PodbotError::Container(ContainerError::SocketNotFound { .. })) => Ok(()),
        Err(e) => Err(Box::leak(
            format!("expected SocketNotFound, got: {e}").into_boxed_str(),
        )),
        Ok(_) => Err("expected error but connection succeeded"),
    }
}

#[then("the error message includes the socket path")]
fn error_includes_socket_path(
    engine_connection_state: &EngineConnectionState,
) -> StepResult<()> {
    let result = engine_connection_state
        .connection_result
        .get()
        .ok_or("connection result should be set")?;

    let error_msg = match result {
        Err(e) => e.to_string(),
        Ok(_) => return Err("expected error but connection succeeded"),
    };

    let socket = engine_connection_state
        .test_socket_path
        .get()
        .ok_or("test socket path should be set")?;

    let expected_path = socket.strip_prefix("unix://").unwrap_or(&socket);

    if error_msg.contains(expected_path) {
        Ok(())
    } else {
        Err(Box::leak(
            format!("error message should contain path '{expected_path}': {error_msg}")
                .into_boxed_str(),
        ))
    }
}
```

---

### Task 8: Update BDD helpers module

**File:** `tests/bdd_engine_connection_helpers/mod.rs`

Add module declaration after line 6:

```rust
mod permission_error_steps;
```

Add re-export after line 22:

```rust
pub use permission_error_steps::*;
```

Add import at top:

```rust
use podbot::error::PodbotError;
```

Add fields to `EngineConnectionState` struct after line 55:

```rust
    /// The socket path to test against (for error testing).
    pub test_socket_path: Slot<String>,
    /// The result of a connection attempt.
    pub connection_result: Slot<Result<bollard::Docker, PodbotError>>,
```

---

### Task 9: Add BDD scenario bindings

**File:** `tests/bdd_engine_connection.rs`

Add after line 101:

```rust
// Permission error scenario bindings

#[scenario(
    path = "tests/features/engine_connection.feature",
    name = "Permission denied error provides actionable guidance"
)]
fn permission_denied_error_guidance(engine_connection_state: EngineConnectionState) {
    let _ = engine_connection_state;
}

#[scenario(
    path = "tests/features/engine_connection.feature",
    name = "Socket not found error provides actionable guidance"
)]
fn socket_not_found_error_guidance(engine_connection_state: EngineConnectionState) {
    let _ = engine_connection_state;
}
```

---

### Task 10: Update user documentation

**File:** `docs/users-guide.md`

Add after line 199 (after "Engine health check" section):

```markdown
### Connection error troubleshooting

When podbot cannot connect to the container engine, it provides actionable error
messages to help diagnose the issue.

**Possible connection errors:**

| Error | Cause | Resolution |
| ----- | ----- | ---------- |
| `permission denied accessing container socket: <path>` | User lacks permission to access the Docker/Podman socket | Add user to the docker group: `sudo usermod -aG docker $USER && newgrp docker`. For Podman, use the rootless socket at `/run/user/$UID/podman/podman.sock` |
| `container engine socket not found: <path>` | Socket file does not exist; daemon not running | Start the daemon: Docker: `sudo systemctl start docker`. Podman: `systemctl --user start podman.socket` |
| `failed to connect to container engine: connection refused` | Daemon not accepting connections | Restart the daemon service and check its status |

**Common permission scenarios:**

1. **Docker on Linux**: By default, the Docker socket (`/var/run/docker.sock`)
   is owned by the `docker` group. Add your user to this group:

   ```bash
   sudo usermod -aG docker $USER
   newgrp docker  # Apply group membership without logging out
   ```

2. **Rootless Podman**: Use the user-level socket instead of the system socket:

   ```bash
   # Start the user socket
   systemctl --user start podman.socket

   # Configure podbot to use it
   export PODBOT_ENGINE_SOCKET="unix:///run/user/$(id -u)/podman/podman.sock"
   ```

3. **Podman with sudo**: If using the system Podman socket, ensure the socket
   service is running:

   ```bash
   sudo systemctl start podman.socket
   ```
```

---

### Task 11: Update roadmap

**File:** `docs/podbot-roadmap.md` (line 79)

Change:

```markdown
- [ ] Handle socket permission errors with actionable error messages.
```

To:

```markdown
- [x] Handle socket permission errors with actionable error messages.
```

---

## Files Modified

| File | Action |
| ---- | ------ |
| `Cargo.toml` | Upgrade rstest-bdd to 0.4.0 |
| `src/engine/connection/mod.rs` | Add error classification |
| `src/error.rs` | Enhance error messages |
| `src/engine/connection/tests.rs` | Add unit tests |
| `tests/features/engine_connection.feature` | Add scenarios |
| `tests/bdd_engine_connection_helpers/mod.rs` | Add state fields |
| `tests/bdd_engine_connection_helpers/permission_error_steps.rs` | **New file** |
| `tests/bdd_engine_connection.rs` | Add scenario bindings |
| `docs/users-guide.md` | Add troubleshooting docs |
| `docs/podbot-roadmap.md` | Mark task complete |
| `docs/execplans/2-1-3-handle-socket-permission-errors.md` | **New file** |

## Verification

```bash
set -o pipefail
make check-fmt 2>&1 | tee /tmp/fmt.log
make lint 2>&1 | tee /tmp/lint.log
make test 2>&1 | tee /tmp/test.log
```

## Testing Notes

- Unit tests verify error classification with mock Bollard errors
- BDD `SocketNotFound` test should always pass (nonexistent path)
- BDD `PermissionDenied` test uses `rstest_bdd::skip!()` when user has
  permissions or daemon not running

---

## Final Status: COMPLETE

All tasks completed successfully:

1. rstest-bdd upgraded from 0.3.2 to 0.4.0
2. Error classification functions added with proper Bollard error inspection
3. `connect()` now uses semantic error classification
4. Error messages enhanced with actionable remediation hints
5. Unit tests added for error classification (4 tests)
6. BDD feature scenarios added (2 scenarios)
7. BDD step definitions added in new module
8. BDD helpers updated with `ConnectionOutcome` enum for state tracking
9. Scenario bindings added
10. User documentation updated with troubleshooting section
11. Roadmap marked complete
12. Execution plan copied to execplans directory

**Quality gates passed:**
- `make check-fmt` ✓
- `make lint` ✓
- `make test` ✓ (all 167 tests pass)

## Lessons Learned

1. **Bollard IOError struct syntax**: Bollard v0.20.0 uses `IOError { err }` struct
   syntax rather than tuple `IOError(io_err)`. Must match directly on this variant
   rather than relying on error source chain inspection.

2. **thiserror concat!() limitation**: The `#[error(...)]` attribute doesn't support
   `concat!()` macro. Use multi-line string literals with `\n\` continuation instead.

3. **rstest-bdd Slot<T> requires Clone**: Can't store `Result<Docker, PodbotError>`
   in `Slot<T>` because it doesn't implement `Clone`. Created `ConnectionOutcome`
   enum that captures just the error classification result.

4. **Health check skip logic**: After adding new error message formats, existing
   `is_daemon_unavailable()` needed updating to recognize "socket not found" pattern
   in addition to legacy patterns like "No such file".
