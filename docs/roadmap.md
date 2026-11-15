# Podbot development roadmap

This roadmap describes the delivery order for Podbot, the sandboxed agent
runner described in [podbot-design.md](podbot-design.md) and refined by the
hosting, Corbusier conformance, and Architectural Decision Record (ADR)
documents in this directory. It is an implementation guide, not a date
commitment.

The structure follows Goals, Ideas, Steps, Tasks (GIST): each phase states a
testable idea, each step is a workstream that validates or falsifies part of
that idea, and each numbered task is a review-sized execution unit. Tasks
include dependencies, design signposts, and completion criteria where the
finish line is not obvious.

Podbot is delivered through two first-class surfaces: a Command-Line Interface
(CLI) for terminal operators, and a Rust library for embedding in larger
agent-hosting tools. Roadmap work must preserve that dual-delivery model.

Kubernetes backend work is proposed in
[RFC 0001](rfcs/0001-kubernetes-execution-backend.md) and tracked in phase 10.
The new phase preserves existing numbering and completed work. Its runtime
abstraction precedes the unfinished launch tasks; it does not wait for deferred
phase 9 extensions. Proposed tasks require the ratification gate in 10.1.1.

## 1. Foundation and configuration

Idea: if Podbot settles its dependency spine, error model, configuration
contract, and hosting-era schema before wider orchestration work, later slices
can extend one coherent architecture rather than reworking operator and
embedder interfaces repeatedly.

This phase is foundational, but it is still testable: later phases should be
able to consume semantic errors, layered configuration, and hosting defaults
without reaching back into CLI-only types or ad hoc configuration parsing.

### 1.1. Establish the dependency and build spine

This step answers whether the repository can compile the core dependency set
needed for asynchronous execution, container orchestration, GitHub App
authentication, and capabilities-oriented file access. The result informs all
later implementation because these crates define the runtime and public-error
constraints. See podbot-design.md §§Execution flow, Crate selection.

- [x] 1.1.1. Add the foundational crate set for async runtime, container
  management, GitHub App authentication, command parsing, layered
  configuration, semantic errors, opaque application errors, and
  capabilities-oriented paths.
  - Include `tokio`, `bollard`, `octocrab`, `clap`, `ortho_config`,
    `thiserror`, `eyre`, `cap_std`, and `camino`.
  - Success: the workspace compiles, `make lint` passes, and the crate builds
    without warnings.

### 1.2. Define the error handling boundary

This step proves that library code can return semantic errors while the binary
keeps opaque, human-readable reporting at the application boundary. The result
unblocks library APIs and CLI adapters that must not leak `eyre::Report`
through stable public surfaces. See podbot-design.md §§Dual delivery model,
Error handling boundary.

- [x] 1.2.1. Create the root semantic error module.
  - See docs/execplans/1-2-1-root-error-module.md.
  - Success: domain errors compile as inspectable enums and remain usable from
    library call paths.
- [x] 1.2.2. Configure `eyre::Report` only at the binary entrypoint boundary.
  - Requires 1.2.1.
  - See docs/execplans/1-2-2-eyre-report.md.
  - Success: the main entrypoint reports human-readable failures without
    exporting opaque errors from library APIs.
- [x] 1.2.3. Keep panic-prone helpers out of production code.
  - Requires 1.2.1 and 1.2.2.
  - Success: `unwrap` and `expect` remain absent from production code, and the
    strict `expect_used` policy remains enforceable.

### 1.3. Build layered configuration that both surfaces can consume

This step answers whether operators and embedders can resolve one validated
configuration model through files, environment variables, CLI flags, and
explicit library options. The result informs hosting schema migration and later
launch normalization. See podbot-design.md §§Configuration, Execution flow.

- [x] 1.3.1. Define `AppConfig` as the root configuration structure.
  - See docs/execplans/1-3-1-define-app-config.md.
  - Success: all configuration groups compose through one loadable root model.
- [x] 1.3.2. Add `GitHubConfig` for App ID, installation ID, and private key
  path.
  - Requires 1.3.1.
  - See docs/execplans/1-3-2-github-config.md.
  - Success: GitHub configuration validates required fields before
    authentication work begins.
- [x] 1.3.3. Add `SandboxConfig` for privileged mode, `/dev/fuse`, and
  Security-Enhanced Linux (SELinux) policy.
  - Requires 1.3.1.
  - See docs/execplans/1-3-3-sandbox-config.md.
  - Success: container security options can be resolved without hard-coded
    sandbox defaults.
- [x] 1.3.4. Add `AgentConfig` and `WorkspaceConfig` for agent kind,
  execution mode, and workspace base directory.
  - Requires 1.3.1.
  - See docs/execplans/1-3-4-agent-and-workspace-config.md.
  - Success: agent and workspace choices can be validated independently of CLI
    parsing.
- [x] 1.3.5. Support deterministic configuration discovery and validation.
  - Requires 1.3.1-1.3.4.
  - Include `~/.config/podbot/config.toml`, environment overrides, CLI
    overrides, defaults, and required-field validation.
  - Success: file, environment, and CLI precedence is covered by unit and
    behavioural tests.
- [x] 1.3.6. Implement `OrthoConfig` derive support for layered precedence.
  - Requires 1.3.1-1.3.5.
  - See docs/execplans/1-3-6-ortho-config-derive.md and
    docs/execplans/adopt-ortho-config-v0-8-0.md.
  - Success: configuration loading uses the derive path rather than a parallel
    hand-rolled precedence implementation.

### 1.4. Prove the schema can absorb hosting mode

This step answers whether the existing configuration model can grow protocol
hosting, workspace sources, and MCP defaults without breaking legacy
configuration files. The result unblocks protocol-safe execution and hosted
session work. See podbot-design.md §§Execution flow, Host-mount path safety
policy; mcp-server-hosting-design.md §§7-8.

- [x] 1.4.1. Add hosting-era schema fields, deterministic migration rules, and
  compatibility validation.
  - Requires 1.3.6.
  - Include `workspace.source`, `workspace.host_path`,
    `workspace.container_path`, `agent.command`, `agent.args`,
    `agent.env_allowlist`, MCP hosting defaults, and legal combinations of
    subcommand, agent kind, agent mode, and workspace source.
  - See docs/execplans/1-4-1-hosting-schema-migration.md.
  - Success: legacy configurations continue to load, hosting configurations
    validate, and illegal combinations produce actionable semantic errors.

## 2. Container execution and protocol-safe transport

Idea: if Podbot can own the host container socket, create hardened containers,
inject only selected credentials, and proxy protocol sessions without TTY
framing or stdout contamination, then the sandbox boundary is real enough for
both interactive agents and app-server hosting.

This phase delivers the first usable execution slice: connect to the engine,
create a sandbox, inject credentials, run attached sessions, and prove that
protocol hosting can reuse the execution layer without inheriting terminal
behaviour.

### 2.1. Connect to and verify the container engine

This step answers whether Podbot can discover and verify Docker or Podman
endpoints with clear diagnostics. It informs all later container lifecycle
work. See podbot-design.md §§Crate selection, Engine connection protocol
support.

- [x] 2.1.1. Implement socket connection from environment variables and direct
  endpoint paths.
  - Requires 1.3.3.
  - See docs/execplans/2-1-1-socket-connection-via-env-var.md.
  - Success: `DOCKER_HOST`, `CONTAINER_HOST`, `PODMAN_HOST`, and explicit
    paths resolve through the supported endpoint dispatcher.
