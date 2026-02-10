# Sandboxed agent runner design

This document describes a sandboxed execution environment for running AI coding
agents (Claude Code, Codex) with repository access. The design prioritizes
security by treating the host container engine as high-trust infrastructure,
while the agent container operates in a low-trust playpen with no access to the
host socket.

## Overview

The core principle is straightforward: the Rust Command-Line Interface (CLI)
acts as the sole holder of the host Podman or Docker socket. The agent
container never receives access to this socket. Instead, the agent runs an
inner Podman service for any nested container operations (such as `act` for
GitHub Actions or `cross` for cross-compilation), ensuring that mount paths
resolve within the sandbox filesystem rather than the host.

For screen readers: The following diagram illustrates the trust boundary
between the host CLI and the sandboxed agent container.

```mermaid
flowchart TB
    subgraph Host["Host system (high trust)"]
        CLI["Rust CLI"]
        Socket["Podman/Docker socket"]
        TokenFile["Token file<br/>(XDG_RUNTIME_DIR)"]
        CLI --> Socket
        CLI --> TokenFile
    end

    subgraph Sandbox["Agent container (low trust)"]
        Agent["Claude Code / Codex"]
        InnerPodman["Inner Podman service"]
        Repo["Cloned repository"]
        Agent --> InnerPodman
        Agent --> Repo
    end

    CLI -->|"Creates and manages"| Sandbox
    TokenFile -.->|"Read-only bind mount"| Agent
```

_Figure 1: Trust boundaries between host CLI and sandboxed agent._

## Execution flow

The CLI orchestrates container creation and agent execution through eight steps.

1. **Create outer container** from a pre-configured image containing:

   - `podman`, `fuse-overlayfs`, and `slirp4netns` for the inner engine
   - `git`
   - `claude` and `codex` binaries
   - A helper script for Git authentication via token file

2. **Inject agent credentials** from `~/.claude` and `~/.codex` by copying into
   the container filesystem using Bollard's `upload_to_container` method.[^1]
   Bind-mounting the home directory would expose unnecessary host state.

3. **Configure Git identity** by reading `user.name` and `user.email` from the
   host and executing `git config --global` within the container.

4. **Create a GitHub App installation access token** using Octocrab.[^2]
   Installation tokens expire after one hour.[^3]

5. **Start a token renewal daemon** that refreshes the installation token before
   expiry. Rather than repeatedly executing commands or copying files into the
   container, this daemon writes the token to a host-side file and relies on a
   read-only bind mount for the container to observe updates.

6. **Clone the repository** specified by the operator. GitHub supports
   Hypertext Transfer Protocol (HTTP) access using the installation token in
   the form `x-access-token:TOKEN@github.com/owner/repo`.[^3] However,
   embedding tokens in Uniform Resource Locators (URLs) risks leaking
   credentials into process arguments and shell history. A safer approach uses
   `GIT_ASKPASS` with a script that reads from `/run/secrets/ghapp_token`.

7. **Start the agent in permissive mode**, attached to the terminal:

   - Claude Code: `claude --dangerously-skip-permissions`[^4]
   - Codex CLI: `codex --dangerously-bypass-approvals-and-sandbox`[^5]

## Security model

The design establishes clear trust boundaries.

| Component       | Trust level | Socket access     | Notes                                     |
| --------------- | ----------- | ----------------- | ----------------------------------------- |
| Rust CLI        | High        | Host socket       | Single auditable chokepoint               |
| Agent container | Low         | Inner socket only | Cannot reach host engine                  |
| Token daemon    | High        | None              | Runs on host, writes to runtime directory |

_Table 1: Trust levels and socket access by component._

The agent container cannot escalate to host access because:

- It never receives the host Podman or Docker socket.
- Nested containers operate via an inner Podman socket, so any mounts resolve
  within the container filesystem.
- GitHub token refresh occurs outside the container; the agent observes a
  read-only file.

This design does not eliminate all risk. Container isolation depends on kernel
security boundaries. A container escape vulnerability would compromise the host
user account. Virtual machines (VMs) provide stronger isolation guarantees but
at the cost of operational complexity.

For additional hardening, network egress could be restricted to model endpoints
and GitHub. Both Claude Code and Codex documentation note prompt injection
risks when broad network access is enabled.[^4][^5]

## Error handling boundary

See "Error handling" below for the detailed error boundary description.

## Token management

