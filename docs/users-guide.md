# `podbot` user's guide

`podbot` is a sandboxed execution environment for AI coding agents. It provides
a secure container-based sandbox for running AI coding assistants such as
Claude Code and Codex, treating the host container engine as high-trust
infrastructure while the agent container operates in a low-trust playpen.

## Installation

Build and install from source:

```bash
cargo install --path .
```

## Quick start

Run an AI agent against a GitHub repository:

```bash
podbot run --repo owner/name --branch feature-branch
```

## Command-line interface (CLI)

### Global options

| Option            | Description                            |
| ----------------- | -------------------------------------- |
| `--config PATH`   | Path to a custom configuration file    |
| `--engine-socket` | Container engine socket path or URL    |
| `--image`         | Container image to use for the sandbox |

### Subcommands

#### `run`

Run an AI agent in a sandboxed container.

```bash
podbot run --repo owner/name --branch main --agent claude
```

| Option         | Required | Default         | Description                                          |
| -------------- | -------- | --------------- | ---------------------------------------------------- |
| `--repo`       | Yes      | -               | Repository in owner/name format                      |
| `--branch`     | Yes      | -               | Branch to check out                                  |
| `--agent`      | No       | Config/defaults | Agent type: `claude`, `codex`, or `custom`           |
| `--agent-mode` | No       | Config/defaults | Agent mode; `run` accepts only `podbot` semantically |

#### `host`

Host an app-server protocol for a long-lived agent runtime.

```bash
podbot host --agent codex --agent-mode codex_app_server
```

For custom agents, you must configure `agent.command` via config file or
environment variable:

```bash
export PODBOT_AGENT_COMMAND=/usr/local/bin/my-agent
podbot host --agent custom --agent-mode acp
```

| Option         | Required | Default         | Description                                |
| -------------- | -------- | --------------- | ------------------------------------------ |
| `--agent`      | No       | Config/defaults | Agent type: `claude`, `codex`, or `custom` |
| `--agent-mode` | No       | Config/defaults | Hosted mode: `codex_app_server` or `acp`   |

#### `token-daemon`

Run the GitHub token refresh daemon for a container.

```bash
podbot token-daemon <container-id>
```

#### `ps`

List running podbot containers.

```bash
podbot ps
```

#### `stop`

Stop a running container.

```bash
podbot stop <container>
```

#### `exec`

Execute a command in a running container.

```bash
podbot exec <container> -- command arg1 arg2
```

Use attached mode by default, or detached mode with `--detach`:

```bash
# Attached mode (default): streams are forwarded to the local terminal
podbot exec <container> -- sh -lc "echo hello"

# Detached mode: no stream attachment, but podbot still waits for completion
podbot exec --detach <container> -- sh -lc "exit 7"
```

Execution behaviour:

- Attached mode forwards stdin/stdout/stderr between the local terminal and the
  container exec session.
- Detached mode does not attach streams and always uses `tty = false`.
- Protocol mode (`ExecMode::Protocol`) connects streams like attached mode but
  permanently disables TTY allocation. This mode is used internally by the
  hosting subsystem to proxy protocol bytes without TTY framing corruption.
  Library consumers can use it for non-TTY attached execution where stdout must
  remain a pure byte stream. In protocol mode, podbot forwards host stdin to
  container stdin, container stdout to host stdout, and container stderr to
  host stderr without terminal framing or interactive echo injection. Protocol
  mode uses bounded buffering (64 KiB buffers for stdin forwarding and output
  chunks) so hosted protocols can apply backpressure; if the host stdout writer
  blocks, the proxy yields and backpressure propagates to the container.
- TTY allocation is enabled only when attached mode is selected and both local
  stdin and stdout are terminals.
- When TTY is enabled, podbot sends an initial resize to the daemon. On Unix
  targets, podbot also listens for `SIGWINCH` and propagates window-size
  changes. Detached mode, protocol mode, or attached mode with TTY disabled do
  not register a resize listener. podbot reads terminal size using `stty size`;
  if that command is unavailable or returns unexpected output, resize
  propagation is skipped and execution continues.
- Protocol mode ignores daemon `StdIn` echo records, so stdout contains only
  bytes that originated from container stdout or console output. When host
  stdin reaches EOF, podbot flushes and closes container stdin before waiting
  for the command to finish.