- [x] 2.1.2. Add a health check that verifies the engine responds.
  - Requires 2.1.1.
  - See docs/execplans/2-1-2-health-check-call.md.
  - Success: lazy TCP failures surface during verification with clear errors.
- [x] 2.1.3. Handle socket permission errors with actionable diagnostics.
  - Requires 2.1.1.
  - See docs/execplans/2-1-3-handle-socket-permission-errors.md.
  - Success: missing and inaccessible sockets are distinguished for operator
    remediation.
- [x] 2.1.4. Support Unix sockets, named pipes, TCP, HTTP, HTTPS, and bare
  endpoint paths.
  - Requires 2.1.1 and 2.1.2.
  - See docs/execplans/2-1-4-support-both-unix-sockets-and-tcp-connections.md.
  - Success: connection dispatch matches the documented protocol matrix.

### 2.2. Create sandbox containers with reviewable security policy

This step proves that container creation can apply explicit security and image
policy without smuggling assumptions into later orchestration. It informs
credential injection, execution, and image work. See podbot-design.md
§§Execution flow, Security model.

- [x] 2.2.1. Implement container creation with configurable security options.
  - Requires 2.1.4 and 1.3.3.
  - See docs/execplans/2-2-1-create-container.md.
  - Success: callers can create a sandbox container from validated config.
- [x] 2.2.2. Support privileged mode for maximum compatibility.
  - Requires 2.2.1.
  - See docs/execplans/2-2-2-privileged-mode.md.
  - Success: privileged containers start with the expected compatibility
    settings.
- [x] 2.2.3. Support minimal mode with only `/dev/fuse` mounted.
  - Requires 2.2.1.
  - See docs/execplans/2-2-3-minimal-mode.md.
  - Success: minimal containers preserve the inner-Podman use case without
    broader privilege.
- [x] 2.2.4. Configure SELinux capabilities and security options.
  - Requires 2.2.1.
  - See docs/execplans/2-2-4-configure-se-linux-capabilities.md.
  - Success: SELinux environments can run with documented label behaviour.
- [x] 2.2.5. Set the container image from configuration.
  - Requires 1.3.5 and 2.2.1.
  - See docs/execplans/2-2-5-set-container-image-from-config.md.
  - Success: image selection is validated through configuration rather than
    hard-coded into the engine layer.

### 2.3. Inject selected agent credentials

This step answers whether Podbot can copy credential families into the sandbox
without exposing the whole host home directory. The result informs interactive
launch and hosted app-server launch. See podbot-design.md §§Execution flow,
Credential injection contract.

- [x] 2.3.1. Upload selected credential directories into the container through
  Bollard tar archives.
  - Requires 2.2.5.
  - Include Claude and Codex credential families, missing-directory no-op
    behaviour, permission preservation, deterministic reported target paths,
    and semantic upload errors.
  - See docs/execplans/2-3-1-agent-credentials.md.
  - Success: selected credential families appear at the expected in-container
    paths with preserved permissions.

### 2.4. Attach interactive execution

This step proves that the same container can host a human-operated terminal
session. The result informs the separation between interactive and protocol
execution paths. See podbot-design.md §Execution flow.

- [x] 2.4.1. Implement interactive exec with terminal attachment, resize
  propagation, attached and detached modes, exit-code capture, and cleanup.
  - Requires 2.2.5 and 2.3.1.
  - See docs/execplans/2-4-1-interactive-execution.md.
  - Success: terminal sessions work with resize propagation and accurate
    process outcomes.

### 2.5. Prove protocol execution is stdout-pure

This step answers whether hosted protocols can run through Podbot without TTY
framing, buffering surprises, or Podbot diagnostics on stdout. The result
unblocks `podbot host`, Codex App Server, ACP, and MCP bridge work. See
podbot-design.md §§Execution flow, Bounded buffering implementation;
developers-guide.md §§4-6.

- [x] 2.5.1. Enforce `tty = false` for protocol exec attachment.
  - Requires 2.4.1.
  - See docs/execplans/2-5-1-exec-attachment-with-tty-false-enforced.md.
  - Success: protocol sessions never allocate terminal framing.
- [x] 2.5.2. Implement byte-stream proxy loops for host stdin, container
  stdout, and container stderr.
  - Requires 2.5.1.
  - See docs/execplans/2-5-2-byte-stream proxy loops.md.
  - Success: proxied protocol bytes are forwarded without prefixes, newline
    rewrites, or interactive echo leakage.
- [x] 2.5.3. Bound proxy buffering and preserve stream purity through
  lifecycle edges.
  - Requires 2.5.2.
  - Include startup, steady-state, shutdown, error-path, and zero-extra-stdout
    regression coverage as implementation acceptance criteria.
  - See docs/execplans/2-5-3-keep-proxy-buffering-bounded.md.
  - Success: hosted protocol sessions apply backpressure and keep stdout free
    of Podbot diagnostics before, during, and after proxying.

### 2.6. Enforce ACP sandbox-preserving defaults

This step answers whether ACP client-side delegation can be masked before it
bypasses sandbox boundaries. The result informs validation diagnostics and
hosted protocol conformance. See podbot-design.md §Execution flow; ADR 006;
docs/corbusier-conformance-design-for-agents-mcp-wires-and-hooks.md §§Current
state and required alignment, Prompt validation request/response and capability
dispositions.

- [x] 2.6.1. Rewrite ACP initialization to mask `terminal/*` and `fs/*`
  capabilities by default.
  - Requires 2.5.3.
  - See docs/execplans/2-6-1-intercept-acp-initialization.md.
  - Success: ACP initialization no longer advertises host-executed tool
    families unless an explicit policy allows them.
- [x] 2.6.2. Enforce runtime denial for blocked ACP methods.
  - Requires 2.6.1.
  - Return protocol errors for blocked methods and record denials on stderr.
  - See docs/execplans/2-6-2-runtime-denylist.md.
  - Success: blocked ACP method calls fail deterministically after
    initialization without contaminating stdout.
- [ ] 2.6.3. Add the explicit ACP delegation override.
  - Requires 2.6.2 and 1.4.1.
  - See ADR 006 and ADR 008.
  - Success: operators can opt into host-side ACP delegation only through a
    visible trust-boundary change covered by configuration validation.

## 3. GitHub App repository access

Idea: if GitHub authentication, token refresh, and Git credential delivery are
handled outside the sandbox and exposed through a read-only token file, agents
can clone and fetch repositories without receiving long-lived host secrets.

This phase delivers the private-repository path for
`workspace.source = "github_clone"`. It is sequenced after container execution
because token files must be mounted into a concrete sandbox.

### 3.1. Authenticate as a GitHub App

This step proves that configured App credentials are loadable, construct an
Octocrab client, and can be validated against GitHub with classified errors.
The result unblocks installation token acquisition. See podbot-design.md
§§Token management, Octocrab, Credential validation contract.

- [x] 3.1.1. Load the RSA private key from the configured path.
  - Requires 1.3.2.
  - See docs/execplans/3-1-1-load-private-key-from-configured-path.md.
  - Success: supported PEM formats load and wrong key types fail clearly.
- [x] 3.1.2. Configure `OctocrabBuilder` with App ID and private key.
  - Requires 3.1.1.
  - See docs/execplans/3-1-2-configure-octocrab-builder.md.
  - Success: client construction reports missing runtime and builder failures
    as semantic authentication errors.
