# Corbusier–Podbot Conformance Design for Agents, MCP Wires, and Hooks

## Executive summary

Corbusier can conform to the Podbot library API surface by treating Podbot as
the **single owner of container orchestration and sandbox wiring**, while
Corbusier retains **policy, registry, and orchestration authority**. This
matches Podbot’s stated dual-delivery model (CLI + embeddable library) and its
“stdout protocol-purity” guarantee for hosting mode.

The highest-impact architectural change is to **split Corbusier’s tool plane
into a registry/control plane and a per-workspace “wire” plane**. The Podbot
MCP hosting design explicitly recommends that Corbusier remains the
policy/registry layer while Podbot owns transport bridging, auth token
injection, and lifecycle clean-up, presenting **Streamable HTTP** to the
container regardless of upstream source.

Under the user’s hook assumptions, Corbusier must add a **HookCoordinator**
that consumes Podbot hook events over a *podbot → orchestrator* channel and
deterministically acknowledges them, because Podbot suspends execution until
Corbusier acks. (This hook channel is not described in current Podbot docs; it
is a new integration requirement and must be specified/implemented explicitly
as part of the integration contract.)

Finally, to support consistent agent behaviour across backends, Corbusier
should introduce a **prompt + skill bundle abstraction** modelled on
Anthropic’s “skills as folders with YAML-frontmatter SKILL.md”, with harmonized
frontmatter across prompts/bundles/skills and Jinja2 (Goose-style)
interpolation, plus a **proposed Podbot `validate_prompt` surface** that
reports ignored/rejected capabilities for a target agent runtime.

## Current state and required alignment

Corbusier is organized as hexagonal modules (domain/ports/adapters) with key
subsystems including `agent_backend` and `tool_registry`, and an explicit
`worker` module. The roadmap shows that *workspace encapsulation* and a *hook
engine* remain planned (not delivered), while the MCP server lifecycle and tool
routing portions are already implemented.

### Corbusier tool registry today

Corbusier already models an MCP server registry and lifecycle persistence:

- `mcp_servers` table stores a `transport` JSONB plus lifecycle and health
  state.
- Tenant scoping is being added via `tenant_id` on `mcp_servers` with composite
  foreign keys to dependent tables.
- `tool_registry/services` exports a `McpServerLifecycleService` and a
  `ToolDiscoveryRoutingService`.
- Tool-call parameter validation currently uses a lightweight structural
  checker (object type + required keys) rather than full JSON Schema.

Corbusier’s design document still frames tool hosting as “MCP server hosting
with stdio and HTTP+SSE managers” and positions a tool router in Corbusier’s
call path. Its initial lifecycle implementation provides an
`InMemoryMcpServerHost` adapter for deterministic tests, which currently
appears as the concrete “runtime host” adapter in the repo.

### Podbot contract constraints Corbusier must respect

Podbot’s design asserts three constraints that materially affect Corbusier
integration:

- Podbot is **both** CLI and embeddable Rust library; library functions return
  typed results and must not write directly to stdout/stderr.
- In hosting mode, Podbot must preserve **stdout purity**: container-protocol
  bytes only, with diagnostics on stderr.
- For ACP hosting, Podbot enforces **capability masking** by rewriting the ACP
  `initialize` exchange to remove `terminal/*` and `fs/*`, and may reject those
  calls if attempted. An explicit opt-in may allow delegation.

On workspace strategy, Podbot’s config explicitly supports
`workspace.source = github_clone | host_mount`, and defines hard safety
requirements for host mounts (canonicalization, allowlisted roots, symlink
escape rejection).

### MCP hosting alignment target

The Podbot MCP hosting design recommends:

- Corbusier chooses which MCP servers / wires a task may use (policy/registry),
- Podbot creates “wires”, performs bridging and clean-up, injects the resulting
  URL + auth material into the agent container,
- the agent consumes **Streamable HTTP** endpoints, even when the true source
  is stdio.

This creates a **direct mismatch** with Corbusier’s current “tool registry &
router in the call path” mental model: if the agent container talks to MCP
wires directly, Corbusier cannot remain the runtime “router” for those tool
calls unless Podbot additionally proxies through Corbusier (not described).
This is a pivotal open design choice and must be made explicit in the
integration design:

- **Option A (recommended by Podbot docs):** agent container is the MCP client;
  Corbusier moves from “tool router” to “tool policy + wire provisioning +
  audit ingestion”.