GitHub App installation tokens present a credential lifecycle challenge: they
expire after one hour and must be refreshed without interrupting the agent.

The token strategy works as follows:

1. On container creation, the CLI establishes a runtime directory at
   `$XDG_RUNTIME_DIR/podbot/<container_id>/`.

2. The CLI writes the initial token to `ghapp_token` within this directory,
   with mode `0600` and directory mode `0700`.

3. The container receives a read-only bind mount:
   `<token_path>:/run/secrets/ghapp_token:ro`.

4. The token daemon refreshes the token with a time buffer using Octocrab's
   `installation_token_with_buffer` method,[^2] writing atomically via rename
   from a temporary file.

5. Inside the container, `GIT_ASKPASS` reads the mounted file, ensuring Git
   operations continue working after token refresh.

```rust,no_run
// Token refresh pseudocode
let octocrab = Octocrab::builder()
    .app(app_id, private_key)
    .build()?;

let installation = octocrab.installation(installation_id);
let token = installation
    .installation_token_with_buffer(Duration::from_secs(300))
    .await?;

// Atomic write: create temporary, then rename
let temp_path = token_path.with_extension("new");
std::fs::write(&temp_path, token.as_str())?;
std::fs::rename(&temp_path, &token_path)?;
```

## Crate selection

The implementation relies on three primary crates.

### Bollard

Bollard provides a Docker Application Programming Interface (API) client that
connects to Unix sockets via `DOCKER_HOST` or direct path specification.[^1]
Key methods include:

- `create_container` and `start_container` for lifecycle management
- `upload_to_container` for injecting credentials as tar archives
- `exec` with TTY attachment for interactive agent sessions

### Octocrab

Octocrab handles GitHub App authentication and token management.[^2] The
`OctocrabBuilder::app(app_id, key)` constructor establishes App identity, and
the installation method acquires scoped tokens with automatic caching.

### OrthoConfig

OrthoConfig provides layered configuration with predictable precedence: CLI
flags override environment variables, which override configuration files, which
override defaults.[^6] The derive macro generates the layering logic from
annotated structs.

## Engine connection protocol support

The `EngineConnector` supports multiple endpoint protocols for connecting to
container engines. This section documents the rationale for the protocol
handling design.

### Supported protocols

| Protocol    | Scheme prefix | Bollard method       | Use case                                           |
| ----------- | ------------- | -------------------- | -------------------------------------------------- |
| Unix socket | `unix://`     | `connect_with_socket` | Local Docker/Podman daemon (default on Linux/macOS) |
| Named pipe  | `npipe://`    | `connect_with_socket` | Local Docker on Windows                            |
| TCP         | `tcp://`      | `connect_with_http`  | Remote daemon over network                         |
| HTTP        | `http://`     | `connect_with_http`  | Remote daemon (explicit HTTP)                      |
| HTTPS       | `https://`    | `connect_with_http`  | Remote daemon with TLS                             |
| Bare path   | (none)        | `connect_with_socket` | Convenience shorthand for socket paths             |

_Table 2: Supported connection protocols and their Bollard dispatch._

### TCP-to-HTTP rewriting

Bollard does not natively accept `tcp://` schemes. The `EngineConnector`
rewrites `tcp://` to `http://` before calling `connect_with_http`. This
matches the behaviour of the Docker command-line interface (CLI), which treats
`tcp://` as an alias for `http://`. The rewriting is a simple string
replacement (`tcp://` to `http://`) applied once during connection
establishment.

### Lazy versus eager connection

Unix socket and named pipe connections via `connect_with_socket` perform eager
validation: if the socket file does not exist or is inaccessible, the
connection fails immediately with a descriptive error (`SocketNotFound` or
`PermissionDenied`).

TCP/HTTP connections via `connect_with_http` are lazy: Bollard creates the
client configuration synchronously without attempting to reach the remote host.
Failures surface only during the first API call, typically the health check
ping. This means:

- `connect()` always succeeds for TCP/HTTP endpoints.
- Errors are detected during `connect_and_verify()` (health check phase).
- TCP errors produce `ConnectionFailed` or `HealthCheckFailed`, never
  `SocketNotFound` or `PermissionDenied` because there is no filesystem path
  to attribute the error to.

### Bare path normalization

Paths without a scheme prefix are classified by syntax:

- Paths starting with `\\` or `//` are treated as Windows named pipe paths and
  prefixed with `npipe://`.