- [x] 3.1.3. Validate credentials against GitHub on startup.
  - Requires 3.1.2.
  - See docs/execplans/3-1-3-validate-credentials-on-startup.md.
  - Success: commands requiring GitHub access fail before launch when the App
    credentials are invalid.
- [x] 3.1.4. Classify invalid or expired App credentials with clear errors.
  - Requires 3.1.3.
  - See the 3.1.4 credential-error execplan.
  - Success: HTTP status and network failures include actionable remediation
    hints.

### 3.2. Acquire scoped installation tokens

This step answers whether Podbot can mint scoped, expiring repository tokens
without exposing GitHub App private key material to the container. The result
unblocks the token daemon and clone flow. See podbot-design.md §Token
management.

- [x] 3.2.1. Implement installation token acquisition with an expiry buffer.
  - Requires 3.1.4.
  - See docs/execplans/3-2-1-installation-token-with-buffer.md.
  - Use `installation_token_with_buffer`, return the token string for Git
    operations, handle acquisition failures semantically, and log expiry
    timing without logging token values.
  - Success: scoped tokens acquire successfully for configured installations
    and carry enough expiry metadata for refresh scheduling.

### 3.3. Refresh tokens through a host-side daemon

This step proves that repository credentials can rotate while the container
continues to read a stable in-container secret path. The result informs
`GIT_ASKPASS` and long-lived sessions. See podbot-design.md §§Execution flow,
Token management.

- [ ] 3.3.1. Implement the token daemon runtime directory and atomic token
  writer.
  - Requires 3.2.1 and 2.2.5.
  - Create `$XDG_RUNTIME_DIR/podbot/<container_id>/`, set directory mode
    `0700`, set token file mode `0600`, and write by rename from a temporary
    file.
  - Success: readers never observe partial token contents.
- [ ] 3.3.2. Implement the refresh loop and retry policy.
  - Requires 3.3.1.
  - Refresh before expiry with a five-minute buffer and retry transient
    failures without widening token exposure.
  - Success: a long-running session continues to authenticate after token
    refresh.

### 3.4. Authenticate Git through `GIT_ASKPASS`

This step answers whether cloned workspaces can use the refreshed token file
without leaking credentials into process arguments or shell history. The result
unblocks repository cloning. See podbot-design.md §§Execution flow, Token
management; mcp-server-hosting-design.md §7.4.

- [ ] 3.4.1. Mount the token file read-only and configure the Git askpass
  helper contract.
  - Requires 3.3.2 and 7.3.1.
  - Include the read-only bind mount, `GIT_ASKPASS` environment variable,
    helper-script documentation, and clone/fetch verification after refresh.
  - Success: Git operations authenticate through `/run/secrets/ghapp_token`
    and continue after token rotation without token values appearing in
    process arguments.

## 4. Agent launch, hosted sessions, and orchestration surfaces

Idea: if Podbot normalizes every launch into typed request and plan values,
then interactive sessions, hosted protocols, MCP wires, prompt artefacts,
hooks, recovery, and e2e orchestration can share one library-owned control
plane while the CLI remains a thin adapter.

This phase is the main vertical slice for embedder-facing hosting. It turns the
lower-level container and credential pieces into a coherent agent session
surface.

### 4.1. Configure Git identity inside the sandbox

This step proves that Podbot can apply host Git identity as a convenience
without blocking execution when identity is missing. The result informs
repository workflows and interactive sessions. See podbot-design.md §Execution
flow; developers-guide.md §17.

- [x] 4.1.1. Read host Git identity and configure it inside the container.
  - Requires 2.4.1.
  - Include `user.name`, `user.email`, in-container `git config --global`
    calls, and warning-only behaviour for missing identity.
  - See docs/execplans/4-1-1-git-identity-configuration.md.
  - Success: commits inside the container use configured identity when present
    and missing identity does not block execution.

### 4.2. Prepare repository workspaces

This step answers whether Podbot can materialize the target working tree for
both clone-backed and mount-backed launches. The result informs interactive
startup, app-server startup, and MCP helper-container sharing. See
podbot-design.md §§Execution flow, Host-mount path safety policy.

- [x] 4.2.1. Implement authenticated repository cloning for
  `workspace.source = "github_clone"`.
  - Requires 4.1.1.
  - Accept `owner/name`, require an explicit branch, clone to
    `workspace.base_dir`, and authenticate through `GIT_ASKPASS`.
  - See docs/execplans/4-2-1-repository-cloning.md.
  - Success: the specified branch is checked out without exposing credentials
    in process arguments.
- [ ] 4.2.2. Implement safe host-mounted workspaces.
  - Requires 1.4.1 and 2.2.5.
  - Canonicalize host paths, reject symlink escapes, enforce allowlisted mount
    roots, validate rootless-engine write permissions, and document the
    threat-model boundary.
  - Success: operators can choose host-mounted workspaces only within
    configured boundaries, with negative coverage for forbidden paths.

### 4.3. Start interactive and hosted agents through distinct modes

This step proves that launch mode determines stream policy and command shape
without duplicating orchestration. The result informs launch normalization and
CLI command boundaries. See podbot-design.md §Execution flow.

- [ ] 4.3.1. Launch interactive agents in permissive terminal-attached mode.
  - Requires 2.4.1, 2.3.1, 4.4.2, 10.2.2, and 4.2.1 or 4.2.2.
  - Use the normalized launch plan and neutral runtime boundary; this task
    preserves the container backend and does not require Kubernetes parity.
  - Start Claude Code and Codex with their documented permissive flags, attach
    the terminal, and clean up on agent exit.
  - Success: `agent.mode = "podbot"` starts an interactive session and does
    not accept hosting-only modes.
- [ ] 4.3.2. Launch app-server and ACP sessions through protocol-safe hosting.
  - Requires 2.5.3, 2.6.3, 4.4.2, 10.2.3, and 4.2.1 or 4.2.2.
  - Route through the shared runtime and transport contracts so Kubernetes
    reuses the same hosted-session policy rather than a second implementation.
  - Support Codex App Server over `stdio://`, generic ACP command execution,
    library/proxy launch behaviour, legal `(agent.kind, agent.mode)`
    validation, and clean shutdown on stdin close or termination signals.
  - Success: hosted app servers run through non-TTY proxying and preserve
    stdout purity from setup through shutdown.

### 4.4. Normalize launch requests and launch plans

This step answers whether all session modes can be described before execution
using typed, inspectable library values. The result informs public API
stabilization and artefact staging. See ADR 001; ADR 007.

- [ ] 4.4.1. Define the library-level `LaunchRequest` model.
  - Requires 1.4.1, 5.2.1, and 10.2.1.
  - Define the model before concrete launch implementation; include required
    runtime capabilities without exposing provider-specific API types.
  - Include agent kind, mode, workspace source, credential policy, prompt
    references, bundle references, skill selection, hook subscriptions, and
    MCP wire definitions.
  - Success: CLI and embedder paths can submit equivalent launch intent
    without sharing CLI parse types.
- [ ] 4.4.2. Define `LaunchPlan` and route `run` and `host` through one
  normalization path.
  - Requires 4.4.1.
  - Resolve command, args, environment policy, mount policy, stream policy,
    artefact staging targets, and wire injection details.
  - Success: normalization outcomes match across command entry points and
    invalid combinations fail before container mutation.

### 4.5. Expose hosted-session control without protocol contamination