- podbot polls exec status until the command exits, then uses the daemon exit
  code as the CLI outcome. Exit code `0` returns success. Non-zero values in
  the `1..=255` range are returned directly, negative values are mapped to `1`,
  and values above `255` are clamped to `255`.
- If the daemon reports completion without an exit code, podbot returns an exec
  failure instead of guessing the result.

## Configuration

Configuration can be provided via:

1. Command-line arguments (highest precedence)
2. Environment variables
3. Configuration file
4. Built-in defaults (lowest precedence)

### Configuration file

Configuration files are discovered in the following order (first match wins):

1. Path specified via `--config` CLI argument
2. Path specified via `PODBOT_CONFIG_PATH` environment variable
3. The XDG Base Directory Specification configuration directory
   (`$XDG_CONFIG_HOME/podbot/config.toml`, typically
   `~/.config/podbot/config.toml`)
4. `~/.podbot.toml` (dotfile in home directory)

If `--config` is provided, `PODBOT_CONFIG_PATH` is ignored. If the `--config`
path does not exist, podbot falls back only to the remaining file-based
discovery locations (`$XDG_CONFIG_HOME/podbot/config.toml` and
`~/.podbot.toml`) and does not consult `PODBOT_CONFIG_PATH`.

**Note:** GitHub App credentials (`app_id`, `installation_id`,
`private_key_path`) are validated only when GitHub operations are performed.
Commands like `podbot ps` or `podbot stop` do not require GitHub configuration.

```toml
# Container engine socket (Podman or Docker)
# Unix socket (default for local daemons):
engine_socket = "unix:///run/user/1000/podman/podman.sock"
# TCP endpoint (for remote daemons):
# engine_socket = "tcp://docker.example.com:2375"

# Container image for the sandbox
image = "ghcr.io/example/podbot-sandbox:latest"

[github]
# GitHub App credentials (optional, for private repositories)
app_id = 12345
installation_id = 67890
private_key_path = "/home/user/.config/podbot/github-app.pem"

[sandbox]
# Run the container in privileged mode (less secure, more compatible)
privileged = false
# Mount /dev/fuse for fuse-overlayfs support (required for inner Podman)
mount_dev_fuse = true
# SELinux label handling: "disable_for_container" or "keep_default"
selinux_label_mode = "disable_for_container"

[agent]
# Default agent type: "claude", "codex", or "custom"
kind = "claude"
# Execution mode for the agent: "podbot", "codex_app_server", or "acp"
mode = "podbot"
# Custom launcher command (required when kind = "custom")
command = "opencode"
# Additional launcher arguments (defaults to [])
args = ["acp"]
# Environment variables copied from the host (defaults to [])
env_allowlist = ["OPENAI_API_KEY", "ANTHROPIC_API_KEY"]

[workspace]
# Workspace source: "github_clone" or "host_mount"
source = "github_clone"
# Base directory for cloned repositories inside the container
base_dir = "/work"
# Host path mounted into the sandbox when source = "host_mount"
host_path = "/abs/path/to/project"
# Container path for the mount; defaults to "/workspace" in host_mount mode
container_path = "/workspace/project"

[creds]
# Copy credentials from the host into the container
copy_claude = true
copy_codex = true

[mcp]
# HTTP bridge reachability strategy for hosted MCP servers
bind_strategy = "host_gateway"
# Idle timeout in seconds for hosted MCP bridges
idle_timeout_secs = 900
# Maximum message size in bytes
max_message_size_bytes = 1048576
# Token issuance policy: "per_workspace" or "per_wire"
auth_token_policy = "per_workspace"
# Allowed-origin policy: "same_origin" or "any"
allowed_origin_policy = "same_origin"
```

Semantic validation rules:

- `podbot run` accepts only `agent.mode = "podbot"`.
- `podbot host` accepts only `agent.mode = "codex_app_server"` or `"acp"`.
- `agent.kind = "custom"` requires a non-empty `agent.command`.
- Built-in agent kinds reject `agent.command` and `agent.args`.
- `workspace.source = "host_mount"` requires `workspace.host_path` and
  defaults `workspace.container_path` to `"/workspace"` when omitted.
