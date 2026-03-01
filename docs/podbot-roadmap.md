# Podbot development roadmap

This roadmap breaks down the implementation of the sandboxed agent runner into
achievable, measurable tasks. Each phase represents a strategic milestone, with
steps grouping related work and tasks defining atomic execution units.

Podbot is delivered through two interfaces:

- A Command-Line Interface (CLI) for terminal operators.
- A Rust library for embedding in larger agent-hosting tools.

Roadmap items must preserve this dual-delivery model.

## Phase 1: Foundation and configuration

Establish the project's dependency graph and configuration system, providing
the scaffolding upon which all other modules depend.

### Step 1.1: Core dependencies ✓

Add the foundational crates required for async runtime, container management,
GitHub integration, and cross-platform filesystem operations.

**Tasks:**

- [x] Use tokio as the async runtime with full features enabled.
- [x] Integrate bollard for Docker and Podman API access.
- [x] Enable octocrab for GitHub App authentication.
- [x] Define semantic error enums with thiserror.
- [x] Employ eyre for opaque error handling at the application boundary.
- [x] Leverage cap_std and camino for capabilities-oriented filesystem access.
- [x] Introduce OrthoConfig for layered configuration with derive support.
- [x] Use clap for command-line argument parsing.

**Completion criteria:** All dependencies compile, `make lint` passes, and the
crate builds without warnings. ✓

### Step 1.2: Error handling foundation ✓

Establish the error handling patterns that propagate through all modules.

**Tasks:**

- [x] Create a root error module defining the pattern for semantic error enums.
- [x] Configure eyre::Report as the return type for the main entry point.
- [x] Ensure no unwrap or expect calls appear outside test code.

**Completion criteria:** Error handling compiles, clippy raises no warnings
about error patterns, and the expect_used lint remains strict. ✓

### Step 1.3: Configuration module ✓

Implement the configuration system with layered precedence: CLI flags override
environment variables, which override configuration files, which override
defaults.

**Tasks:**

- [x] Define AppConfig as the root configuration structure.
- [x] Create GithubConfig for App ID, installation ID, and private key path.
- [x] Establish SandboxConfig for privileged mode and /dev/fuse mount options.
- [x] Specify AgentConfig for agent kind and execution mode.
- [x] Add WorkspaceConfig for base directory.
- [x] Implement OrthoConfig derive for layered precedence.
- [x] Support configuration file at ~/.config/podbot/config.toml.
- [x] Add validation ensuring required fields are present.

**Completion criteria:** Configuration loads from file, environment, and CLI
flags with correct precedence. Unit tests cover each layer and validation
errors. ✓

### Step 1.4: Hosting schema migration and compatibility matrix

Extend configuration types for hosting mode without breaking existing
installations.

**Tasks:**

- [ ] Add schema fields for hosting: `workspace.source`, `workspace.host_path`,
  `workspace.container_path`, `agent.command`, `agent.args`, and
  `agent.env_allowlist`.
- [ ] Add execution-mode values required for hosting mode while preserving
  `podbot` defaults for existing configurations.
- [ ] Define migration rules so legacy config files load deterministically
  without manual edits.
- [ ] Implement validation for legal combinations of subcommand, `agent.kind`,
  `agent.mode`, and `workspace.source`.
- [ ] Add a compatibility test matrix covering legacy and hosting-era
  configuration variants.

**Completion criteria:** Legacy configurations continue to load as expected,
and hosting-mode configurations validate with actionable semantic errors.

## Phase 2: Container engine integration

Implement the Bollard wrapper that manages container lifecycle, credential
injection, and both interactive and protocol-safe execution.

### Step 2.1: Engine connection ✓

Connect to the Podman or Docker socket and verify the connection.

**Tasks:**

- [x] Implement socket connection via DOCKER_HOST, CONTAINER_HOST or PODMAN_HOST
  environment variables or direct path specification.
- [x] Add a health check that verifies the engine responds.
- [x] Handle socket permission errors with actionable error messages.
- [x] Support both Unix sockets and TCP connections.

**Completion criteria:** Connection succeeds against a running Podman or Docker
daemon. Permission errors produce clear diagnostic messages. ✓

### Step 2.2: Container creation

Create containers with the security configuration required for sandboxed
execution.

**Tasks:**

- [x] Implement create_container with configurable security options.
- [x] Support privileged mode for maximum compatibility.
- [x] Support minimal mode with only /dev/fuse mounted.
- [x] Configure appropriate capabilities and security options for SELinux
  environments.
- [x] Set the container image from configuration.