This step proves that embedders can observe and control hosted sessions through
typed events while protocol bytes remain separate. The result informs hooks,
recovery, and CLI stderr rendering. See ADR 002.

- [ ] 4.5.1. Introduce the `HostedSession` handle.
  - Requires 4.4.2.
  - Separate protocol IO from a typed event stream and expose typed stop and
    hook-acknowledgement control methods.
  - Success: embedding tests can drive the hosted-session surface directly
    without shelling out to the CLI.
- [ ] 4.5.2. Render hosted-session events through adapters.
  - Requires 4.5.1.
  - Emit lifecycle, diagnostic, MCP wire, and hook-request events from library
    code, and render CLI diagnostics to stderr only.
  - Success: `podbot host` stays protocol-clean while using the same library
    session surface as embedders.

### 4.6. Provision MCP wires per workspace

This step answers whether Podbot can turn orchestrator-selected MCP sources
into agent-facing endpoints without putting tool catalogue policy in Podbot.
The result informs Corbusier integration and hosted validation. See
mcp-server-hosting-design.md §§4-8; ADR 001.

- [ ] 4.6.1. Add typed MCP wire request and response models.
  - Requires 4.4.1.
  - Include `McpSource::Stdio`, `McpSource::StdioContainer`,
    `McpSource::StreamableHttp`, `RepoAccess`, `CreateMcpWireRequest`, and
    `CreateMcpWireResponse`.
  - Success: the public wire contract exposes source intent and returned URL
    plus header details without exposing lifecycle internals.
- [ ] 4.6.2. Implement per-workspace MCP wire lifecycle operations.
  - Requires 4.6.1 and 2.5.3.
  - Support creating, listing, and removing operations, isolate per-workspace
    state from registry metadata, and inject only agent-facing reachability
    data.
  - Success: agents receive enough endpoint data to connect while Podbot owns
    bridge lifecycle and cleanup.
- [ ] 4.6.3. Enforce helper-container `RepoAccess` boundaries.
  - Requires 4.6.2 and 4.2.2.
  - Keep `RepoAccess::None` as the default and ensure helper-container access
    never changes the agent container's own workspace mount policy.
  - Success: stdio helper containers can request explicit repository access
    without implicit cross-container data exposure.

### 4.7. Stage prompts, skills, and bundles before launch

This step proves that hosted sessions can materialize prompt-driven artefacts
deterministically before an agent starts. The result informs validation, hooks,
and recovery. See ADR 004; ADR 005; ADR 007.

- [ ] 4.7.1. Define prompt frontmatter, template rendering, and bundle manifest
  contracts.
  - Requires 4.4.1.
  - Preserve standard skill-folder discovery while namespacing Podbot
    additions for prompts, skills, MCP references, and hook artefacts.
  - Success: artefacts have a documented ingestion contract and render with
    strict template semantics.
- [ ] 4.7.2. Materialize launch artefacts in deterministic order.
  - Requires 4.7.1 and 4.4.2.
  - Stage skills, render prompts, provision requested wires, register hook
    subscriptions, mount staged artefacts read-only, and clean them up on
    teardown.
  - Success: identical launch inputs produce identical staged artefacts and no
    stale session artefacts remain after normal teardown.

### 4.8. Validate prompt and capability disposition before launch

This step answers whether operators and orchestrators can inspect prompt, wire,
hook, and capability mismatches without mutating runtime state. The result
informs CI usage and ACP masking diagnostics. See ADR 006; ADR 008;
docs/corbusier-conformance-design-for-agents-mcp-wires-and-hooks.md §§555-778.

- [ ] 4.8.1. Implement the side-effect-free `validate_prompt` library surface.
  - Requires 2.6.3, 4.6.1, and 4.7.1.
  - Return typed diagnostics, canonicalized frontmatter where possible, and
    capability dispositions for native, host-enforced, translated, ignored,
    and denied capabilities.
  - Success: validation reports missing inputs, missing wires, missing hooks,
    and ACP-masked capabilities without receiving secrets or creating
    containers.
- [ ] 4.8.2. Add structured CLI validation output.
  - Requires 4.8.1.
  - Add `podbot validate-prompt` only if it can emit stable JSON for operator
    and Continuous Integration (CI) use without coupling validation to CLI
    parse types.
  - Success: CI can consume deterministic validation results and non-JSON
    diagnostics remain off stdout in machine mode.

### 4.9. Gate hosted hooks through suspend and acknowledge

This step proves that hosted sessions can pause for orchestrator governance
without giving hooks uncontrolled workspace or secret access. The result
informs recovery and Corbusier conformance. See ADR 003; ADR 008.

- [ ] 4.9.1. Define hook artefact and subscription models.
  - Requires 4.5.1 and 4.7.1.
  - Include inline scripts, container images, trigger subscriptions, explicit
    workspace access, and hook-specific environment allowlists.
  - Success: hook intent is reviewable before launch and separate from MCP
    helper-container access.
- [ ] 4.9.2. Implement suspend-and-acknowledge session flow.
  - Requires 4.9.1 and 4.5.2.
  - Emit hook request events, suspend progress until acknowledgement, and
    enforce deterministic allow, deny, timeout, and abort semantics.
  - Success: the orchestrator controls whether a hook runs and the session
    resumes or aborts predictably.
- [ ] 4.9.3. Capture hook outputs as structured events.
  - Requires 4.9.2.
  - Capture hook stdout and stderr separately, redact configured sensitive
    values, and avoid protocol stream contamination.
  - Success: hook completion is auditable through control events without
    writing hook output to hosted protocol stdout.

### 4.10. Recover and replay hosted control-plane state

This step answers whether hook-gated and wire-backed hosted sessions can be
recovered or abandoned deterministically after process restart. The result
informs e2e orchestration and operator trust. See ADR 009.

- [ ] 4.10.1. Add monotonic event envelopes to hosted session events.
  - Requires 4.5.2.
  - Include event identifiers, session identifiers, timestamps, ordering
    guarantees, and duplicate-detection semantics.
  - Success: orchestrators can detect gaps and duplicate events.
- [ ] 4.10.2. Persist minimal recovery state for hooks and wires.
  - Requires 4.10.1, 4.6.2, 4.7.2, and 4.9.2.
  - Persist enough state to resume or abandon pending hooks, clean stale wires,
    and avoid duplicate hook execution after restart.
  - Success: restart during a pending acknowledgement resolves
    deterministically without violating stdout purity or hook idempotency.

### 4.11. Prove orchestration with gated e2e scenarios

This step validates the phase idea against real runtime boundaries rather than
unit-level contracts alone. The result informs release readiness and CI
operability. See podbot-design.md §§Execution flow, Security model;
mcp-server-hosting-design.md §9; ADR 009.

- [ ] 4.11.1. Create the on-demand e2e harness and preflight contract.
  - Requires 4.10.2 and 7.4.1.
  - Add a distinct e2e suite, `make test-e2e`, machine-parseable JSON Lines
    preflight output, engine/socket readiness checks, sandbox image checks,
    binary availability checks, nested-container prerequisite checks, and
    assistive remediation messages.
  - Success: e2e runs fail before scenarios start when prerequisites are
    missing, and default `make test` scope is unchanged.
- [ ] 4.11.2. Add sandbox, nested-container, and Codex mock-provider
  scenarios.
  - Requires 4.11.1.
  - Cover a mock agent shell script, inner Podman startup, and Codex configured
    for an OpenAI-compatible mock inference provider implemented with Vidai
    Mock.
  - Success: the basic runtime path is validated against real container
    boundaries.