- **Option B (legacy Corbusier model):** Corbusier remains the caller; Podbot
  executes tools inside container and returns results to Corbusier. This would
  require a Podbot→Corbusier tool-call bridge API that is **unspecified** in
  current Podbot docs.

The rest of this document designs for **Option A**, because it matches the
primary Podbot MCP hosting design and avoids inventing an unstated Podbot
service. Where Option A implies new audit/telemetry pathways, those are called
out as required additions.

## Domain model changes

This section defines the concrete models Corbusier needs (or needs to evolve)
to align with Podbot’s design surface and the hook assumptions. All type
sketches are Rust-like and intentionally “library-facing” (serializable
request/response shapes, stable enums).

### AgentRuntimeSpec

Corbusier needs a runtime description that maps cleanly to Podbot’s
`agent.kind`, `agent.mode`, and optional custom command fields, plus an env
allowlist.

```rust
/// Corbusier-owned description of the agent runtime to launch via Podbot.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AgentRuntimeSpec {
    pub kind: AgentKind,          // claude | codex | custom
    pub mode: AgentMode,          // podbot(interactive) | codex_app_server | acp
    pub command: Option<String>,  // required when kind=custom (per Podbot design)
    pub args: Vec<String>,        // required when kind=custom
    pub env_allowlist: Vec<String>,
    pub working_dir: Option<String>, // container path; default derived from workspace
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum AgentKind { Claude, Codex, Custom }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum AgentMode { Interactive, CodexAppServer, Acp }
```

Conformance requirements:

- `AgentMode::Acp` must assume that `terminal/*` and `fs/*` are masked by
  Podbot and are not reliable capabilities.
- `env_allowlist` must be enforced in Corbusier configuration generation, not
  left as an “advisory” field (Podbot treats this as a contract boundary).

### WorkspaceSource

Corbusier needs to decide whether Podbot clones into a container-local volume
(`github_clone`) or bind-mounts a host workspace (`host_mount`). Podbot’s
existing config describes both modes and imposes safety policy for host mounts.

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum WorkspaceSource {
    GitHubClone {
        owner: String,
        repo: String,
        branch: String,
        // token material is not in this struct; it comes via Podbot GitHub config
    },
    HostMount {
        host_path: std::path::PathBuf,
        container_path: String, // absolute; default "/workspace"
        read_only: bool,        // strongly recommended default for prompts; tasks may opt-in
    },
}
```

**Unspecified detail:** Corbusier’s repo currently lacks a concrete “workspace
manager” implementation in code; the Corbusier design doc contains a conceptual
`EncapsulationProvider` but no corresponding crate module. Corbusier must
create this module and decide whether it provisions the host workspace
directory (recommended for determinism) or delegates cloning to Podbot (aligns
with Podbot’s existing `github_clone` flow).

### McpEndpointSource

Corbusier’s persistent transport model should be updated to match Podbot’s
recommended `McpSource` shape: `Stdio`, `StdioContainer`, `StreamableHttp`,
with explicit repo volume sharing for helper containers.

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum McpEndpointSource {
    Stdio {
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        cwd: Option<String>,
    },
    StdioContainer {
        image: String,
        command: Vec<String>,
        env: Vec<(String, String)>,
        repo_access: RepoAccess, // none/ro/rw (for helper container only)
    },
    StreamableHttp {
        url: String,
        headers: Vec<(String, String)>, // injected upstream auth if needed
    },
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum RepoAccess { None, ReadOnly, ReadWrite }
```

This aligns with Podbot’s MCP hosting design, where Podbot normalizes the
agent-facing transport to Streamable HTTP even when the source is stdio.

**Required Corbusier schema change:** Corbusier’s existing
`mcp_servers.transport` JSONB column is flexible, but the *meaning* of stored
transports must evolve: stop modelling “HTTP+SSE” as first-class, and align
persisted transport shapes to *source definitions*
(stdio/stdio-container/streamable-http), leaving “agent-facing URL” as a
per-workspace wire artefact rather than a global server attribute.

### HookArtifact and HookSubscription

Based on the user’s hook assumptions, Corbusier needs:

- a “hook artefact” model that Podbot can execute (single script OR tar OR
  optional container image),
- a subscription model that says which hooks should fire at which points for
  which workspaces,
