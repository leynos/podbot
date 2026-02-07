# Execplan: Socket connection via environment variables

**Task:** Implement socket connection via `DOCKER_HOST`, `CONTAINER_HOST`, or
`PODMAN_HOST` environment variables or direct path specification.

**Phase:** 2.1 (Engine Connection) from `docs/podbot-roadmap.md`

**Status:** Complete

______________________________________________________________________

## Big picture

Create a container engine module that connects to Docker or Podman via the
Bollard library. The socket endpoint is resolved through a priority-based
fallback chain:

1. **Command-line interface (CLI) argument** (`--engine-socket`) — highest
   priority
2. **Config file** (`engine_socket` in TOML)
3. **`PODBOT_ENGINE_SOCKET`** environment variable
4. **`DOCKER_HOST`** environment variable
5. **`CONTAINER_HOST`** environment variable
6. **`PODMAN_HOST`** environment variable
7. **Platform default** — `/var/run/docker.sock` (Unix) or
   `//./pipe/docker_engine` (Windows)

The first three sources are already handled by the existing configuration layer
system (`loader.rs`). This implementation adds support for the
industry-standard `DOCKER_HOST`, `CONTAINER_HOST`, and `PODMAN_HOST`
environment variables as fallbacks, and connects to the engine using Bollard.

______________________________________________________________________

## Constraints

1. **No `unwrap`/`expect`** in production code (clippy denies these)
2. **Environment variable mocking** via the `mockable` crate for testability
3. **Semantic error types** using `thiserror` (`ContainerError` already defined)
4. **Files < 400 lines**; split into submodules if needed
5. **rstest fixtures** for unit tests; **rstest-bdd** for behaviour-driven
   development (BDD) tests
6. **All checks must pass:** `make check-fmt`, `make lint`, `make test`

______________________________________________________________________

## Implementation tasks

### Task 1: Add `mockable` dependency ✓

**File:** `Cargo.toml`

Add `mockable` to dev-dependencies for environment variable mocking in tests:

```toml
[dev-dependencies]
mockable = { version = "0.1.4", default-features = false, features = ["mock"] }
```

______________________________________________________________________

### Task 2: Create the `engine` module ✓

**Files to create:**

- `src/engine/mod.rs` — module root with public exports
- `src/engine/connection.rs` — socket resolution and Bollard connection logic

**File:** `src/lib.rs`

Add module declaration:

```rust
pub mod engine;
```

______________________________________________________________________

### Task 3: Implement `SocketResolver` ✓

**File:** `src/engine/connection.rs`

Create a `SocketResolver` struct that determines the socket endpoint from
multiple sources using dependency injection for testability:

```rust
use mockable::Env;

/// Environment variable names checked in fallback order.
const FALLBACK_ENV_VARS: &[&str] = &[
    "DOCKER_HOST",
    "CONTAINER_HOST",
    "PODMAN_HOST",
];

/// Default socket paths by platform.
#[cfg(unix)]
const DEFAULT_SOCKET: &str = "unix:///var/run/docker.sock";

#[cfg(windows)]
const DEFAULT_SOCKET: &str = "npipe:////./pipe/docker_engine";

pub struct SocketResolver<'a, E: Env> {
    env: &'a E,
}

impl<'a, E: Env> SocketResolver<'a, E> {
    pub fn new(env: &'a E) -> Self {
        Self { env }
    }

    /// Resolve the socket endpoint from fallback environment variables.
    /// Returns `None` if no fallback variable is set.
    pub fn resolve_from_env(&self) -> Option<String> {
        for var_name in FALLBACK_ENV_VARS {
            if let Ok(value) = self.env.var(var_name) {
                if !value.is_empty() {
                    return Some(value);
                }
            }
        }
        None
    }

    /// Returns the platform default socket path.
    pub const fn default_socket() -> &'static str {
        DEFAULT_SOCKET
    }
}
```

______________________________________________________________________

### Task 4: Implement `EngineConnector` ✓

**File:** `src/engine/connection.rs`

Create a connector that uses Bollard to establish the connection:

```rust
use bollard::Docker;
use crate::error::{ContainerError, Result};

pub struct EngineConnector;

impl EngineConnector {
    /// Connect to the container engine at the specified socket path.
    ///
    /// Supports Unix sockets (`unix://`), Windows named pipes (`npipe://`),
    /// HTTP (`http://`), and HTTPS (`https://`) endpoints.
    pub fn connect(socket: &str) -> Result<Docker> {
        let docker = if socket.starts_with("unix://") || socket.starts_with("npipe://") {
            Docker::connect_with_socket(socket, 120, &bollard::API_DEFAULT_VERSION)
        } else if socket.starts_with("http://") || socket.starts_with("https://") {
            Docker::connect_with_http(socket, 120, &bollard::API_DEFAULT_VERSION)
        } else {
            // Treat bare paths as Unix sockets
            let socket_uri = format!("unix://{socket}");
            Docker::connect_with_socket(&socket_uri, 120, &bollard::API_DEFAULT_VERSION)
        }
        .map_err(|e| ContainerError::ConnectionFailed {
            message: e.to_string(),
        })?;

        Ok(docker)
    }

    /// Connect using the resolved socket from configuration and environment.
    ///
    /// Resolution order:
    /// 1. `config_socket` (from CLI, config file, or `PODBOT_ENGINE_SOCKET`)
    /// 2. `DOCKER_HOST`, `CONTAINER_HOST`, `PODMAN_HOST` (via resolver)
    /// 3. Platform default socket
    pub fn connect_with_fallback<E: Env>(
        config_socket: Option<&str>,
        resolver: &SocketResolver<'_, E>,
    ) -> Result<Docker> {
        let socket = config_socket
            .map(String::from)
            .or_else(|| resolver.resolve_from_env())
            .unwrap_or_else(|| SocketResolver::<E>::default_socket().to_owned());

        Self::connect(&socket)
    }
}
```

______________________________________________________________________

### Task 5: Update `ContainerError` if needed ✓

**File:** `src/error.rs`

The existing `ContainerError::ConnectionFailed` variant is sufficient. No
changes needed.

______________________________________________________________________

### Task 6: Create unit tests with `rstest` ✓

**File:** `src/engine/connection.rs` (in `#[cfg(test)]` module)