- [ ] 4.11.3. Add MCP wire and hook recovery scenarios.
  - Requires 4.11.1, 4.6.3, and 4.10.2.
  - Cover host-mounted workspaces with multiple MCP wires and restart during a
    hosted hook acknowledgement.
  - Success: injected wire metadata is sufficient for agent startup,
    duplicate hook acknowledgements are idempotent, and recovery never blindly
    replays a hook whose execution outcome is unknown.
- [ ] 4.11.4. Wire e2e execution into explicit CI triggers and artefacts.
  - Requires 4.11.1-4.11.3.
  - Run e2e only on manual dispatch or explicit workflow call, enforce
    run-scoped isolation with unique `run_id` names and labels, and persist
    logs and diagnostics as CI artefacts.
  - Success: local and CI e2e runs are reproducible, isolated, and diagnosable
    without slowing the default test suite.

## 5. Stable library API and embedding support

Idea: if the already-built CLI behaviour is extracted behind a curated,
semantic, versioned library boundary, Podbot can be embedded by host tools
without those tools depending on binary entrypoints, Clap parse types, or
process-exit side effects.

This phase preserves the existing public-library work while making its
relationship to later hosted surfaces explicit. Completed tasks stabilize the
current API; future hosted surfaces graduate only after their contracts are
implemented and documented.

### 5.1. Extract command orchestration into library modules

This step proves that command behaviour can be invoked without shelling out to
the binary. The result informs every embedder-facing API. See podbot-design.md
§Dual delivery model; ADR 001.

- [x] 5.1.1. Introduce public orchestration APIs for command flows.
  - Requires 2.5.3.
  - Include `run`, `exec`, `stop`, `ps`, and token-daemon operations, typed
    outcomes, and CLI-only process exits kept in the adapter.
  - See docs/execplans/5-1-1-public-orchestration-module.md.
  - Success: command flows are callable through library APIs, and the binary is
    a thin adapter.

### 5.2. Decouple configuration APIs from Clap

This step answers whether embedders can resolve configuration without
constructing CLI parse types. The result informs public API stability and
feature gating. See podbot-design.md §Configuration.

- [x] 5.2.1. Add a library-facing configuration loader.
  - Requires 1.3.6.
  - Accept explicit load options and overrides, keep Clap-dependent structures
    in the CLI adapter, and provide conversion helpers from parsed flags.
  - See docs/execplans/5-2-1-library-facing-configuration-loader.md.
  - Success: library consumers can resolve `AppConfig` without using
    `clap::Parser` or `Cli` structs.

### 5.3. Stabilize the supported public boundary

This step proves that external callers can rely on a small documented surface
while implementation internals remain private. The result is the baseline for
future hosted APIs. See ADR 001; developers-guide.md §11.

- [x] 5.3.1. Document and enforce stable public modules, request and response
  types, semantic errors, and CLI feature boundaries.
  - Requires 5.1.1 and 5.2.1.
  - Reconcile public hook and validation schema direction with the documented
    integration contract before stabilizing any new hosted surface.
  - See docs/execplans/5-3-1-stabilize-public-library-boundaries.md.
  - Success: Podbot can be integrated as a Rust dependency with documented,
    versioned APIs and no CLI coupling requirement.

## 6. Operator CLI

Idea: if the CLI is a disciplined adapter over library orchestration, terminal
operators get clear commands and exit codes while hosted protocol and embedder
surfaces keep their stricter stream and type contracts.

This phase completes the user-facing command surface. It is sequenced after
library extraction so CLI work does not become the source of orchestration
truth.

### 6.1. Route subcommands through library APIs

This step answers whether all operator commands can parse arguments, validate
required inputs, and delegate to library functions. The result informs command
specific work. See podbot-design.md §Dual delivery model.

- [ ] 6.1.1. Implement subcommand dispatch for `run`, `host`,
  `token-daemon`, `ps`, `stop`, and `exec`.
  - Requires 5.3.1.
  - Implemented scope:
    - [x] `run` dispatch is implemented through the library-owned request
      boundary in docs/execplans/6-1-1-run-subcommand.md.
    - [ ] `host` dispatch remains pending.
    - [ ] `token-daemon` dispatch remains pending.
    - [ ] `ps` dispatch remains pending.
    - [ ] `stop` dispatch remains pending.
    - [ ] `exec` dispatch remains pending.
  - Success: help text describes each command, invalid arguments produce clear
    errors, and dispatch does not duplicate library orchestration.

### 6.2. Launch interactive sessions through `podbot run`

This step proves that terminal operators can start interactive agent sessions
without accidentally selecting protocol-hosting modes. The result informs
operator documentation. See podbot-design.md §Execution flow.

- [ ] 6.2.1. Implement the interactive `run` command.
  - Requires 4.3.1 and 6.1.1.
  - Accept required `--repo owner/name`, required `--branch`, `--agent` values
    `codex`, `claude`, or `custom`, and only `--agent-mode podbot`.
  - Success: hosting modes are rejected with guidance to use `podbot host`,
    and successful runs return the agent exit code.

### 6.3. Manage containers through operator commands

This step answers whether operators can inspect and manage running Podbot
containers without using raw container-engine commands. The result informs
supportability. See users-guide.md §Command-line interface (CLI).

- [ ] 6.3.1. Implement `ps`, `stop`, and `exec` management commands.
  - Requires 6.1.1 and 5.1.1.
  - List active Podbot containers, terminate containers by ID or name, execute
    arbitrary commands in containers, and format human-readable output.
  - Success: management commands operate against running containers with
    clear, consistent output.

### 6.4. Run the token daemon as an operator command

This step proves that token refresh can be supervised independently of an agent
session. The result informs systemd integration and long-lived clone flows. See
podbot-design.md §Token management.

- [ ] 6.4.1. Implement the `token-daemon` subcommand.
  - Requires 3.3.2 and 6.1.1.
  - Accept container ID, support user systemd execution, handle `SIGTERM`
    gracefully, and log refresh events without token values.
  - Success: the daemon can run independently and shut down cleanly.

### 6.5. Host protocol sessions through `podbot host`

This step answers whether the CLI can expose protocol hosting without sharing
interactive output concerns. The result informs protocol conformance. See
podbot-design.md §Execution flow; developers-guide.md §§4-6.

- [ ] 6.5.1. Implement the dedicated protocol-only `host` command.
  - Requires 4.3.2 and 6.1.1.
  - Accept hosting modes `codex_app_server` and `acp`, route to non-TTY proxy
    orchestration, write lifecycle diagnostics to stderr, handle disconnects
    and signals cleanly, and return explicit non-zero setup failures.
  - Success: `podbot host` behaves as a protocol-clean transport adapter.

### 6.6. Expose MCP wire operations for operators and orchestrators

This step proves that the public MCP wire lifecycle is reachable through JSON
CLI output without making Corbusier scrape human-oriented text. The result
informs Corbusier integration. See mcp-server-hosting-design.md §8.1.

- [ ] 6.6.1. Add `podbot wire mcp add`, `remove`, and `list`.
  - Requires 4.6.2 and 6.1.1.
  - Support structured JSON output, stdio helper-container source definitions,
    explicit repo-access settings, and semantic setup failures.
  - Success: orchestrators can create and remove wires through the CLI when
    they cannot link the Rust library directly.

