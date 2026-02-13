# Podbot development roadmap

This roadmap breaks down the implementation of the sandboxed agent runner into
achievable, measurable tasks. Each phase represents a strategic milestone, with
steps grouping related work and tasks defining atomic execution units.

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

## Phase 2: Container engine integration

Implement the Bollard wrapper that manages container lifecycle, credential
injection, and interactive execution.

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
- [ ] Support minimal mode with only /dev/fuse mounted.
- [ ] Configure appropriate capabilities and security options for SELinux
  environments.
- [ ] Set the container image from configuration.

**Completion criteria:** Containers start in both privileged and minimal modes.
Inner Podman executes successfully within the container.

### Step 2.3: Credential injection

Copy agent credentials into the container filesystem.

**Tasks:**

- [ ] Implement upload_to_container using tar archive format.
- [ ] Copy ~/.claude credentials when configured.
- [ ] Copy ~/.codex credentials when configured.
- [ ] Preserve file permissions during upload.
- [ ] Verify credentials appear at expected paths within container.

**Completion criteria:** Credentials upload successfully. File permissions
match source. Agent binaries can read their credentials.

### Step 2.4: Interactive execution

Attach a terminal to the agent process for interactive sessions.

**Tasks:**

- [ ] Implement exec with TTY attachment for interactive sessions.
- [ ] Handle terminal resize signals (SIGWINCH).
- [ ] Support both attached and detached execution modes.
- [ ] Capture exit codes from executed commands.

**Completion criteria:** Interactive sessions work with proper terminal
handling. Resize events propagate correctly. Exit codes return accurately.

## Phase 3: GitHub App integration

Handle GitHub App authentication and the token lifecycle required for
repository access.

### Step 3.1: App authentication

Configure Octocrab with GitHub App credentials.

**Tasks:**

- [ ] Load the private key from the configured path.
- [ ] Configure OctocrabBuilder with app_id and private_key.
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

- [ ] Create the runtime directory at $XDG_RUNTIME_DIR/podbot/<container_id>/.
- [ ] Set directory mode 0700 and file mode 0600.
- [ ] Write the initial token to ghapp_token within the directory.
- [ ] Implement a refresh loop with a five-minute buffer before expiry.
- [ ] Write tokens atomically via rename from a temporary file.
- [ ] Handle refresh failures with retry logic.

**Completion criteria:** Tokens refresh automatically before expiry. Atomic
writes prevent partial reads. The daemon runs reliably over extended periods.

### Step 3.4: GIT_ASKPASS mechanism

Configure the container to use token-based Git authentication.

**Tasks:**

- [ ] Document the helper script that reads /run/secrets/ghapp_token.
- [ ] Configure the read-only bind mount for the token file.
- [ ] Set GIT_ASKPASS environment variable in the container.
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

### Step 4.3: Agent startup

Launch the agent in permissive mode and attach the terminal.

**Tasks:**

- [ ] Implement the orchestration of steps one through seven from the design
  document.
- [ ] Start Claude Code with --dangerously-skip-permissions flag.
- [ ] Start Codex with --dangerously-bypass-approvals-and-sandbox
  flag.
- [ ] Attach the terminal to the agent process.
- [ ] Handle agent exit and cleanup.

**Completion criteria:** Agents start successfully in permissive mode. Terminal
interaction works correctly. Container cleanup occurs on agent exit.

## Phase 5: CLI

Complete the user-facing command interface with all subcommands.

### Step 5.1: Subcommand dispatch

Implement the argument parsing and subcommand routing.

**Tasks:**

- [ ] Define the run subcommand for launching agent sessions.
- [ ] Add the token-daemon subcommand for standalone token management.
- [ ] Create the ps subcommand for listing containers.
- [ ] Implement the stop subcommand for terminating containers.
- [ ] Provide the exec subcommand for running commands in containers.
- [ ] Validate required arguments per subcommand.

**Completion criteria:** All subcommands parse correctly. Help text describes
each command and its arguments. Invalid arguments produce clear errors.

### Step 5.2: Run subcommand

Implement the primary workflow for launching agent sessions.

**Tasks:**

- [ ] Accept --repo owner/name as a required argument.
- [ ] Accept --branch as a required argument.
- [ ] Accept --agent with values codex or claude.
- [ ] Orchestrate the full execution flow from the run_flow module.
- [ ] Return appropriate exit codes on success and failure.

**Completion criteria:** The run command launches complete agent sessions.
Required arguments enforce presence. The full orchestration executes correctly.

### Step 5.3: Management subcommands

Implement commands for managing running containers.

**Tasks:**

- [ ] Add ps to list active podbot containers with status.
- [ ] Create stop to terminate a container by ID or name.
- [ ] Provide exec to run arbitrary commands within a container.
- [ ] Format output for readability.

**Completion criteria:** Management commands operate correctly against running
containers. Output formats are clear and consistent.

### Step 5.4: Token daemon subcommand

Support standalone token daemon execution.

**Tasks:**

- [ ] Accept container ID as an argument.
- [ ] Support execution as a user systemd service.
- [ ] Implement graceful shutdown on SIGTERM.
- [ ] Log token refresh events.

**Completion criteria:** The daemon runs independently of agent sessions.
Systemd integration works correctly. Shutdown handles cleanly.

## Phase 6: Container image

Create the sandbox container image with all required components.

### Step 6.1: Base image definition

Define the Containerfile for the sandbox environment.

**Tasks:**

- [ ] Select an appropriate base image with Podman support.
- [ ] Install podman, fuse-overlayfs, and slirp4netns packages.
- [ ] Install git and required utilities.
- [ ] Configure user namespace support.
- [ ] Set appropriate default user and working directory.

**Completion criteria:** The image builds successfully. Inner Podman executes
within the container. Git operations function correctly.

### Step 6.2: Agent binaries

Add the AI agent binaries to the image.

**Tasks:**

- [ ] Add Claude Code binary or installation method.
- [ ] Add Codex CLI binary or installation method.
- [ ] Verify binaries execute correctly within the container.
- [ ] Document binary update procedures.

**Completion criteria:** Both agent binaries execute within the container.
Version information displays correctly.

### Step 6.3: GIT_ASKPASS helper

Install the helper script for token-based authentication.

**Tasks:**

- [ ] Write the helper script that reads /run/secrets/ghapp_token.
- [ ] Install at a known path (e.g., /usr/local/bin/git-askpass).
- [ ] Set executable permissions.
- [ ] Configure as the default GIT_ASKPASS in the image.

**Completion criteria:** The helper script reads tokens correctly. Git
operations authenticate using the helper. Permissions are appropriate.

### Step 6.4: Image build automation

Automate image building and distribution.

**Tasks:**

- [ ] Add a Makefile target for local image builds.
- [ ] Configure Continuous Integration (CI) workflow to build on changes.
- [ ] Push images to a container registry.
- [ ] Document the image versioning strategy.
- [ ] Add image verification tests.

**Completion criteria:** Images build automatically in CI. Registry pushes
succeed. Version tags follow a documented convention.

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