- `workspace.source = "github_clone"` rejects host-mount-only fields.

### Private key file requirements

The `private_key_path` field must point to a PEM-encoded RSA private key.
GitHub App authentication uses the RS256 algorithm exclusively, so only RSA
keys are supported.

Accepted formats:

- **PKCS#1:** header `-----BEGIN RSA PRIVATE KEY-----`
- **PKCS#8:** header `-----BEGIN PRIVATE KEY-----` (must contain an RSA key)

To generate a suitable key:

```bash
openssl genrsa -out github-app.pem 2048
```

Common error messages when loading the key:

| Message                                             | Cause                                                                          |
| --------------------------------------------------- | ------------------------------------------------------------------------------ |
| "file is empty"                                     | The key file exists but contains no data.                                      |
| "failed to read file"                               | The file does not exist or cannot be read.                                     |
| "the file appears to contain an ECDSA key"          | An EC key was provided instead of RSA.                                         |
| "the file appears to contain an OpenSSH-format key" | An OpenSSH key was provided; convert with `ssh-keygen -p -m pem -f <keyfile>`. |
| "invalid RSA private key"                           | The file contents are not valid PEM-encoded RSA data.                          |

### Credential validation errors

After loading the private key successfully, podbot validates the credentials
against the GitHub API by making an authenticated request to `GET /app`. If
this validation fails, podbot classifies the failure mode and provides
actionable error messages with remediation hints:

| Message fragment                            | Cause and remediation                                                                                                                                                                     |
| ------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| "credentials rejected (HTTP 401)"           | The private key does not match the App, or the App has been suspended. Verify the App ID and regenerate the private key from the GitHub App settings page. Check for clock skew.          |
| "insufficient permissions (HTTP 403)"       | The App lacks required permissions. Check the App's permission settings in GitHub.                                                                                                        |
| "App not found (HTTP 404)"                  | The App ID is incorrect or the App has been deleted. Verify that `github.app_id` is correct.                                                                                              |
| "GitHub API unavailable (HTTP 5xx)"         | GitHub is experiencing an outage or maintenance. Check <https://www.githubstatus.com> for service status. Retry after the service recovers.                                               |
| "failed to validate GitHub App credentials" | A network error occurred or the API returned an unexpected status. Check network connectivity and DNS resolution. Review the detailed error message for the specific cause.               |

### Environment variables

All configuration options can be set via environment variables using the
`PODBOT_` prefix:

| Variable                            | Configuration key            |
| ----------------------------------- | ---------------------------- |
| `PODBOT_ENGINE_SOCKET`              | `engine_socket`              |
| `PODBOT_IMAGE`                      | `image`                      |
| `PODBOT_GITHUB_APP_ID`              | `github.app_id`              |
| `PODBOT_GITHUB_INSTALLATION_ID`     | `github.installation_id`     |
| `PODBOT_GITHUB_PRIVATE_KEY_PATH`    | `github.private_key_path`    |
| `PODBOT_SANDBOX_PRIVILEGED`         | `sandbox.privileged`         |
| `PODBOT_SANDBOX_MOUNT_DEV_FUSE`     | `sandbox.mount_dev_fuse`     |
| `PODBOT_SANDBOX_SELINUX_LABEL_MODE` | `sandbox.selinux_label_mode` |
| `PODBOT_AGENT_KIND`                 | `agent.kind`                 |
| `PODBOT_AGENT_MODE`                 | `agent.mode`                 |
| `PODBOT_AGENT_COMMAND`              | `agent.command`              |
| `PODBOT_AGENT_ARGS`                 | `agent.args`                 |
| `PODBOT_AGENT_ENV_ALLOWLIST`        | `agent.env_allowlist`        |
| `PODBOT_WORKSPACE_SOURCE`           | `workspace.source`           |
| `PODBOT_WORKSPACE_BASE_DIR`         | `workspace.base_dir`         |
| `PODBOT_WORKSPACE_HOST_PATH`        | `workspace.host_path`        |
| `PODBOT_WORKSPACE_CONTAINER_PATH`   | `workspace.container_path`   |
| `PODBOT_CREDS_COPY_CLAUDE`          | `creds.copy_claude`          |
| `PODBOT_CREDS_COPY_CODEX`           | `creds.copy_codex`           |
| `PODBOT_MCP_BIND_STRATEGY`          | `mcp.bind_strategy`          |
| `PODBOT_MCP_IDLE_TIMEOUT_SECS`      | `mcp.idle_timeout_secs`      |
| `PODBOT_MCP_MAX_MESSAGE_SIZE_BYTES` | `mcp.max_message_size_bytes` |
| `PODBOT_MCP_AUTH_TOKEN_POLICY`      | `mcp.auth_token_policy`      |
| `PODBOT_MCP_ALLOWED_ORIGIN_POLICY`  | `mcp.allowed_origin_policy`  |