- All other paths are treated as Unix socket paths and prefixed with `unix://`.

This detection is syntax-based, not platform-based. A path like `//some/path`
is treated as a named pipe even on Unix. This is a deliberate design choice to
support cross-platform testing and configuration portability.

## Configuration

The CLI reads configuration from `~/.config/podbot/config.toml` with
environment and flag overrides.

```toml
engine_socket = "unix:///run/user/1000/podman/podman.sock"
image = "ghcr.io/example/podbot-sandbox:latest"

[github]
app_id = 12345
installation_id = 67890
private_key_path = "/home/user/.config/podbot/github-app.pem"

[workspace]
base_dir = "/work"

[creds]
copy_claude = true
copy_codex = true

[sandbox]
privileged = false
mount_dev_fuse = true

[agent]
kind = "codex"
mode = "podbot"
```

The `agent.mode` setting defines the execution mode for the agent. The current
implementation accepts only `podbot`, which indicates the default
podbot-managed execution path. This value is reserved for future expansion when
additional execution modes are introduced.

The `sandbox.privileged` setting controls the trade-off between compatibility
and isolation. Privileged mode enables more Podman-in-Podman configurations but
expands the attack surface. The minimal mode mounts only `/dev/fuse` and avoids
the privileged flag.

## Error handling

Podbot defines semantic error enums in `src/error.rs` for configuration,
container, GitHub, and filesystem operations. These enums are aggregated by
`PodbotError`, and modules return `podbot::error::Result<T>` so callers can
match on domain failures. The binary keeps opaque reporting at the boundary by
returning `eyre::Result<()>` from `main` and converting domain errors into
`eyre::Report` only when presenting messages to the operator.

For screen readers: The following diagram summarizes the error types and how
they flow from library modules to the CLI entry point.

```mermaid
classDiagram
    direction TB

    class ConfigError {
        +message: String
        +source: OptionError
    }
    <<enumeration>> ConfigError

    class ContainerError {
        +message: String
        +source: OptionError
    }
    <<enumeration>> ContainerError

    class GitHubError {
        +message: String
        +source: OptionError
    }
    <<enumeration>> GitHubError

    class FilesystemError {
        +message: String
        +path: OptionPathBuf
        +source: OptionError
    }
    <<enumeration>> FilesystemError

    class PodbotError {
        +from_config(error: ConfigError)
        +from_container(error: ContainerError)
        +from_github(error: GitHubError)
        +from_filesystem(error: FilesystemError)
        +display() String
        +source() OptionError
    }

    class ResultAlias {
        +ResultAliasT
    }

    class EyreReport {
        +from_error(error: PodbotError)
    }

    class PodbotLib {
        +public_api_functions_return_ResultAlias()
    }

    class MainCli {
        +main() eyreResultUnit
    }

    class UnitTestsErrorModule {
        +test_error_display()
        +test_from_conversions()
        +test_result_alias_usage()
    }

    class BddTestsErrorHandling {
        +given_invalid_configuration()
        +when_running_cli()
        +then_user_sees_friendly_error_message()
    }

    PodbotError --> ConfigError : wraps
    PodbotError --> ContainerError : wraps
    PodbotError --> GitHubError : wraps
    PodbotError --> FilesystemError : wraps

    ResultAlias --> PodbotError : error_type

    PodbotLib --> ResultAlias : uses

    MainCli --> EyreReport : returns
    MainCli --> PodbotLib : calls

    EyreReport --> PodbotError : constructed_from

    UnitTestsErrorModule --> PodbotError : tests
    UnitTestsErrorModule --> ConfigError : tests
    UnitTestsErrorModule --> ContainerError : tests
    UnitTestsErrorModule --> GitHubError : tests
    UnitTestsErrorModule --> FilesystemError : tests

    BddTestsErrorHandling --> MainCli : drives
    BddTestsErrorHandling --> EyreReport : observes
    BddTestsErrorHandling --> PodbotError : scenarios_cover
```

For screen readers: The following class diagram focuses on engine connection
error classification and how semantic container errors propagate to
`PodbotError`.