## 7. Runtime container image

Idea: if the sandbox image is built from explicit, versioned runtime choices,
Podbot can run nested containers, Git operations, interactive agents, and
hosted app servers reproducibly rather than depending on mutable operator
machines.

This phase creates the runtime substrate consumed by orchestration and e2e
tests. It is deliberately separate from the engine wrapper because image
contents have their own versioning and upgrade contract.

### 7.1. Build the base sandbox image

This step answers whether the image can run inner Podman and Git inside the
sandbox. The result informs agent runtime installation and e2e preflight. See
podbot-design.md §Execution flow.

- [ ] 7.1.1. Define the base Containerfile for the sandbox environment.
  - Requires 2.2.5.
  - Select a Podman-capable base image, install `podman`, `fuse-overlayfs`,
    `slirp4netns`, `git`, and required utilities, configure user namespace
    support, and set the default user and working directory.
  - Success: the image builds, inner Podman runs, and Git commands execute
    inside the container.

### 7.2. Install agent runtimes and hosted tooling

This step proves that the image contains the runtimes needed for interactive
and hosted agents. The result informs launch commands and versioning policy.
See podbot-design.md §Execution flow.

- [ ] 7.2.1. Add Claude Code, Codex, ACP tooling runtimes, and documented
  version checks.
  - Requires 7.1.1.
  - Include Claude Code, Codex CLI, Node.js for OpenCode and Droid ACP tooling,
    Python 3.10+ for Claude Agent SDK wrappers, OpenCode installation, Goose
    invocation documentation, command verification, and upgrade notes.
  - Success: each documented runtime and agent command executes inside the
    container with repeatable upgrade guidance.

### 7.3. Install the Git askpass helper

This step answers whether the image can consume the rotated token file through
a narrow helper contract. The result unblocks clone-backed workspaces. See
podbot-design.md §Token management; mcp-server-hosting-design.md §7.4.

- [ ] 7.3.1. Install the `GIT_ASKPASS` helper in the sandbox image.
  - Requires 7.1.1.
  - Read `/run/secrets/ghapp_token`, install the helper at a known executable
    path, set permissions, and configure the default `GIT_ASKPASS`.
  - Success: Git operations authenticate through the mounted token file.

### 7.4. Automate image build, verification, and distribution

This step proves that image updates are reviewable and reproducible. The result
informs e2e scenarios and operator upgrades. See mcp-server-hosting-design.md
§8.5.

- [ ] 7.4.1. Add local and CI image build automation.
  - Requires 7.1.1-7.3.1.
  - Add a Makefile target, CI build workflow, registry push, image
    verification, version tags, and digest-pinning documentation.
  - Success: images build automatically, publish with documented tags, and can
    be verified before use in e2e runs.

## 8. Protocol and orchestrator conformance

Idea: if Codex App Server, ACP, MCP wires, prompt validation, and hook
acknowledgement all remain correct under realistic protocol traffic, then
Podbot's hosting contract is stable enough for external orchestrators such as
Corbusier to depend on.

This phase contains integration and conformance tasks because protocol framing,
stdout purity, and orchestrator contracts are product surfaces in their own
right. These tasks complement implementation tasks; they are not isolated
unit-test chores.

### 8.1. Prove Codex App Server hosting

This step answers whether a real app-server client flow survives the Podbot
host proxy. The result informs `podbot host` release readiness. See
podbot-design.md §Execution flow.

- [ ] 8.1.1. Add Codex App Server conformance coverage.
  - Requires 6.5.1 and 7.2.1.
  - Launch `podbot host` with `codex app-server --listen stdio://`, drive
    initialize, new-thread, and prompt traffic through a test client, and
    assert no Podbot diagnostics appear on stdout.
  - Success: the Codex client flow succeeds against Podbot hosting without
    protocol contamination.

### 8.2. Prove ACP transport and capability masking

This step answers whether ACP newline framing and sandbox-preserving masking
survive real hosted traffic. The result informs ACP support policy. See
podbot-design.md §Execution flow; ADR 006.

- [ ] 8.2.1. Add ACP transport conformance coverage.
  - Requires 2.6.3 and 6.5.1.
  - Exchange newline-delimited JSON-RPC through Podbot, assert exact framing,
    assert stdout purity, verify initialization masking, and verify runtime
    denial for blocked ACP methods.
  - Success: ACP sessions remain protocol-correct with default
    sandbox-preserving enforcement.

### 8.3. Prove lifecycle stream purity under edge conditions

This step validates the most failure-prone protocol-hosting edges: startup,
partial frames, backpressure, client disconnect, and signal termination. The
result informs supportability and regression risk. See developers-guide.md
§§4-6.

- [ ] 8.3.1. Add host lifecycle and output-purity conformance coverage.
  - Requires 6.5.1.
  - Cover zero stdout before the first proxied protocol byte, zero stdout
    after shutdown, stderr-only diagnostics under signals, partial-frame
    handling, and backpressure scenarios.
  - Success: hosted protocols remain stream-clean across startup,
    steady-state, and shutdown.

### 8.4. Prove orchestrator-facing integration contracts

This step answers whether Corbusier can rely on Podbot's wire, validation, and
hook semantics as documented. The result informs cross-project adoption. See
docs/corbusier-conformance-design-for-agents-mcp-wires-and-hooks.md §§778-867.

- [ ] 8.4.1. Add wire, hook, and validation conformance coverage.
  - Requires 4.6.3, 4.8.2, 4.9.3, and 4.10.2.
  - Cover multiple MCP wires in a host-mounted workspace, deterministic
    `validate_prompt` diagnostics for masked capabilities and missing
    artefacts, hook allow/deny/timeout/restart outcomes, and explicit
    helper-container `RepoAccess` behaviour.
  - Success: the orchestrator-facing contract is validated end to end against
    documented behaviour.

## 9. Deferred extensions

Idea: if the core Podbot promise is already trustworthy and boring to operate,
the project can evaluate broader isolation and session-management extensions on
their product value instead of letting them destabilize the main release.

These items are intentionally deferred from the core roadmap. They should not
block the phases above unless a later design document explicitly promotes one
of them into required v1 scope.

These deferrals do not include phase 10's Kubernetes security baseline or
controller-restart recovery. Task 9.1.1 remains a broader container-backend
policy evaluation; task 9.2.2 concerns agent persistence across container
restarts, not reconnection to a still-running agent after controller restart.

### 9.1. Reassess stronger isolation after the container path is stable

This step captures isolation enhancements that are valuable but not necessary
to prove the current container-hosted design. See podbot-design.md §Security
model.

- [ ] 9.1.1. Evaluate network egress restriction for model endpoints and
  GitHub.
  - Requires: 8.1.1, 8.2.1, 8.3.1, and 8.4.1.
  - Success: the project has a documented decision on whether egress
    restriction belongs in a follow-up release.
- [ ] 9.1.2. Evaluate virtual machine isolation for higher-assurance
  environments.
  - Requires: 8.1.1, 8.2.1, 8.3.1, and 8.4.1.
  - Success: the project has a documented trade-off analysis comparing virtual
    machines with the existing container boundary.

### 9.2. Reassess broader workspace and session models

This step captures product extensions that depend on a reliable hosted-session
foundation. See podbot-design.md §§Execution flow, Security model.

- [ ] 9.2.1. Evaluate multi-repository workspace support.
  - Requires: 8.4.1.
  - Success: the project has a documented design for whether and how multiple
    repositories share credentials, mounts, and MCP wire policy.