`PODBOT_AGENT_ARGS` and `PODBOT_AGENT_ENV_ALLOWLIST` use comma-separated values.

### Container engine socket

The socket endpoint for connecting to Docker or Podman is resolved in the
following order (first match wins):

1. `--engine-socket` CLI argument
2. `engine_socket` in configuration file
3. `PODBOT_ENGINE_SOCKET` environment variable
4. `DOCKER_HOST` environment variable
5. `CONTAINER_HOST` environment variable
6. `PODMAN_HOST` environment variable
7. Platform default (`unix:///var/run/docker.sock` on Unix,
   `npipe:////./pipe/docker_engine` on Windows)

This allows podbot to integrate with existing Docker and Podman environments
without additional configuration. When `DOCKER_HOST` or `PODMAN_HOST` is
already set for container tooling, podbot will automatically use that endpoint.

### TCP endpoint support

In addition to Unix sockets and Windows named pipes, podbot supports TCP
connections to remote container engines. This is useful when the Docker or
Podman daemon is running on a different host or is configured to listen on a
TCP port.

**Supported TCP endpoint formats:**

| Format              | Example                           | Notes                             |
| ------------------- | --------------------------------- | --------------------------------- |
| `tcp://host:port`   | `tcp://192.168.1.100:2375`        | Rewritten internally to `http://` |
| `http://host:port`  | `http://docker.example.com:2375`  | Used directly                     |
| `https://host:port` | `https://docker.example.com:2376` | TLS-encrypted connection          |

**Configuration examples:**

Via CLI argument:

```bash
podbot run --engine-socket tcp://remotehost:2375 --repo owner/name --branch main
```

Via environment variable:

```bash
export DOCKER_HOST=tcp://192.168.1.100:2375
podbot run --repo owner/name --branch main
```

Via configuration file:

```toml
engine_socket = "tcp://docker.example.com:2375"
```

**TCP-specific troubleshooting:**

| Error                                                      | Cause                                             | Resolution                                                                                       |
| ---------------------------------------------------------- | ------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `failed to connect to container engine: <message>`         | TCP endpoint unreachable or daemon not listening  | Verify the remote host is reachable and the daemon is configured to listen on the specified port |
| `container engine health check failed: <message>`          | Connection established but daemon did not respond | Verify the daemon is healthy: `curl http://remotehost:2375/v1.40/_ping`                          |
| `container engine health check timed out after 10 seconds` | Network latency or daemon overloaded              | Check network connectivity and daemon load                                                       |

**Security note:** TCP connections without TLS (`tcp://` and `http://`)
transmit data unencrypted. Use `https://` with TLS certificates for production
environments. Consult the Docker or Podman documentation for configuring TLS.

### Engine health check

When connecting to a container engine, podbot performs a health check to verify
the engine is responsive. This confirms the engine is operational, not just
that the socket is reachable.

**Health check behaviour:**

- A ping request is sent to the engine after establishing the connection
- The check times out after 10 seconds if the engine does not respond
- If the health check fails, podbot reports a clear error message

**Possible error messages:**

| Error                                                      | Cause                                                    |
| ---------------------------------------------------------- | -------------------------------------------------------- |
| `container engine health check failed: <message>`          | The engine did not respond correctly to the ping request |
| `container engine health check timed out after 10 seconds` | The engine took too long to respond                      |

### Connection error troubleshooting

When podbot cannot connect to the container engine, it provides actionable
error messages to help diagnose the issue.

**Possible connection errors:**