Unit tests cover:

- Resolver returns `None` when no environment variables are set
- Resolver returns `DOCKER_HOST` value when set
- Each fallback variable is respected individually
- `DOCKER_HOST` takes priority over `CONTAINER_HOST` and `PODMAN_HOST`
- `CONTAINER_HOST` takes priority over `PODMAN_HOST`
- Empty environment variable values are skipped
- Default socket path is correct for the platform
- `resolve_socket` uses config when provided
- `resolve_socket` uses environment when config is `None`
- `resolve_socket` uses default when no source is available
- Config takes precedence over environment

______________________________________________________________________

### Task 7: Create BDD feature file ✓

**File:** `tests/features/engine_connection.feature`

Scenarios cover:

- Socket resolved from `DOCKER_HOST` when config is not set
- Config socket takes precedence over `DOCKER_HOST`
- Fallback to `CONTAINER_HOST` when `DOCKER_HOST` is not set
- Fallback to `PODMAN_HOST` when higher-priority vars are not set
- Fallback to platform default when no sources are set
- Empty environment variable is skipped
- `DOCKER_HOST` takes priority over `CONTAINER_HOST`
- `CONTAINER_HOST` takes priority over `PODMAN_HOST`

______________________________________________________________________

### Task 8: Create BDD test implementation ✓

**File:** `tests/bdd_engine_connection.rs`

Scenario bindings for all feature file scenarios.

**File:** `tests/bdd_engine_connection_helpers.rs`

Step definitions using `MockEnv` for environment variable control.

______________________________________________________________________

### Task 9: Update user's guide ✓

**File:** `docs/users-guide.md`

Added documentation for the container engine socket resolution order.

______________________________________________________________________

### Task 10: Update roadmap ✓

**File:** `docs/podbot-roadmap.md`

Mark the first task in Step 2.1 as done.

______________________________________________________________________

### Task 11: Run verification ✓

Execute all checks before committing:

```bash
make check-fmt && make lint && make test
```

______________________________________________________________________

## Design decisions

### Decision 1: Fallback order for environment variables

**Chosen:** `DOCKER_HOST` > `CONTAINER_HOST` > `PODMAN_HOST`

**Rationale:** `DOCKER_HOST` is the most widely used convention.
`CONTAINER_HOST` is a container-agnostic alternative. `PODMAN_HOST` is
Podman-specific. This order maximizes compatibility with existing tooling.

### Decision 2: Dependency injection for environment access

**Chosen:** Use `mockable::Env` trait for environment variable access.

**Rationale:** Per `docs/reliable-testing-in-rust-via-dependency-injection.md`,
this enables isolated, deterministic unit tests without touching the global
process environment. Tests can run in parallel without interference.

### Decision 3: Module structure

**Chosen:** Create `src/engine/` module with `connection.rs` submodule.

**Rationale:** Keeps the engine-related code organized. Future tasks (health
check, container creation) will add sibling modules under `src/engine/`.

### Decision 4: Synchronous connect method

**Chosen:** Made `connect` and `connect_with_fallback` synchronous rather than
async.

**Rationale:** Bollard's `connect_with_socket` and `connect_with_http` methods
are synchronous — they only create the client configuration. The actual async
input/output (I/O) happens when making application programming interface (API)
calls. This simplifies the API and test code.

______________________________________________________________________

## Files modified

Table: Files modified in this implementation

| File                                       | Action                                             |
| ------------------------------------------ | -------------------------------------------------- |
| `Cargo.toml`                               | Added `mockable` dev-dependency                    |
| `src/lib.rs`                               | Added `pub mod engine;`                            |
| `src/engine/mod.rs`                        | Created module root                                |
| `src/engine/connection.rs`                 | Implemented `SocketResolver` and `EngineConnector` |
| `tests/features/engine_connection.feature` | Created BDD feature                                |
| `tests/bdd_engine_connection.rs`           | Created BDD test file                              |
| `tests/bdd_engine_connection_helpers.rs`   | Created BDD step definitions                       |
| `docs/users-guide.md`                      | Documented environment variable fallback           |
| `docs/podbot-roadmap.md`                   | Marked task as done                                |

______________________________________________________________________

## Progress log

Table: Progress log for this implementation

| Date       | Status   | Notes                              |
| ---------- | -------- | ---------------------------------- |
| 2026-01-25 | Complete | All tasks implemented and verified |