**Completion criteria:** Containers start in both privileged and minimal modes.
Inner Podman executes successfully within the container.

### Step 2.3: Credential injection

Copy agent credentials into the container filesystem.

**Tasks:**

- [x] Implement upload_to_container using tar archive format.
- [x] Copy ~/.claude credentials when configured.
- [x] Copy ~/.codex credentials when configured.
- [x] Preserve file permissions during upload.
- [x] Verify credentials appear at expected paths within container.

**Completion criteria:** Credentials upload successfully. File permissions
match source. Agent binaries can read their credentials.

### Step 2.4: Interactive execution

Attach a terminal to the agent process for interactive sessions.

**Tasks:**

- [x] Implement exec with TTY attachment for interactive sessions.
- [x] Handle terminal resize signals (SIGWINCH).
- [x] Support both attached and detached execution modes.
- [x] Capture exit codes from executed commands.

**Completion criteria:** Interactive sessions work with proper terminal
handling. Resize events propagate correctly. Exit codes return accurately.

### Step 2.5: Protocol-safe execution (stdio proxy)

Provide non-TTY execution for app-server hosting protocols.

**Tasks:**

- [ ] Implement exec attachment with `tty = false` enforced.
- [ ] Implement byte-stream proxy loops: stdin -> container stdin, container
  stdout -> host stdout, and container stderr -> host stderr.
- [ ] Keep proxy buffering bounded so hosted protocols can apply backpressure.
- [ ] Ensure `podbot host` emits no non-protocol bytes to stdout while proxying.
- [ ] Add lifecycle stream-purity tests for startup, steady-state, shutdown, and
  error paths.
- [ ] Add a regression test asserting zero stdout bytes before the first proxied
  protocol byte and after the final proxied byte.

**Completion criteria:** Hosting sessions run without TTY framing, preserve
protocol byte streams, and keep stdout free from Podbot diagnostics.

### Step 2.6: ACP capability masking enforcement

Prevent ACP client-side delegation from bypassing container sandbox boundaries.

**Tasks:**

- [ ] Intercept ACP initialization and mask `terminal/*` and `fs/*` capabilities
  by default before forwarding capability metadata.
- [ ] Enforce a runtime denylist for blocked ACP methods after initialization.
- [ ] Return protocol errors for blocked methods and record denials on stderr.
- [ ] Add explicit configuration to opt in to ACP delegation when operators
  intentionally accept host-side execution.
- [ ] Add tests for handshake rewriting, blocked-method denial, and override
  behaviour.

**Completion criteria:** ACP hosting defaults to sandbox-preserving behaviour,
with deterministic and test-backed enforcement.

## Phase 3: GitHub App integration

Handle GitHub App authentication and the token lifecycle required for
repository access.

### Step 3.1: App authentication

Configure Octocrab with GitHub App credentials.

**Tasks:**

- [x] Load the private key from the configured path.
- [x] Configure OctocrabBuilder with app_id and private_key.
- [ ] Validate credentials produce a valid App token on startup.
- [ ] Handle invalid or expired App credentials with clear errors.

**Completion criteria:** App authentication succeeds against GitHub. Invalid
credentials produce actionable error messages.

### Step 3.2: Installation token acquisition

Acquire scoped installation tokens for repository access.

**Tasks:**

- [ ] Implement installation_token_with_buffer to acquire tokens with expiry
  buffer.
- [ ] Return the token string for use in Git operations.
- [ ] Handle token acquisition failures gracefully.
- [ ] Log token expiry time for debugging.

**Completion criteria:** Installation tokens acquire successfully. Tokens have
appropriate scope for repository operations.

### Step 3.3: Token daemon

Implement the background daemon that refreshes tokens before expiry.

**Tasks:**

- [ ] Create the runtime directory at $XDG_RUNTIME_DIR/podbot/\<container_id>/.
- [ ] Set directory mode 0700 and file mode 0600.
- [ ] Write the initial token to ghapp_token within the directory.
- [ ] Implement a refresh loop with a five-minute buffer before expiry.
- [ ] Write tokens atomically via rename from a temporary file.
- [ ] Handle refresh failures with retry logic.

**Completion criteria:** Tokens refresh automatically before expiry. Atomic
writes prevent partial reads. The daemon runs reliably over extended periods.

### Step 3.4: GIT_ASKPASS mechanism (Git credential helper variable)

Configure the container to use token-based Git authentication.

**Tasks:**