```mermaid
classDiagram
    class EngineConnector {
        +connect(socket_str: &str) Result~Docker, PodbotError~
    }

    class ContainerError {
        <<enum>>
        ConnectionFailed
        SocketNotFound
        PermissionDenied
    }

    class ContainerError_ConnectionFailed {
        +message: String
    }

    class ContainerError_SocketNotFound {
        +path: PathBuf
    }

    class ContainerError_PermissionDenied {
        +path: PathBuf
    }

    class PodbotError {
        <<enum or struct>>
        +from(container_error: ContainerError) PodbotError
    }

    class ErrorClassificationHelpers {
        <<module>>
        +extract_socket_path(socket_uri: &str) Option~&Path~
        +classify_io_error_kind(kind: ErrorKind, socket_path: Option~&Path~, error_msg: &str) ContainerError
        +classify_connection_error(bollard_error: &Error, socket_uri: &str) ContainerError
        +io_error_kind_in_chain(error: &Error) Option~ErrorKind~
    }

    class BollardError {
        <<enum>>
        SocketNotFoundError
        IOError
        Other
    }

    class IoError {
        +kind() ErrorKind
        +to_string() String
    }

    class ErrorKind {
        <<enum>>
        PermissionDenied
        NotFound
        Other
    }

    EngineConnector --> ErrorClassificationHelpers : uses
    EngineConnector --> PodbotError : returns
    PodbotError o-- ContainerError
    ContainerError o-- ContainerError_ConnectionFailed
    ContainerError o-- ContainerError_SocketNotFound
    ContainerError o-- ContainerError_PermissionDenied
    ErrorClassificationHelpers --> ContainerError : returns
    ErrorClassificationHelpers --> BollardError : inspects
    ErrorClassificationHelpers --> IoError : inspects
    IoError --> ErrorKind : uses
```

_Figure 2: Engine connection error hierarchy and classification flow._

## CLI interface

The CLI exposes a minimal surface area.

```plaintext
podbot run --repo owner/name --agent codex|claude
podbot token-daemon
podbot ps
podbot stop <container>
podbot exec <container> <command>
```

The `run` subcommand orchestrates the full execution flow. The `token-daemon`
subcommand can run standalone, potentially as a user systemd service, to manage
token refresh independently of active sessions.

## Module structure

A suggested organisation for maintainability:

```plaintext
src/
├── main.rs             # Configuration loading, subcommand dispatch
├── config/             # Configuration module (CLI + structs + tests)
│   ├── mod.rs          # Module docs and re-exports
│   ├── cli.rs          # Clap argument definitions
│   ├── types.rs        # AppConfig, GitHubConfig, SandboxConfig, AgentConfig
│   └── tests.rs        # Unit tests for configuration types
├── engine.rs           # Bollard wrapper: connect, create, upload, exec
├── github.rs           # Octocrab App authentication, token acquisition
├── token_daemon.rs     # Token refresh loop, atomic file writes
└── run_flow.rs         # Orchestration of steps 1–7
```

The `engine.rs` module encapsulates all Bollard interactions, providing a
testable abstraction over container operations. The `github.rs` module handles
Octocrab configuration and token acquisition without exposing API details to
calling code.

## Container image requirements

The sandbox image must support nested Podman execution. Required components:

- `/dev/fuse` access for `fuse-overlayfs` storage driver
- User namespace support and `slirp4netns` for rootless networking
- Appropriate capabilities or security options depending on Security-Enhanced
  Linux (SELinux) policy

The image should pre-install Git, the agent binaries, and a `GIT_ASKPASS`
helper script that reads `/run/secrets/ghapp_token`.

## Threat model summary

The design accepts that the agent can damage the cloned repository and make
network requests from within the sandbox. The design prevents the agent from:

- Accessing the host container socket
- Mounting arbitrary host paths via nested containers
- Observing or modifying credentials beyond the scoped installation token
- Persisting changes outside the container filesystem

The residual risk is kernel-level container escape. Operators requiring
stronger guarantees should consider VM-based isolation.

______________________________________________________________________

[^1]: Bollard Docker struct documentation:
      <https://docs.rs/bollard/latest/bollard/struct.Docker.html>

[^2]: Octocrab builder and installation token documentation:
      <https://docs.rs/octocrab/latest/octocrab/struct.OctocrabBuilder.html>

[^3]: GitHub App installation authentication:
      <https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/authenticating-as-a-github-app-installation>

[^4]: Claude Code best practices, including permissive mode:
      <https://www.anthropic.com/engineering/claude-code-best-practices>

[^5]: Codex CLI security documentation:
      <https://developers.openai.com/codex/security/>

[^6]: OrthoConfig repository:
      <https://github.com/leynos/ortho-config>