- a runtime state model for acknowledgements.

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct HookArtifact {
    pub kind: HookArtifactKind,           // script | tar
    pub digest: Option<String>,           // strongly recommended; sha256:...
    pub container_image: Option<String>,  // optional override for execution environment
    pub entrypoint: Option<String>,       // optional; tar needs some entrypoint (unspecified)
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum HookArtifactKind {
    Script { path: String }, // workspace-relative or bundle-relative (policy decides)
    Tar { path: String },    // contains runnable content
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct HookSubscription {
    pub hook_name: String,
    pub triggers: Vec<HookTrigger>,
    pub workspace_access: WorkspaceAccessMode, // r/o or r/w mount policy for workspace volume
    pub env_allowlist: Vec<String>,
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum WorkspaceAccessMode { ReadOnly, ReadWrite }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum HookTrigger {
    // Concrete trigger taxonomy is currently unspecified in Podbot docs;
    // Corbusier design doc lists commit/merge/deploy style governance triggers conceptually.
    PreTurn,
    PostTurn,
    PreToolCall,
    PostToolCall,
    PreCommit,
    PreMerge,
    PreDeploy,
}
```

**Unspecified but required:** a concrete mapping between Corbusier’s workflow
events and hook triggers (including which component emits them) is not
implemented in Corbusier today, and Podbot has no hook spec in its current
design docs. A binding spec must be authored inside the Corbusier–Podbot
integration layer that at minimum defines: trigger names, payload schema, ack
semantics, timeout/resume rules, and audit persistence fields.

### Prompt validation request/response and capability dispositions

Corbusier already has an agent capability model (`supports_streaming`,
`supports_tool_calls`, supported content types). Podbot adds a runtime-enforced
capability mask for ACP (`terminal/*`, `fs/*`). To unify these, introduce
prompt-surface capabilities and dispositions.

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ValidatePromptRequest {
    pub agent: AgentRuntimeSpec,
    pub prompt: PromptDocument,          // parsed frontmatter + body
    pub bundle_refs: Vec<BundleRef>,     // optional: skills/bundles used by the prompt
    pub assumed_mcp_wires: Vec<String>,  // names or ids referenced in frontmatter
    pub assumed_hooks: Vec<String>,      // hook names referenced in frontmatter
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ValidatePromptResponse {
    pub ok: bool,
    pub effective_prompt: Option<EffectivePrompt>, // body + evaluated metadata after drops
    pub diagnostics: Vec<PromptDiagnostic>,
    pub capability_report: Vec<CapabilityDispositionReport>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CapabilityDispositionReport {
    pub capability: String,              // e.g. "acp.terminal", "prompt.jinja2", "mcp.wire:weaver"
    pub disposition: CapabilityDisposition,
    pub details: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum CapabilityDisposition {
    Supported,
    Ignored,     // capability will be dropped; prompt can proceed
    Rejected,    // capability required but unavailable; prompt must fail validation
    Unknown,     // validator cannot decide (should be rare)
}
```

This is the core of the proposed `validate_prompt` behaviour: identify
capabilities the prompt requests that a specific agent runtime will ignore or
reject, with explicit reasons.

## Ports, services, and refactors

This section specifies the concrete ports/traits and the refactors needed in
Corbusier.

### New ports/traits

Corbusier should add three new port families. They mirror the Podbot
recommended responsibility split (policy in Corbusier, wiring in Podbot).

#### PodbotAgentLauncher

A Corbusier port that wraps Podbot’s library orchestration into a stable
Corbusier interface.

```rust
#[async_trait::async_trait]
pub trait PodbotAgentLauncher: Send + Sync {
    async fn prepare_workspace(
        &self,
        ctx: &crate::context::RequestContext,
        workspace: &WorkspaceRuntimeSpec,
    ) -> Result<PreparedWorkspace, PodbotLaunchError>;

    async fn launch_agent(
        &self,
        ctx: &crate::context::RequestContext,
        request: LaunchAgentRequest,
    ) -> Result<LaunchedAgent, PodbotLaunchError>;

    async fn stop_agent(
        &self,
        ctx: &crate::context::RequestContext,
        agent_id: AgentInstanceId,
    ) -> Result<(), PodbotLaunchError>;
}
```

This proposed port should be implemented as a Podbot adapter that does not
violate Podbot’s stdout-purity and no-direct-print requirements.

#### WorkspaceMcpWires

A Corbusier port that calls Podbot’s MCP wire surface (as proposed in Podbot’s
MCP hosting design) and returns injection details.

```rust
#[async_trait::async_trait]
pub trait WorkspaceMcpWires: Send + Sync {
    async fn create_wire(
        &self,
        ctx: &crate::context::RequestContext,
        req: CreateWorkspaceMcpWireRequest,
    ) -> Result<CreateWorkspaceMcpWireResponse, McpWireError>;

    async fn remove_wire(
        &self,
        ctx: &crate::context::RequestContext,
        wire_id: WireId,
    ) -> Result<(), McpWireError>;

    async fn list_wires(
        &self,
        ctx: &crate::context::RequestContext,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<WorkspaceMcpWire>, McpWireError>;
}
```

This is the key boundary where Corbusier transitions from “MCP server lifecycle
manager” to “MCP wire provisioning manager”, matching Podbot’s design.

#### HookCoordinator, HookRegistry, HookPolicyService

Corbusier needs a workflow-governance subsystem consistent with its own design
goals (hook engine and encapsulation management appear as planned features).
Under the user’s hook assumptions, HookCoordinator must also coordinate
acknowledgements to Podbot.

```rust
#[async_trait::async_trait]
pub trait HookRegistry: Send + Sync {
    async fn subscriptions_for(
        &self,
        ctx: &crate::context::RequestContext,
        scope: HookScope,
    ) -> Result<Vec<HookSubscription>, HookError>;
}

#[async_trait::async_trait]
pub trait HookPolicyService: Send + Sync {
    async fn authorize_hook(
        &self,
        ctx: &crate::context::RequestContext,
        request: HookRequestContext,
    ) -> Result<HookDecision, HookPolicyError>;
}

#[async_trait::async_trait]
pub trait HookCoordinator: Send + Sync {
    async fn on_podbot_hook_message(
        &self,
        ctx: &crate::context::RequestContext,
        msg: PodbotHookMessage, // integration type; see flows below
    ) -> Result<PodbotHookAck, HookError>;
}
```

**Unspecified detail:** the Podbot hook message schema and transport
(“podbot→orchestrator channel”) do not exist in current Podbot docs. This
proposed integration interface must be implemented either as a typed callback
channel in the embedding library or an out-of-band transport (e.g. UDS) for CLI
mode. Either way, it must not violate Podbot hosting stdout purity.

### Lifecycle and service refactors

#### Tool registry lifecycle split

Currently, Corbusier’s `tool_registry/services` exports
`McpServerLifecycleService` and `ToolDiscoveryRoutingService`. Corbusier must
split “server definition lifecycle” from “workspace wiring lifecycle”:

- **Keep:** `McpServerLifecycleService` as the CRUD/health layer for globally
  registered MCP server *definitions* (sources), stored in
  `mcp_servers.transport`.
- **Add:** `WorkspaceMcpWireService` as the per-workspace provisioning layer
  that calls Podbot to create wires and persists the returned Streamable HTTP
  endpoints by workspace.
- **Change:** `ToolDiscoveryRoutingService` should no longer assume Corbusier
  is the runtime invoker for containerized agents (Option A). Instead it
  becomes:
  - a “catalogue service” used to materialize tool lists for UI/audit, and/or
  - a “bootstrap tool manifest builder” for agents (by creating wires and
    passing endpoints to agent startup).

This refactor also aligns with Corbusier’s design doc, which already
anticipates a workspace manager and Podbot adapter, but currently only at the
conceptual level.

#### Workspace-wire service & schema

Introduce new persistent entities:

- `workspaces` (or `workspace_runtimes`) that identifies a Podbot workspace /
  volume and correlates to `task_id` and possibly `conversation_id`. (Corbusier
  already has task lifecycle and agent sessions/handoffs persistence patterns.)
- `workspace_mcp_wires` with:
  - `workspace_id`
  - `wire_name` (stable name referenced by prompts)
  - `server_id` (FK to `mcp_servers`)
  - `agent_url` (Streamable HTTP URL returned from Podbot)
  - `headers` (auth headers returned from Podbot)
  - `status` and timestamps

This matches Podbot’s contract: Corbusier says *what to wire*; Podbot returns
*how the container reaches it* (URL + headers).

#### Hook coordinator state machine

Corbusier must store hook execution and acknowledgement for auditability
(consistent with its broader audit goals in tool calls and agent handoffs). A
concrete minimal state machine for hook gating:

- `Pending` (hook requested by Podbot, not yet authorised)
- `Authorized` or `Denied` (after HookPolicyService decision)
- `Acked` (ack delivered to Podbot)
- `Completed` / `Failed` (proposed future states if Podbot later emits
  completion events; currently unspecified)

Corbusier must guarantee idempotent ack behaviour: repeated hook request
messages (e.g. after restart) must not cause duplicate approvals.

### Concrete Corbusier file changes mapping

The following list maps existing Corbusier files (plus a few “new file”
touchpoints) to required refactors. File existence and module layout are
derived from the current repository tree.

- `src/lib.rs`: declares top-level modules (`agent_backend`, `tool_registry`,
  etc.). Proposed change: add new modules `workspace` (encapsulation),
  `hook_engine`, `prompt`, `bundle`, and `podbot_adapter`. Risk / effort: Med.
- `src/main.rs`: stub entry point. Proposed change: replace with real
  server/daemon bootstrap only when Corbusier’s HTTP/event surfaces are
  delivered; not strictly required for library-integration work. Risk / effort:
  Low.
- `docs/corbusier-design.md`: high-level architecture including workspace
  management and the `EncapsulationProvider` concept. Proposed change: update
  to reflect the Podbot MCP wire model (Streamable HTTP), hook ack channel,
  prompt validation surface, and tool-router role shift (Option A). Risk /
  effort: Med.
- `docs/roadmap.md`: delivery plan; workspace encapsulation and hook engine
  remain planned. Proposed change: add explicit milestones for Podbot wire
  provisioning, hook coordination, and prompt validation. Risk / effort: Low.
- `src/tool_registry/domain/transport.rs`: transport modelling for MCP server
  connectivity, including legacy shapes such as HTTP+SSE. Proposed change:
  replace or alias to `McpEndpointSource` (`Stdio`, `StdioContainer`,
  `StreamableHttp`) and treat Streamable HTTP as the default agent-facing
  injection. Risk / effort: High.
- `src/tool_registry/ports/host.rs`: defines the MCP server hosting port
  (`start`, `stop`, `health`, `list_tools`, `call_tool`). Proposed change:
  deprecate for Podbot-hosted agents, keep only for tests/local, and introduce
  new ports `WorkspaceMcpWires` and optionally `McpCatalogReader` for registry
  UI. Risk / effort: High.
- `src/tool_registry/services/lifecycle/mod.rs`: service orchestration for MCP
  server lifecycle. Proposed change: split definition lifecycle from
  workspace-wire lifecycle, moving wire operations out of “server lifecycle”
  into `workspace/wires.rs`. Risk / effort: High.
- `src/tool_registry/services/discovery/log_and_audit.rs`: tool discovery
  logging and audit capture. Proposed change: convert to “registry audit” and
  “wire provisioning audit” for Option A, adding ingestion hooks for
  Podbot-provided tool-call logs if implemented. Risk / effort: Med/High.
- `src/tool_registry/domain/validation.rs`: lightweight schema validation for
  tool parameters. Proposed change: extend or reuse for prompt input schema
  validation, with an explicit later assessment of full JSON Schema. Risk /
  effort: Med.
- `src/tool_registry/adapters/runtime.rs`: in-memory MCP host adapter for
  tests. Proposed change: keep it, add Podbot-wire fakes for integration tests,
  and avoid overloading the module with real Podbot wiring. Risk / effort:
  Low/Med.
- `migrations/..._add_mcp_servers_table/up.sql`: adds `mcp_servers` with
  `transport` JSONB. Proposed change: add migrations for `workspace_runtimes`,
  `workspace_mcp_wires`, `hook_executions`, and prompt/bundle registries if
  persistence is required. Risk / effort: Med.
- `src/agent_backend/domain/capabilities.rs`: agent capability flags
  (`supports_streaming`, `supports_tool_calls`, content types). Proposed
  change: extend with `PromptSurfaceCapabilities` and ACP-related constraints,
  plus capability-to-disposition mapping for validation. Risk / effort: Med.
- `src/agent_backend/services/registry.rs`: backend registry and discovery.
  Proposed change: add runtime-spec resolution that maps backend registry
  entries to `AgentRuntimeSpec` and launches via Podbot when the backend is
  “podbot-hosted”. Risk / effort: Med.
- `src/worker/*` and `src/bin/pg_worker.rs`: background work infrastructure.
  Proposed change: add background sweeps for stale wire cleanup, hook timeout
  handling, and possibly Podbot reconcile loops. Risk / effort: Med.

## Prompt, bundles, and validation

This section proposes a concrete prompt/bundle system that aligns with:

- Anthropic’s skill structure: “skills are folders” with a `SKILL.md`
  containing YAML frontmatter and instructions (minimum frontmatter keys:
  `name`, `description`).
- Claude Code’s use of Markdown + YAML frontmatter for other agent-facing
  instruction artefacts (e.g. output styles).
- Jinja2 template syntax for interpolation (`{{ ... }}` and `{% ... %}`), which
  Goose-style templating uses.

### File taxonomy

1. **Skill** (Anthropic-compatible): directory `skills/<skill-id>/SKILL.md` +
   optional supporting files (`scripts/*`, `references/*`, etc.).
2. **Prompt** (Corbusier/Podbot-compatible): a Markdown prompt that can be run
   by an agent, with YAML frontmatter harmonized with SKILL.md.
3. **Bundle**: a distributable package of skills + prompts + optional MCP
   server definitions + optional hook artefacts.

**Important design choice:** Keep SKILL.md *compatible* with Anthropic by
limiting required frontmatter to `name` and `description`, while permitting
additional namespaced keys under `x-corbusier`, `x-podbot`, etc. This preserves
progressive disclosure conventions while enabling extra metadata.

### Harmonized frontmatter schema

Define a shared “frontmatter contract” used in:

- SKILL.md (compatible superset)
- PROMPT.md (new)
- BUNDLE.yaml (new; not necessarily Markdown)

Core keys:

- `apiVersion`: e.g. `corbusier.dev/v1alpha1`
- `kind`: `Skill | Prompt | Bundle`
- `name`: string (skill id / prompt id)
- `description`: string
- `inputs`: optional schema for prompt parameters
- `capabilities`: prompt-surface requirements (MCP wires, hooks, ACP)
- `mcp`: wire requirements (names and sources)
- `hooks`: subscriptions
- `x-*`: extension namespace blocks

### Prompt file example with Goose/Jinja2 interpolation

```markdown
---
apiVersion: corbusier.dev/v1alpha1
kind: Prompt
name: review-and-fix
description: Review a change set, run configured hooks, and propose a minimal fix.
inputs:
  schema:
    type: object
    required: [task_id]
    properties:
      task_id: { type: string }
      focus: { type: string, default: "correctness" }
capabilities:
  require:
    - mcp.wire:weaver
    - hook:pre-commit
  prefer:
    - mcp.wire:search
  forbid:
    - acp.terminal
    - acp.fs
mcp:
  wires:
    - name: weaver
      server_ref: "mcp_servers/weaver"
    - name: search
      server_ref: "mcp_servers/search"
hooks:
  subscribe:
    - hook_name: pre-commit
      trigger: pre_commit
      workspace_access: read_only
---
# Task: {{ inputs.task_id }}

Working directory: `{{ workspace.container_path }}`

## Instructions

1. Load the relevant files using Weaver (do **not** directly edit files).
2. Analyse the repo for {{ inputs.focus }} risks.
3. Before proposing a patch, ensure the `pre-commit` hook has been acknowledged and allowed by policy.
4. Produce:
   - a short diagnosis
   - a Weaver change plan
   - a validation plan
```

Jinja2 syntax and semantics for `{{ ... }}` substitution and `{% ... %}`
control flow are documented in the upstream Jinja template reference.

### Skill bundle abstraction

Model the bundle after Anthropic’s “skills as folders”, but extend it to
include *prompts*, *MCP definitions*, and *hook artefacts*.

Bundle layout:

```text
bundle/
  BUNDLE.yaml
  skills/<skill-id>/SKILL.md
  prompts/<prompt-id>.md
  mcp-servers/<server-id>.yaml
  hooks/<hook-id>.(sh|tar)
```

Example `BUNDLE.yaml`:

```yaml
apiVersion: corbusier.dev/v1alpha1
kind: Bundle
name: repo-quality-gates
description: A set of skills and prompts for governance and quality enforcement.
version: 0.1.0

skills:
  - id: linting
    path: skills/linting/SKILL.md
  - id: security-review
    path: skills/security-review/SKILL.md

prompts:
  - id: review-and-fix
    path: prompts/review-and-fix.md

mcp_servers:
  - id: weaver
    source:
      stdio:
        command: weaver-mcp
        args: ["--stdio"]
        env: []
  - id: search
    source:
      streamable_http:
        url: "https://search.internal.example/mcp"
        headers: []

hooks:
  - id: pre-commit
    artifact:
      kind: script
      path: hooks/pre-commit.sh
      digest: "sha256:..."
    workspace_access: read_only
```

**Unspecified detail:** Whether Corbusier persists bundles/prompts in its DB
versus loading from a workspace filesystem is not currently defined in
Corbusier. Given Podbot’s host-mount safety model, a practical first iteration
is “bundle lives in repo, Corbusier parses it from the mounted workspace”, then
move to a curated registry later.

### Proposed Podbot `validate_prompt` surface

Podbot should eventually expose validation as:

- a library function for embedders (Corbusier),
- optionally, a CLI `podbot validate-prompt` that emits JSON for operators/CI.

Validation must at minimum enforce the ACP masking reality: if the prompt
requires terminal or fs ACP capabilities, validation should report them as
**ignored** or **rejected** depending on whether the prompt marked them as
required.

Sample request:

```json
{
  "agent": {
    "kind": "custom",
    "mode": "acp",
    "command": "opencode",
    "args": ["acp"],
    "env_allowlist": ["OPENAI_API_KEY"],
    "working_dir": "/workspace"
  },
  "prompt": {
    "name": "review-and-fix",
    "frontmatter": { "capabilities": { "require": ["hook:pre-commit"], "forbid": ["acp.terminal"] } },
    "body": "..."
  },
  "bundle_refs": ["repo-quality-gates@0.1.0"],
  "assumed_mcp_wires": ["weaver", "search"],
  "assumed_hooks": ["pre-commit"]
}
```

Sample response (capability ignored but prompt remains valid):

```json
{
  "ok": true,
  "effective_prompt": {
    "body": "...",
    "applied_drops": ["acp.terminal", "acp.fs"]
  },
  "diagnostics": [
    {
      "severity": "warning",
      "code": "ACP_CAPABILITY_MASKED",
      "message": "Agent runs in ACP mode; terminal/* and fs/* are masked by Podbot and will be ignored.",
      "location": { "frontmatterPath": "capabilities" }
    }
  ],
  "capability_report": [
    { "capability": "acp.terminal", "disposition": "Ignored", "details": "Masked by Podbot ACP policy." },
    { "capability": "acp.fs", "disposition": "Ignored", "details": "Masked by Podbot ACP policy." },
    { "capability": "hook:pre-commit", "disposition": "Supported" }
  ]
}
```

## Security, migration, tests, and documentation

### Security and trust boundary changes

1. **Workspace access and host mounts**  
   If Corbusier uses `host_mount`, it must implement Podbot’s required path
   policy (canonicalize, allowlist roots, reject symlink escapes). Enforcement
   cannot be left solely to operators.

2. **Environment secret passthrough**  
   Corbusier must treat `env_allowlist` as a hard gate for both agent runtime
   and hooks. Podbot’s design explicitly separates “credential injection” from
   “env allowlist” and requires secret redaction.

3. **MCP transport framing and stdout purity**  
   For stdio MCP sources, MCP requires newline-delimited JSON-RPC messages with
   no embedded newlines, and no non-protocol bytes on stdout. Podbot’s hosting
   design mirrors this “protocol purity” goal for its own hosting mode. This
   implies:
   - do not log structured diagnostics onto MCP stdio streams,
   - isolate tool/hook logs into stderr or structured side channels.

4. **Repo access for helper containers**  
   Podbot’s MCP hosting design requires explicit `RepoAccess` for helper
   containers, defaulting to `None`, and distinguishes helper-container sharing
   from the agent container’s own workspace mount. Corbusier must surface this
   in policy/UI and persist it in the server definition schema.

5. **ACP delegation**  
   Corbusier must not rely on ACP’s “IDE-host tools” for file system or
   terminal operations when using Podbot-hosted agents; Podbot masks them by
   default. Any override to allow ACP delegation is a trust-boundary change
   that Corbusier should treat as policy-controlled and auditable.

### Migration plan

A staged rollout should preserve functioning parts of Corbusier’s current tool
registry while introducing Podbot-wired operation safely.

- **Backwards compatibility adapters**  
  Corbusier currently models transport in `tool_registry/domain/transport.rs`
  with historical variants; add a compatibility layer:
  - map legacy `http_sse` records to `streamable_http` where possible
    (Streamable HTTP may optionally employ SSE for streaming, but the defining
    contract is Streamable HTTP).  
  - keep legacy parsing to avoid migration failures, but re-serialize to the
    new source model on update.

- **Legacy SSE adapter**  
  If Corbusier currently expects SSE as a first-class transport, treat it as
  deprecated and only supported via bridging layers (Podbot can optionally use
  SSE within Streamable HTTP; Corbusier should not model SSE as its own stable
  transport). Any dedicated “SSE-only” support should be explicitly labelled
  legacy and isolated behind an adapter boundary.

- **Staged rollout**  
  1) Ship schema + domain-type changes; keep existing lifecycle tests green
     (using in-memory host).  
  2) Add Podbot-wire provisioning for one “golden path” MCP server and one
     workspace strategy (likely host_mount).  
  3) Enable prompt/bundle parsing and validation in “warn-only” mode
     (diagnostics logged/audited but not blocking).  
  4) Enforce policy gates and hook acknowledgements in “block” mode.

### Tests and QA requirements

Corbusier already emphasises deterministic testing via in-memory adapters and
structured audit trails. Extend this with:

- **Unit tests**
  - transport conversion: legacy → new `McpEndpointSource`
  - prompt parsing + frontmatter validation
  - capability disposition mapping (ACP masked capabilities must produce
    deterministic diagnostics)

- **Integration tests**
  - `WorkspaceMcpWires` fake that simulates Podbot returning Streamable HTTP
    endpoints and headers (URL + auth).
  - hook coordinator idempotency: duplicate hook requests after restart must
    not double-ack.

- **E2E tests (requires real Podbot + container engine)**
  - create workspace (host mount), create 2 MCP wires, launch ACP agent, ensure:
    - terminal/fs capabilities are masked (the proposed `validate_prompt`
      surface warns appropriately),
    - hooks suspend and resume correctly across ack,
    - failure/restart scenario: Corbusier restarts mid-hook; it resumes and
      acks exactly once.

### Documentation and roadmap updates

Corbusier documentation must reflect the doctrinal shift where Podbot owns
runtime mechanics:

- Update Corbusier design doc sections describing tool hosting and workspace
  encapsulation, replacing “HTTP+SSE manager” framing with “Podbot MCP wires
  presenting Streamable HTTP to agent containers”.
- Update Corbusier roadmap to include:
  - Podbot wire provisioning milestone (under encapsulation/workspace
    management),
  - hook coordinator + ack loop milestone (hook engine),
  - prompt validation milestone (external interface + governance).

### Implementation timeline

The timeline below shows the recommended sequencing for the main Corbusier and
Podbot integration workstreams.

Figure: Corbusier–Podbot staged implementation timeline.

```mermaid
gantt
title Corbusier–Podbot conformance staged plan
dateFormat  YYYY-MM-DD
axisFormat  %d %b

section Foundations
Domain model + DB migrations [M]          :a1, 2026-03-17, 14d
Legacy transport adapters [M]             :a2, after a1, 10d

section Podbot integration
Podbot adapter: AgentLauncher [H]         :b1, after a1, 20d
Workspace MCP wire service [H]            :b2, after b1, 20d

section Hooks
HookCoordinator + state machine [H]       :c1, after b1, 20d
Hook protocol + ack integration [H]       :c2, after c1, 15d

section Prompts and bundles
Prompt/Bundle parsing + frontmatter [M]   :d1, after a1, 15d
Proposed Podbot validate_prompt [M]       :d2, after d1, 15d
Corbusier policy gating + diagnostics [M] :d3, after d2, 10d

section Quality
Integration + e2e scenarios [H]           :e1, after b2, 25d
Docs + roadmap updates [L]                :e2, after d3, 7d
```

Legend: `[H]` high effort, `[M]` medium effort, `[L]` low effort.

### Hook protocol message flow

This sequence diagram implements the required “Podbot sends hook messages;
execution suspends until Corbusier acknowledges” assumption, without violating
Podbot stdout purity (hook events travel over a dedicated library event
channel, not stdout). The post-ack `HookCompleted` / `HookSkipped` messages
shown below are proposed future extensions rather than current Podbot contracts.

```mermaid
sequenceDiagram
    participant A as Agent runtime (in container)
    participant P as Podbot (host process / library)
    participant C as Corbusier (orchestrator)
    participant H as HookPolicyService

    Note over P,A: Agent running
    Note over P,A: Podbot mediates container lifecycle
    P->>C: HookRequested
    Note over P: Podbot suspends execution until ack
    C->>H: authorize_hook
    H-->>C: decision allow or deny
    C-->>P: HookAck
    alt decision=Allow
        P->>P: execute hook with explicit workspace mount policy
        P->>C: HookCompleted
    else decision=Deny
        P->>C: HookSkipped
    end
    Note over P,A: Podbot resumes agent execution after ack
```

**Unspecified but necessary additions:** the `HookCompleted` / `HookSkipped`
messages and their payload fields are not part of current Podbot docs. Treat
them as proposed future events for deployments that require post-hook auditing
and failure propagation beyond the single ack gate.

______________________________________________________________________

This design deliberately confines new “runtime privilege” (container wiring,
tool bridging, hook execution) to Podbot, while evolving Corbusier into a
policy-driven orchestrator that provisions workspaces and wires, validates
prompts/bundles against agent runtimes, and controls governance hooks via
explicit acknowledgements—matching the primary Podbot design intent and MCP
transport requirements.