- [ ] Document the helper script that reads /run/secrets/ghapp_token.
- [ ] Configure the read-only bind mount for the token file.
- [ ] Set the `GIT_ASKPASS` Git credential helper environment variable in the
  container.
- [ ] Verify Git clone and fetch operations succeed after token refresh.

**Completion criteria:** Git operations authenticate using the mounted token.
Operations continue working after token refresh without intervention.

## Phase 4: Repository and agent orchestration

Wire together the complete execution flow from container creation through agent
startup.

### Step 4.1: Git identity configuration

Configure Git identity within the container using host settings.

**Tasks:**

- [ ] Read user.name from host Git configuration.
- [ ] Read user.email from host Git configuration.
- [ ] Execute git config --global user.name within the container.
- [ ] Execute git config --global user.email within the container.
- [ ] Handle missing Git identity with a warning rather than failure.

**Completion criteria:** Git commits within the container use the configured
identity. Missing identity produces a warning but does not block execution.

### Step 4.2: Repository cloning

Clone the target repository into the container workspace.

**Tasks:**

- [ ] Accept repository in owner/name format.
- [ ] Require the --branch flag with no default value.
- [ ] Clone using GIT_ASKPASS for authentication.
- [ ] Clone to the configured workspace.base_dir path.
- [ ] Verify the clone completes successfully.

**Completion criteria:** Repository clones with the specified branch checked
out. Authentication uses the token mechanism without exposing credentials in
process arguments.

### Step 4.3a: Interactive agent startup

Launch the agent in permissive mode and attach the terminal.

**Tasks:**

- [ ] Implement orchestration for interactive mode (`agent.mode = "podbot"`).
- [ ] Start Claude Code with `--dangerously-skip-permissions`.
- [ ] Start Codex with `--dangerously-bypass-approvals-and-sandbox`.
- [ ] Attach the terminal to the agent process.
- [ ] Handle agent exit and cleanup.

**Completion criteria:** Interactive agents start in permissive mode, terminal
interaction works correctly, and cleanup occurs on agent exit.

### Step 4.3b: App server startup

Launch long-lived app servers for IDE and protocol clients.

**Tasks:**

- [ ] Add Codex App Server startup support
  (`codex app-server --listen stdio://`).
- [ ] Add ACP startup support through generic command execution
  (`agent.command` + `agent.args`).
- [ ] Route hosting sessions through the Step 2.5 non-TTY stdio proxy.
- [ ] Route protocol hosting through the dedicated `host` command path.
- [ ] Add config validation for legal `(agent.kind, agent.mode)` pairs.
- [ ] Ensure clean shutdown semantics when the client closes stdin or sends a
  termination signal.

**Completion criteria:** App server modes start reliably, run through protocol-
safe proxying, and shut down cleanly when the hosting client ends the session.

### Step 4.4: Workspace strategies

Support both cloned and host-mounted workspace models.

**Tasks:**

- [ ] Implement `workspace.source = "host_mount"` bind mounts.
- [ ] Retain `workspace.source = "github_clone"` for token-backed clone flows.
- [ ] Define path mapping policy (default mount target `/workspace`).
- [ ] Canonicalize host paths before mounting and reject unresolved symlink
  escapes.
- [ ] Enforce allowlisted host mount roots.
- [ ] Ensure container user write permissions are documented and validated for
  rootless engines.
- [ ] Update threat-model documentation for host-mounted workspace boundaries.
- [ ] Add negative tests for forbidden mount paths and boundary violations.

**Completion criteria:** Operators can choose clone or host-mount workspace
strategies explicitly, with documented security and permission behaviour.

### Step 4.5: Normalized launch contract

Centralize launch validation and normalization across interactive and hosting
flows.

**Tasks:**

- [ ] Define a library-level `LaunchRequest` model for agent kind, mode,
  workspace source, and credential policy.
- [ ] Define a normalized `LaunchPlan` that resolves command, args, env policy,
  mount policy, and stream policy.
- [ ] Update orchestration internals so `run` and `host` both use the same
  normalization path.
- [ ] Add tests that assert consistent normalization outcomes across command
  entry points.

**Completion criteria:** Launch behaviour is defined once in library code and
is consistent across CLI commands.

## Phase 5: Library API and embedding support

Expose Podbot orchestration as a stable library API that can be called by
external host applications without shelling out to the CLI.

### Step 5.1: Extract command orchestration into library modules

Move subcommand behaviour from binary entrypoint code into reusable library
functions.

**Tasks:**

- [ ] Introduce a public orchestration module for run, exec, stop, ps, and
  token daemon operations.
- [ ] Replace binary-local orchestration logic with calls into library
  orchestration functions.
