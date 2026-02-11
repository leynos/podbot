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

| Option         | Required | Default  | Description                        |
| -------------- | -------- | -------- | ---------------------------------- |
| `--repo`       | Yes      | -        | Repository in owner/name format    |
| `--branch`     | Yes      | -        | Branch to check out                |
| `--agent`      | No       | `claude` | Agent type: `claude` or `codex`    |
| `--agent-mode` | No       | `podbot` | Agent execution mode (podbot only) |

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

[agent]
# Default agent type: "claude" or "codex"
kind = "claude"
# Execution mode for the agent (currently only "podbot")
mode = "podbot"

[workspace]
# Base directory for cloned repositories inside the container
base_dir = "/work"

[creds]
# Copy credentials from the host into the container
copy_claude = true
copy_codex = true
```

### Environment variables

All configuration options can be set via environment variables using the
`PODBOT_` prefix:

| Variable                         | Configuration key         |
| -------------------------------- | ------------------------- |
| `PODBOT_ENGINE_SOCKET`           | `engine_socket`           |
| `PODBOT_IMAGE`                   | `image`                   |
| `PODBOT_GITHUB_APP_ID`           | `github.app_id`           |
| `PODBOT_GITHUB_INSTALLATION_ID`  | `github.installation_id`  |
| `PODBOT_GITHUB_PRIVATE_KEY_PATH` | `github.private_key_path` |
| `PODBOT_SANDBOX_PRIVILEGED`      | `sandbox.privileged`      |
| `PODBOT_SANDBOX_MOUNT_DEV_FUSE`  | `sandbox.mount_dev_fuse`  |
| `PODBOT_AGENT_KIND`              | `agent.kind`              |
| `PODBOT_AGENT_MODE`              | `agent.mode`              |
| `PODBOT_WORKSPACE_BASE_DIR`      | `workspace.base_dir`      |
| `PODBOT_CREDS_COPY_CLAUDE`       | `creds.copy_claude`       |
| `PODBOT_CREDS_COPY_CODEX`        | `creds.copy_codex`        |

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

| Setting          | Default | Description                                  |
| ---------------- | ------- | -------------------------------------------- |
| `privileged`     | `false` | Run container in privileged mode             |
| `mount_dev_fuse` | `true`  | Mount `/dev/fuse` for fuse-overlayfs support |

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

**Disabling /dev/fuse**: `mount_dev_fuse = false`

The `/dev/fuse` mount is required for fuse-overlayfs, which enables inner
Podman to function correctly. Disable this only when the agent container does
not need nested container support.

### Container creation behaviour

When podbot creates a sandbox container, it applies the following host security
settings:

- `privileged = true`: sets `HostConfig.Privileged = true` and uses engine
  defaults for capabilities, devices, and SELinux options.
- `privileged = false`: sets `HostConfig.Privileged = false`, applies
  `SecurityOpt = ["label=disable"]`, and uses a minimal profile.
- `privileged = false` and `mount_dev_fuse = true`: additionally maps
  `/dev/fuse` and adds `SYS_ADMIN` capability so `fuse-overlayfs` can run.
- `privileged = false` and `mount_dev_fuse = false`: skips `/dev/fuse` mapping
  and capability additions.

Container creation requires `image` to be configured. If it is missing, podbot
returns:

```text
missing required configuration: image
```

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