| Error                                                       | Cause                                                    | Resolution                                                                                                                                                                                              |
| ----------------------------------------------------------- | -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `permission denied accessing container socket: <path>`      | User lacks permission to access the Docker/Podman socket | Add user to the docker group: `sudo usermod -aG docker $USER && newgrp docker`. For Podman, use the rootless socket at `/run/user/$UID/podman/podman.sock` (where user ID (UID) identifies the account) |
| `container engine socket not found: <path>`                 | Socket file does not exist; daemon not running           | Start the daemon: Docker: `sudo systemctl start docker`. Podman: `systemctl --user start podman.socket`                                                                                                 |
| `failed to connect to container engine: connection refused` | Daemon not accepting connections                         | Restart the daemon service and check its status                                                                                                                                                         |

**Common permission scenarios:**

- **Docker on Linux**: By default, the Docker socket (`/var/run/docker.sock`)
   is owned by the `docker` group. Add the current user to this group:

   ```bash
   sudo usermod -aG docker $USER
   newgrp docker  # Apply group membership without logging out
   ```

- **Rootless Podman**: Use the user-level socket instead of the system socket:

   ```bash
   # Start the user socket
   systemctl --user start podman.socket

   # Configure podbot to use it
   export PODBOT_ENGINE_SOCKET="unix:///run/user/$(id -u)/podman/podman.sock"
   ```

- **Podman with sudo**: If using the system Podman socket, ensure the socket
   service is running:

   ```bash
   sudo systemctl start podman.socket
   ```

### Sandbox configuration

The `[sandbox]` section controls the security and compatibility trade-offs for
the container environment.

| Setting              | Default                   | Description                                  |
| -------------------- | ------------------------- | -------------------------------------------- |
| `privileged`         | `false`                   | Run container in privileged mode             |
| `mount_dev_fuse`     | `true`                    | Mount `/dev/fuse` for fuse-overlayfs support |
| `selinux_label_mode` | `"disable_for_container"` | SELinux label handling mode                  |

**Minimal mode** (default): `privileged = false`, `mount_dev_fuse = true`

This is the recommended configuration for most users. It provides:

- Better security isolation by avoiding the privileged flag
- Support for inner Podman via fuse-overlayfs
- Compatibility with most Podman-in-Podman workflows

**Privileged mode**: `privileged = true`

Enable privileged mode only when minimal mode does not work for the target
environment. Privileged mode:

- Provides maximum compatibility with nested container operations
- Expands the container's attack surface significantly
- Should be avoided unless specifically required
- Ignores `mount_dev_fuse` because the engine grants full device access in
  privileged mode

**Disabling /dev/fuse**: `mount_dev_fuse = false`

The `/dev/fuse` mount is required for fuse-overlayfs, which enables inner
Podman to function correctly. Disable this only when the agent container does
not need nested container support.

**SELinux label mode**: `selinux_label_mode`

Controls how SELinux labels are applied to the container process:

- `"disable_for_container"` (default): Applies
  `SecurityOpt = ["label=disable"]` so rootless nested Podman workflows do not
  fail under strict SELinux labelling. This is the recommended setting for most
  environments.
- `"keep_default"`: Leaves SELinux labelling at engine defaults. Use this
  when the host SELinux policy is already configured to permit nested container
  operations, or when SELinux enforcement is disabled system-wide.

In privileged mode, this setting is ignored because the engine governs security
labelling directly.

### Container creation behaviour

When podbot creates a sandbox container, it applies the following host security
settings:

- `privileged = true`: sets `HostConfig.Privileged = true` and uses engine
  defaults for capabilities, devices, and SELinux options. The `mount_dev_fuse`
  and `selinux_label_mode` settings are ignored.
- `privileged = false` with the default `selinux_label_mode`: sets
  `HostConfig.Privileged = false` and applies `SecurityOpt = ["label=disable"]`.
- `privileged = false` and `selinux_label_mode = "keep_default"`: sets
  `HostConfig.Privileged = false` without adding `SecurityOpt`, leaving SELinux
  labelling at engine defaults.
- `mount_dev_fuse = true` (in non-privileged mode): additionally maps
  `/dev/fuse` and adds `SYS_ADMIN` capability so `fuse-overlayfs` can run.