- [ ] Ensure orchestration returns typed outcomes rather than printing
  directly.
- [ ] Keep side-effecting process control (`std::process::exit`) in the CLI
  adapter only.

**Completion criteria:** All command flows are invocable through library APIs.
The binary becomes a thin adapter layer over library orchestration.

### Step 5.2: Decouple configuration APIs from Clap

Ensure embedders can configure Podbot without constructing CLI parse types.

**Tasks:**

- [ ] Add a library-facing configuration loader that accepts explicit load
  options and overrides.
- [ ] Keep Clap-dependent structures in a CLI adapter layer.
- [ ] Provide conversion helpers from parsed CLI flags into library load
  options.
- [ ] Add tests for library configuration loading independent of CLI parsing.

**Completion criteria:** Library consumers can resolve `AppConfig` without
using `clap::Parser` or `Cli` structs.

### Step 5.3: Stabilize public library boundaries

Define and document the supported long-term API surface.

**Tasks:**

- [ ] Document supported public modules and request/response types.
- [ ] Ensure public APIs use semantic errors (`PodbotError`) and avoid opaque
  `eyre` types.
- [ ] Gate CLI-only dependencies and code paths behind a binary or feature
  boundary.
- [ ] Add integration tests that embed Podbot as a library from a host-style
  call path.

**Completion criteria:** Podbot can be integrated as a dependency in another
Rust tool with documented, versioned APIs and no CLI coupling requirements.

## Phase 6: CLI

Complete the user-facing command interface with all subcommands.

### Step 6.1: Subcommand dispatch

Implement the argument parsing and subcommand routing.

**Tasks:**

- [ ] Define the run subcommand for launching agent sessions.
- [ ] Define the host subcommand for protocol-only app server hosting.
- [ ] Add the token-daemon subcommand for standalone token management.
- [ ] Create the ps subcommand for listing containers.
- [ ] Implement the stop subcommand for terminating containers.
- [ ] Provide the exec subcommand for running commands in containers.
- [ ] Validate required arguments per subcommand.

**Completion criteria:** All subcommands parse correctly. Help text describes
each command and its arguments. Invalid arguments produce clear errors.

### Step 6.2: Run subcommand

Implement the interactive workflow for launching terminal-attached sessions.

**Tasks:**

- [ ] Accept --repo owner/name as a required argument.
- [ ] Accept --branch as a required argument.
- [ ] Accept --agent with values `codex`, `claude`, or `custom`.
- [ ] Accept `--agent-mode podbot` only for this command.
- [ ] Reject hosting modes (`codex_app_server` and `acp`) with a clear message
  directing operators to `podbot host`.
- [ ] Orchestrate the interactive execution flow from the run module.
- [ ] Return appropriate exit codes on success and failure.

**Completion criteria:** The run command launches interactive sessions only.
Hosting-mode requests are rejected with actionable guidance.

### Step 6.3: Management subcommands

Implement commands for managing running containers.

**Tasks:**

- [ ] Add ps to list active Podbot containers with status.
- [ ] Create stop to terminate a container by ID or name.
- [ ] Provide exec to run arbitrary commands within a container.
- [ ] Format output for readability.

**Completion criteria:** Management commands operate correctly against running
containers. Output formats are clear and consistent.

### Step 6.4: Token daemon subcommand

Support standalone token daemon execution.

**Tasks:**

- [ ] Accept container ID as an argument.
- [ ] Support execution as a user systemd service.
- [ ] Implement graceful shutdown on SIGTERM.
- [ ] Log token refresh events.

**Completion criteria:** The daemon runs independently of agent sessions.
Systemd integration works correctly. Shutdown handles cleanly.

### Step 6.5: Host subcommand

Implement the dedicated protocol-only bridge command.

**Tasks:**

- [ ] Add `podbot host` for `agent-mode` values `codex_app_server` and `acp`.
- [ ] Wire the command to non-TTY proxy orchestration with strict stdout purity.
- [ ] Ensure all lifecycle diagnostics for `podbot host` are emitted on stderr.
- [ ] Handle client disconnect and signal-driven shutdown cleanly.
- [ ] Return explicit non-zero exit codes when protocol setup fails.

**Completion criteria:** `podbot host` behaves as a protocol-clean transport
adapter and does not share interactive `run` output concerns.

## Phase 7: Container image

Create the sandbox container image with all required components.

### Step 7.1: Base image definition

Define the Containerfile for the sandbox environment.

**Tasks:**