- [ ] 9.2.2. Evaluate persistent agent sessions across container restarts.
  - Requires: 8.3.1.
  - Success: the project has a documented persistence model or a clear
    decision to keep sessions ephemeral.

## 10. Kubernetes execution backend

Goal: run Corbusier's agent sandboxes as Kubernetes-managed workloads through
Podbot's existing library boundary, while preserving standalone Docker/Podman
operation and honest lifecycle guarantees.

Idea: if sandbox intent, protocol policy, and recovery semantics are independent
of the engine API, a second provider can realize the same agent contract without
moving runtime ownership back into Corbusier or weakening sandbox isolation.

This phase implements the proposal in
[RFC 0001](rfcs/0001-kubernetes-execution-backend.md). It is not deferred behind
phase 9: numbering preserves published task references, while dependencies
determine delivery order. Tasks 10.1 and 10.2 precede unfinished launch work.
K0 proves non-resumable execution, K1 adds integrated hosted capabilities, and
K2 proves restart-safe control ownership. All new tasks remain incomplete until
their stated evidence exists; accepting the RFC does not complete them.

### 10.1. Ratify the backend contract and compatibility policy

This step tests whether Kubernetes can fit the existing ownership and public
API boundaries without a second hosted runtime. Its output constrains all new
provider work. See RFC 0001 §§2-4, 10, and 12.

- [ ] 10.1.1. Ratify the backend RFC and record the architecture decision.
  - Requires 5.3.1.
  - Review the capability matrix, public/internal boundary, K0/K1/K2 gates,
    and Corbusier ownership split. Record an ADR and reconcile the primary
    design and ADRs 001, 002, 007, 008, and 009 without implying that proposed
    contracts are already implemented.
  - Success: one reviewed contract identifies accepted capabilities, explicit
    exclusions, and the changes required to local-file recovery assumptions.
- [ ] 10.1.2. Add explicit runtime selection and optional provider features.
  - Requires 10.1.1 and 5.2.1.
  - Preserve container defaults and legacy engine configuration; introduce
    validated Kubernetes connection, namespace, execution-profile, and
    capability settings. Select compatible `kube`/`k8s-openapi` versions and
    document the supported Kubernetes API matrix.
  - Success: precedence and negative tests reject absent features, unsupported
    combinations, and implicit backend fallback before any resource mutation.

### 10.2. Extract backend-neutral execution without regressions

This step tests the proposed abstraction against the existing implementation
before a second provider depends on it. It unblocks backend-neutral launch
planning and both execution adapters. See RFC 0001 §§4.1-4.3 and 7.1.

- [ ] 10.2.1. Define runtime identities, capabilities, and narrow internal
  ports.
  - Requires 10.1.2.
  - Separate logical session/workspace identity from physical runtime identity;
    define versioned handles, execution generations, a minimal intent/cleanup
    store, lifecycle states, semantic errors, and capability requirements.
    Keep provider types out
    of public requests and preserve the existing stable exec API.
  - Success: serialization, compatibility, and contract tests prove that the
    new boundary can represent both providers without Docker-shaped fields.
- [ ] 10.2.2. Adapt existing Bollard execution to the neutral runtime boundary.
  - Requires 10.2.1 and 2.5.3.
  - Preserve socket resolution, security profiles, credential upload, terminal
    behaviour, and semantic outcomes. Keep Bollard-specific test seams inside
    the adapter rather than duplicating orchestration above it.
  - Success: existing container tests and new shared contract tests pass with
    unchanged default behaviour and no Kubernetes dependency requirement.
- [ ] 10.2.3. Extract shared protocol policy from Bollard stream handling.
  - Requires 10.2.2 and 2.6.2.
  - Reuse ACP masking, method denial, bounded byte proxying, cancellation, and
    stdout purity through transport-neutral streams owned by the caller's
    asynchronous runtime.
  - Success: both fake transports pass identical framing, backpressure,
    capability-denial, and zero-extra-stdout tests.

### 10.3. Implement restricted Kubernetes lifecycle and observation

This step tests whether Kubernetes can realize and clean up an owned sandbox
with bounded control-plane work. It unblocks real execution and cluster
preflight. See RFC 0001 §§5 and 8.2-8.3.

- [ ] 10.3.1. Add the namespaced Kubernetes client and authorization preflight.
  - Requires 10.2.1.
  - Support explicit kubeconfig and in-cluster credentials; classify connection,
    authentication, authorization, throttling, and unsupported API failures.
    Restrict contexts and namespaces to trusted operator configuration.
  - Success: mock API tests distinguish each failure and never select another
    backend or cluster after a failed request.
- [ ] 10.3.2. Build typed restricted Pod plans with idempotent creation.
  - Requires 10.3.1 and 4.4.2.
  - Generate session/generation names, `restartPolicy: Never`, explicit
    containers, resource budgets, security contexts, and ownership metadata.
    Use a local single-controller intent ledger for K0 and reconcile lost create
    responses or partial creation; this ledger does not provide K2 recovery.
  - Success: snapshots and property tests reject unsafe specifications and
    conflicting idempotency keys without creating duplicate sandboxes.
- [ ] 10.3.3. Observe, stop, and clean up owned resources safely.
  - Requires 10.3.2.
  - Add bounded namespace watches, relisting, retry budgets, pending/failure
    diagnostics, UID-aware discovery, UID-preconditioned deletion, a cleanup
    ledger, and expiry sweeping. Preserve operator-owned storage.
  - Success: injected watch gaps, stale identities, API outages, and partial
    deletion leave explicit outcomes rather than false success or busy loops.
- [ ] 10.3.4. Provide platform policy examples and enforcement preflight.
  - Requires 10.3.2.
  - Document separate controller/agent identities, namespaced RBAC, admission
    restrictions, execution namespaces, quotas, and enforced network policy.
    Verify the chosen client's stream-subresource verbs and negative access
    tests; exclude cluster-wide bootstrap permissions from runtime roles.
  - Success: the test cluster rejects forbidden service accounts, host access,
    cross-tenant traffic, and unauthorized credential references.

### 10.4. Materialize workspaces and scoped credentials

This step tests whether repository and credential contracts survive remote
scheduling without exposing host state. It supplies K0 workspace preparation
and K1 credential support. See RFC 0001 §6.

- [ ] 10.4.1. Build a restricted test image and clone-backed ephemeral
  workspace.
  - Requires 10.3.3 and 4.2.1.
  - Use an initialization container and `emptyDir`, explicit Git identity,
    bounded startup, and a pinned mock agent. Reject `host_mount` rather than
    translating it to `hostPath`; do not require nested Podman from phase 7.
  - Success: a public fixture repository reaches the requested revision in a
    restricted Pod, and failed initialization leaves a classified outcome.
- [ ] 10.4.2. Add Secret-backed credentials and a rotating delivery sink.
  - Requires 10.4.1, 3.2.1, and 3.3.2.
  - Reuse token issuance and refresh scheduling with a Kubernetes publication
    sink. Project scoped credentials only where needed, avoid `subPath`, handle
    propagation delay/expiry, and redact failures. Define writable credential
    scratch separately from read-only projections.
  - Success: rotation tests preserve Git authentication without exposing App
    signing keys or cluster credentials, and expired credentials fail closed.