- `mount_dev_fuse = false` (in non-privileged mode): skips `/dev/fuse`
  mapping and capability additions.

Container creation requires `image` to be configured. If it is missing or
whitespace-only, podbot returns:

```text
missing required configuration: image
```

Podbot resolves this image from layered configuration precedence (`--image`,
then `PODBOT_IMAGE`, then file/default values). Validation occurs before the
engine create call, so no container-create request is sent when the resolved
image is empty.

### Credential injection behaviour

At sandbox startup, podbot can copy host agent credentials into the container
filesystem using a tar upload to `/root`.

- `creds.copy_claude = true` selects `~/.claude`.
- `creds.copy_codex = true` selects `~/.codex`.
- Selected directories that are missing are skipped.
- If nothing is selected or present, credential injection succeeds as a no-op
  and no upload request is sent.
- Host-side selection or archive-build failures are reported as
  `FilesystemError::IoError`.
- Daemon upload failures are reported as `ContainerError::UploadFailed`.

When credentials are uploaded, expected container paths are:

- `/root/.claude` for Claude credentials.
- `/root/.codex` for Codex credentials.

Permission bits from source files and directories are preserved in the uploaded
tar entries.

Verification notes:

1. Start a sandbox with the desired `copy_claude` and `copy_codex` settings.
2. Check which directories exist in the container:

   ```bash
   podbot exec <container> -- ls -la /root
   ```

3. Compare permission bits for a representative file between host and
   container, for example, with `stat` on each side.

## Security model

Podbot's security model is based on capability-based containment:

1. **Host socket isolation**: The Rust command-line interface (CLI) holds
   the host Podman/Docker socket. The agent container never receives access to
   this socket.

2. **Nested containers**: The agent container can run an inner Podman service
   for any nested container operations, isolated from the host.

3. **Filesystem capabilities**: The `cap-std` crate provides capabilities-
   oriented filesystem access, preventing path traversal attacks.

4. **Credential injection**: Host credentials (Claude, Codex) are copied into
   the container at startup rather than mounted, preventing runtime credential
   exfiltration.

## Error handling

Podbot uses semantic error types internally for conditions that callers might
inspect, retry, or map to specific responses:

- `ConfigError`: Configuration loading and validation errors
- `ContainerError`: Container engine and lifecycle errors
- `GitHubError`: GitHub App authentication errors
- `FilesystemError`: Filesystem operation errors

At the application boundary, these are converted to human-readable error
reports using `eyre`.

## Library API

Podbot can be embedded as a Rust library dependency in addition to its use as a
CLI tool. The `podbot::api` module exposes orchestration functions that accept
library-owned types and return typed outcomes without printing to stdout/stderr
or calling `std::process::exit`.

### Available functions

| Function                                   | Description                                     |
| ------------------------------------------ | ----------------------------------------------- |
| `podbot::api::exec(params)`                | Execute a command in a running container        |
| `podbot::api::run_agent(config)`           | Run an AI agent in a sandboxed container (stub) |
| `podbot::api::stop_container(container)`   | Stop a running container (stub)                 |
| `podbot::api::list_containers()`           | List running podbot containers (stub)           |
| `podbot::api::run_token_daemon(container)` | Run the token refresh daemon (stub)             |

### Return type

All orchestration functions return `podbot::error::Result<CommandOutcome>`:

- `CommandOutcome::Success` indicates a zero exit code.
- `CommandOutcome::CommandExit { code }` carries the non-zero exit code
  reported by the container engine.

### Example usage

```rust,no_run
use podbot::api::{CommandOutcome, ExecParams, exec};
use podbot::engine::{ContainerExecClient, ExecMode};

fn run_command(
    connector: &impl ContainerExecClient,
    runtime_handle: &tokio::runtime::Handle,
) {
    let result = exec(ExecParams {
        connector,
        container: "my-container",
        command: vec!["echo".into(), "hello".into()],
        mode: ExecMode::Attached,
        tty: false,
        runtime_handle,
    });

    match result {
        Ok(CommandOutcome::Success) => println!("Command succeeded"),
        Ok(CommandOutcome::CommandExit { code }) => {
            println!("Command exited with code {code}");
        }
        Err(e) => eprintln!("Error: {e}"),
    }
}
```