- [ ] Select an appropriate base image with Podman support.
- [ ] Install podman, fuse-overlayfs, and slirp4netns packages.
- [ ] Install git and required utilities.
- [ ] Configure user namespace support.
- [ ] Set appropriate default user and working directory.

**Completion criteria:** The image builds successfully. Inner Podman executes
within the container. Git operations function correctly.

### Step 7.2: Agent runtimes and binaries

Add required runtimes and agent binaries to the image.

**Tasks:**

- [ ] Add Claude Code binary or installation method.
- [ ] Add Codex CLI binary or installation method.
- [ ] Add Node.js runtime for OpenCode and Droid ACP tooling.
- [ ] Add Python 3.10+ runtime for Claude Agent SDK wrappers.
- [ ] Add OpenCode installation method and document Droid ACP dependency
  requirements.
- [ ] Add Goose installation method and ACP-mode invocation documentation.
- [ ] Verify each installed runtime and agent command executes correctly within
  the container.
- [ ] Document versioning and upgrade procedures for runtimes and agents.

**Completion criteria:** Required runtimes and hosted agent commands execute
within the container, with documented and repeatable upgrade paths.

### Step 7.3: GIT_ASKPASS helper

Install the helper script for token-based authentication.

**Tasks:**

- [ ] Write the helper script that reads /run/secrets/ghapp_token.
- [ ] Install at a known path (e.g., /usr/local/bin/git-askpass).
- [ ] Set executable permissions.
- [ ] Configure as the default GIT_ASKPASS in the image.

**Completion criteria:** The helper script reads tokens correctly. Git
operations authenticate using the helper. Permissions are appropriate.

### Step 7.4: Image build automation

Automate image building and distribution.

**Tasks:**

- [ ] Add a Makefile target for local image builds.
- [ ] Configure Continuous Integration (CI) workflow to build on changes.
- [ ] Push images to a container registry.
- [ ] Document the image versioning strategy.
- [ ] Add image verification tests.

**Completion criteria:** Images build automatically in CI. Registry pushes
succeed. Version tags follow a documented convention.

## Phase 8: Protocol conformance and hosting tests

Validate protocol correctness for app server hosting integrations.

### Step 8.1: Codex App Server integration test

Verify Podbot can host Codex App Server end-to-end.

**Tasks:**

- [ ] Add integration coverage that launches `podbot host` with
  `codex app-server --listen stdio://`.
- [ ] Drive `initialize -> new thread -> prompt` through the Codex app-server
  test client flow.
- [ ] Assert no Podbot diagnostics are emitted on stdout during protocol
  traffic.

**Completion criteria:** Codex client test flow succeeds against Podbot hosting
without stdout protocol contamination.

### Step 8.2: ACP transport conformance harness

Verify Podbot preserves ACP framing and stream purity.

**Tasks:**

- [ ] Build a minimal ACP harness that exchanges newline-delimited JSON-RPC
  messages through Podbot hosting.
- [ ] Assert newline framing is preserved exactly with no embedded newline
  corruption.
- [ ] Assert Podbot emits no stray stdout bytes outside proxied ACP protocol
  traffic.
- [ ] Add tests for ACP capability masking of `terminal/*` and `fs/*` in the
  initialization handshake.
- [ ] Add tests for runtime denial of blocked ACP methods after initialization.

**Completion criteria:** ACP sessions remain protocol-correct under Podbot
hosting, with default sandbox-preserving capability masking enforced.

### Step 8.3: Host lifecycle and output-purity tests

Validate protocol cleanliness across full process lifecycle transitions.

**Tasks:**

- [ ] Assert `podbot host` emits no stdout bytes before the first proxied
  protocol byte.
- [ ] Assert `podbot host` emits no stdout bytes after client disconnect and
  shutdown.
- [ ] Exercise signal-driven termination paths and assert stderr-only
  diagnostics.
- [ ] Verify partial-frame and backpressure scenarios do not corrupt framing.

**Completion criteria:** Protocol sessions remain stream-clean and framing-safe
across startup, steady-state, and shutdown paths.

## Future enhancements

The following items are documented for future consideration but are not part of
the initial implementation roadmap:

- [ ] **Network egress restriction:** Limit container network access to model
  endpoints and GitHub only, reducing prompt injection risk.
- [ ] **Virtual machine isolation:** Provide VM-based execution for environments
  requiring stronger isolation guarantees than container boundaries.
- [ ] **Multi-repository support:** Allow agents to access multiple repositories
  within a single session.
- [ ] **Session persistence:** Save and restore agent sessions across container
  restarts.