- [ ] 10.4.3. Add explicit retained-workspace and cache policies.
  - Requires 10.4.1.
  - Separate source from PVC allocation/retention; authorize existing claims,
    record access/topology/size constraints, initialize idempotently, and keep
    build-cache scope separate from mutable repository storage.
  - Success: retained work survives Pod replacement without claiming process
    recovery, and cleanup never deletes supplied claims or another tenant's
    cache/workspace.

### 10.5. Prove non-resumable protocol execution through both surfaces

This step tests the smallest useful Kubernetes vertical slice before durable
hosting adds complexity. It establishes K0, not reconnect guarantees. See
RFC 0001 §§3.3 and 7.1.

- [ ] 10.5.1. Implement Kubernetes protocol attachment and process outcomes.
  - Requires 10.2.3, 10.3.3, and 4.5.1.
  - Adapt non-TTY streams to the shared proxy; read stdout/stderr concurrently,
    decode remote status, bound buffers, and distinguish launch via exec from
    attachment to an existing process. Never retry an uncertain exec launch.
  - Success: mock transport tests cover partial frames, missing exit status,
    cancellation, disconnect, and explicit remote termination verification.
- [ ] 10.5.2. Route library and CLI operations through runtime selection.
  - Requires 10.5.1, 4.3.2, 6.3.1, and 6.5.1.
  - Reuse the existing `HostedSession`, `host`, `ps`, `stop`, and `exec`
    surfaces with runtime-aware identities and an advertised non-resumable
    capability. Preserve legacy container command behaviour.
  - Success: a library caller and CLI caller can launch, observe, execute in,
    and stop the same Kubernetes sandbox without parsing human-readable text.
- [ ] 10.5.3. Pass the K0 real-cluster acceptance gate.
  - Requires 10.5.2, 10.4.1, and 10.3.4.
  - Add an explicitly invoked k3d/kind suite with a mock ACP agent, run-scoped
    resources, strict preflight, failure injection, and bounded cleanup.
  - Success: exact protocol exchange, capability masking, non-zero outcomes,
    interruption reporting, and resource cleanup pass without paid inference
    or private credentials; default unit tests remain cluster-free.

### 10.6. Integrate hosted MCP, artefacts, and hook governance

This step tests whether Kubernetes realizes the existing hosted contract rather
than inventing a parallel tool plane. It establishes K1. See RFC 0001 §§6-8.

- [ ] 10.6.1. Provision predeclared Kubernetes MCP helpers and wire endpoints.
  - Requires 10.5.3, 10.4.2, and 4.6.3.
  - Map stdio sources to an explicit sandbox/helper locus, use authorized
    Pod-local or Service endpoints, and enforce per-helper credentials and
    `RepoAccess`. Reject late container additions and host-gateway assumptions.
  - Success: multiple wires work with default no-repository access and explicit
    read-only/read-write grants, without arbitrary controller-side processes.
- [ ] 10.6.2. Stage artefacts and execute supported hooks through shared
  control.
  - Requires 10.6.1, 4.7.2, 4.8.1, and 4.9.3.
  - Reuse prompt validation, read-only artefact staging, hook identities,
    suspend/acknowledge semantics, and separate output capture. Reject
    unsupported
    hook-image/storage combinations before resource mutation.
  - Success: allow, deny, timeout, and unsupported-capability scenarios preserve
    policy and stdout purity without silently changing execution images.
- [ ] 10.6.3. Pass the K1 hosted-contract acceptance gate.
  - Requires 10.6.2 and 8.4.1.
  - Run shared wire, validation, credential-rotation, hook, and cleanup
    scenarios against both providers with explicit capability expectations.
  - Success: Kubernetes satisfies its declared hosted profile and still reports
    reconnect as unsupported until the K2 gate passes.

### 10.7. Establish fenced ownership and restart-safe hosting

This step tests whether a controller restart can preserve supported sessions
without duplicate side effects or competing owners. It establishes K2, while
agent-process and node-loss recovery remain explicitly qualified. See RFC 0001
§§7.2 and 9; ADR 009.

- [ ] 10.7.1. Add shared runtime state and caller-supervised reconciliation.
  - Requires 10.6.3 and 4.10.2.
  - Extend the state-store port for shared intent, runtime identities,
    generations, event cursors, pending hooks, ownership, and cleanup. Support
    shared-store compare-and-set semantics without a Corbusier database
    dependency; distinguish local-file recovery from multi-replica recovery.
  - Success: restart rediscovery preserves the same UID or reports explicit
    interruption, and unknown outcomes never trigger automatic agent replay.
- [ ] 10.7.2. Ratify the bridge transport, authentication, and fencing protocol.
  - Requires 10.7.1.
  - Specify protocol/control separation, bounded replay, controller ownership
    generations, credential rotation, disconnect policy, and agent-specific
    continuity limits. Reconcile ADRs 002 and 009 before implementing the wire.
  - Success: the protocol prevents stale-controller mutations and explicitly
    distinguishes reconnecting transport from replaying agent operations.
- [ ] 10.7.3. Implement the session supervisor and fenced reconnection.
  - Requires 10.7.2.
  - Keep the agent process under a Pod-local supervisor without cluster or
    signing authority. Expose the same library handle, enforce current-owner
    generations, and preserve hook/event identity across supported reconnects.
  - Success: a replacement controller attaches to a supported live session,
    while stale owners and ambiguous pending operations receive typed refusals.
- [ ] 10.7.4. Pass the K2 failure and recovery acceptance gate.
  - Requires 10.7.3 and 10.4.3.
  - Inject controller death, partitions, competing owners, lease expiry, stale
    fencing, lost acknowledgements, Secret rotation, eviction, node loss, UID
    replacement, and partial cleanup. Measure buffers and API-server load.
  - Success: supported continuity works, unsupported recovery is explicit,
    uncertain actions are not replayed, and retained workspaces remain safe.

### 10.8. Publish conformance evidence and enable Corbusier adoption

This step tests whether the backend is supportable outside its own harness and
whether it restores the intended application-plus-agents deployment model.
See RFC 0001 §10.

- [ ] 10.8.1. Add provider feature-matrix CI and gated cluster evidence.
  - Requires 10.5.3 and 10.2.2.
  - Test default, container-only, Kubernetes-only library, and combined builds;
    check dependency isolation and run real-cluster scenarios only on explicit
    dispatch/workflow calls. Publish redacted diagnostics and pinned versions.
  - Success: container consumers do not compile Kubernetes unnecessarily and
    each supported provider has reproducible contract and cluster evidence.
- [ ] 10.8.2. Document operations, compatibility, and rollout/rollback gates.
  - Requires 10.8.1, 10.6.3, and 10.7.4.
  - Update user/developer guides with supported capabilities, platform setup,
    observability, limits, recovery, retention, and draining. Keep new admission
    independently disableable while existing sessions reconcile or terminate.
  - Success: release review can trace every advertised guarantee to a passing
    gate, and rollback does not strand resources or erase durable state.
- [ ] 10.8.3. Verify Corbusier's Kubernetes-native hosted-agent integration.
  - Requires 10.8.2.
  - Track a separate linked Corbusier change for runtime selection, durable
    state/ownership, deployment RBAC, and end-to-end adoption. Record exact
    Podbot/Corbusier revisions; do not mark external integration complete from
    a Podbot-only mock.
  - Success: the Kubernetes-deployed application launches and supervises agent
    Pods through the Podbot library, exercises recovery and cleanup, and never
    requires a node Docker/Podman socket.