### Library embedding

Podbot can be used as a library dependency without the CLI adapter layer. The
`cli` Cargo feature controls the visibility of the `podbot::cli` module, which
contains Clap parse types. This feature is enabled by default.

To depend on Podbot as a library without the CLI types:

```toml
[dependencies]
podbot = { version = "0.1.0", default-features = false }
```

With this configuration, the `podbot::cli` module is not compiled, and the
consumer can use the library without interacting with Clap types directly.

**Note:** The `clap` crate remains a transitive dependency through
`ortho_config` at present, so it is still pulled into the dependency tree. The
feature flag controls module visibility, not the `clap` dependency itself.

#### Stable modules

The following modules are part of the stable public API:

- `podbot::api` — orchestration functions (`exec`, `run_agent`,
  `list_containers`, `stop_container`, `run_token_daemon`)
- `podbot::config` — configuration types and loaders (`AppConfig`,
  `ConfigLoadOptions`, `load_config`)
- `podbot::engine` — container engine types and traits
  (`ContainerExecClient`, `EngineConnector`, `ExecRequest`)
- `podbot::error` — semantic error hierarchy (`PodbotError`, `ConfigError`,
  `ContainerError`)

#### Internal modules

The following modules are public but internal and subject to change:

- `podbot::github` — GitHub App authentication types

#### Adapter modules

The following modules are public but gated behind Cargo features:

- `podbot::cli` — Clap parse types (requires the `cli` feature, enabled by
  default)

## Development

### Running tests

```bash
make test
```

### Running lints

```bash
make lint
```

### Checking formatting

```bash
make check-fmt
```

### Running all checks

```bash
make all
```

### Feature gate verification

The `cli` Cargo feature gates the `podbot::cli` module and the `podbot` binary
target. When modifying library code, verify that the crate compiles without the
CLI feature to ensure the library boundary remains self-contained:

```bash
cargo check --no-default-features
```

This confirms that library consumers who depend on podbot with
`default-features = false` will not encounter compilation errors caused by
unconditional imports of CLI or Clap types. The full feature matrix tested
during development is:

| Command                             | What it verifies                 |
| ----------------------------------- | -------------------------------- |
| `cargo check --no-default-features` | Library compiles without CLI     |
| `cargo check --all-features`        | Everything compiles together     |
| `make test`                         | All tests pass with all features |

### Feature gate maintenance

When adding new public modules or dependencies:

- If the module is part of the stable library boundary (`api`, `config`,
  `error`, `engine`), it must compile without the `cli` feature.
- If the module depends on `clap` or other CLI-only crates, gate it behind
  `#[cfg(feature = "cli")]` in `src/lib.rs` and mark the dependency as
  `optional = true` in `Cargo.toml`.
- Run `cargo check --no-default-features` after any change to the public
  module structure to verify the boundary is intact.

### Behavioural test infrastructure

Podbot uses [rstest-bdd](https://crates.io/crates/rstest-bdd) for
behaviour-driven development (BDD) tests alongside standard `rstest`
parametrized integration tests. The two test styles serve complementary
purposes:

- **BDD scenario tests** (`tests/bdd_*.rs`) are driven by Gherkin feature
  files in `tests/features/`. Each scenario test file declares a helper module
  (`tests/bdd_*_helpers/`) containing step definitions (`given`, `when`,
  `then`), shared state, and assertion helpers. Feature files are read at
  compile time; changes to `.feature` content may require
  `cargo clean -p podbot` to invalidate incremental compilation caches.
- **Parametrized integration tests** (`tests/library_embedding.rs` and
  others) use `rstest` fixtures and `#[case]` parameters directly, without
  feature files. These tests exercise the library API from a
  host-application perspective.

The BDD helper module layout follows a consistent pattern:

```plaintext
tests/bdd_<domain>_helpers/
  mod.rs          -- re-exports and shared types
  state.rs        -- ScenarioState struct and fixture
  steps.rs        -- Given/When step definitions
  assertions.rs   -- Then step definitions
```

Every test module and helper file must begin with a module-level (`//!`) doc
comment explaining its purpose.
