# Architectural decision record (ADR) 007: Define session composition and artefact materialization

## Status

Proposed.

## Date

2026-03-16.

## Context and problem statement

Phase 4.5 of the roadmap anticipates `LaunchRequest` and `LaunchPlan` models
scoped to agent kind, mode, workspace source, and credential policy.[^1] With
the addition of prompts (ADR 004, ADR 005), bundles (ADR 004), hooks (ADR 003),
Model Context Protocol (MCP) wires (MCP server hosting design[^2]), and
validation (ADR 006), the launch contract must expand to include these artefact
references and define the order in which Podbot resolves, validates, stages,
mounts, and cleans them up.

Without a defined composition model, each artefact type will develop its own
ad-hoc staging logic, leading to non-deterministic session setup, inconsistent
cleanup, and ambient filesystem state that becomes an accidental API.

## Decision drivers

- The roadmap already defines `LaunchRequest` and `LaunchPlan` as the
  normalized launch contract. This ADR extends rather than replaces that
  model.[^1]
- Artefact materialization must be deterministic: the same inputs produce the
  same filesystem layout in the session staging area.
- Skills must be materialized into locations that target agents can discover
  (for example, `~/.config/agents/skills/` or `./.agents/skills/` per the Agent
  Skills specification[^3]).
- Cleanup must be complete. No session artefacts should persist after session
  teardown.
- The validate surface (ADR 006) should report whether a bundle or prompt
  feature is translated or ignored, so the orchestrator knows what to expect
  before launch.

## Requirements

### Functional requirements

- `LaunchRequest` accepts prompt references or inline prompt content, bundle
  references, an explicit skill subset, hook subscriptions, and MCP wire
  definitions.
- Podbot resolves, validates, and stages all artefacts before starting the
  agent.
- Skills are materialized into agent-discoverable locations when the target
  agent supports portable skill discovery.
- Prompts are rendered (ADR 005) after staging and before agent launch.
- Hook subscriptions are registered with the session event loop (ADR 003)
  during composition.
- MCP wires are provisioned and injected into the agent container
  (MCP server hosting design[^2]) during composition.
- All staged artefacts are removed on session teardown.

### Technical requirements

- The staging area is a dedicated directory within the session runtime path,
  separate from the workspace volume.
- Staged artefacts are mounted read-only into the agent container to prevent
  accidental mutation.
- Materialization order is fixed and documented.

## Extended launch request model

```rust,no_run
pub struct LaunchRequest {
    // Existing fields (per roadmap Step 4.5)
    pub agent: AgentRuntimeSpec,
    pub workspace: WorkspaceSource,
    pub credentials: CredentialPolicy,

    // New fields (this ADR)
    pub prompt: Option<PromptRef>,
    pub bundles: Vec<BundleRef>,
    pub skills: Vec<SkillRef>,
    pub hooks: Vec<HookSubscription>,
    pub mcp_wires: Vec<McpWireDefinition>,
}

pub enum PromptRef {
    Path(String),
    Inline(PromptDocument),
    ContentAddressed { digest: String, registry: String },
}

pub enum BundleRef {
    Path(String),
    ContentAddressed { digest: String, registry: String },
}

pub enum SkillRef {
    Path(String),
    BundleSkill { bundle_ref: BundleRef, skill_id: String },
}
```

`LaunchPlan` is derived from `LaunchRequest` by the normalization step and
includes resolved paths, validated artefacts, and computed materialization
targets.

## Composition and materialization pipeline

Podbot processes the launch request through a fixed pipeline with seven stages:

### Stage 1: Resolve

Resolve all artefact references to concrete filesystem paths or fetched
content. Content-addressed references are resolved against the staging
registry. Path references are resolved against the host filesystem (for
host-mounted workspaces) or the orchestrator-provided staging directory.

Errors at this stage are fatal: the session cannot proceed without resolved
artefacts.

### Stage 2: Validate

Run the validation surface (ADR 006) against the resolved prompt, agent spec,
and artefact set. Report diagnostics via the session event stream (ADR 002).

- Error-severity diagnostics: abort the launch.
- Warning-severity diagnostics: log and proceed.
- Info-severity diagnostics: log only.

### Stage 3: Stage

Copy or link resolved artefacts into a session-scoped staging directory at
`$XDG_RUNTIME_DIR/podbot/<session_id>/stage/`. The staging directory is created
with mode `0700`.

Directory layout within the staging area:

```plaintext
stage/
├── prompt.md           # Rendered prompt (if provided)
├── bundles/
│   └── <bundle-name>/  # Bundle directory (mirrors source)
├── skills/
│   └── <skill-name>/   # Skill directory (Agent Skills layout)
└── hooks/
    └── <hook-name>/    # Hook artefact directory
```

### Stage 4: Render

Render the prompt body using the template engine (ADR 005) with the resolved
input values and session context. The rendered prompt replaces the raw prompt
in the staging area.

Rendering errors are fatal if they result from missing required variables.
Warnings (for example, unused variables) are logged.

### Stage 5: Materialize

Mount staged artefacts into the agent container:

- **Skills:** Materialized into `~/.config/agents/skills/<skill-name>/`
  within the container when the target agent supports portable skill
  discovery.[^3] For agents that do not scan standard skill locations, skills
  are materialized into the workspace root at `.agents/skills/<skill-name>/` as
  a fallback.
- **Prompt:** Made available at a well-known container path (for example,
  `/run/podbot/prompt.md`) for the orchestrator or agent to reference.
- **Hook artefacts:** Staged but not mounted into the agent container. Hooks
  execute in their own isolated context (ADR 003).
- **MCP wires:** Provisioned as Streamable HTTP endpoints injected into the
  agent's environment (MCP server hosting design[^2]).

All materialized paths are mounted read-only.

### Stage 6: Launch

Start the agent process with the normalized launch plan. The session event loop
begins, hook subscriptions are active, and protocol IO is connected.

### Stage 7: Teardown

On session end (normal exit, abort, or crash):

1. Stop the agent process.
2. Tear down MCP wires.
3. Remove the session staging directory and all its contents.
4. Remove materialized skill directories from the container (if the
   container is still accessible; otherwise, container deletion handles
   cleanup).

Teardown is best-effort: if a step fails, subsequent steps still execute.
Failures are logged as diagnostics.

## Options considered

### Option A: Fixed pipeline with session-scoped staging

Define a seven-stage pipeline (resolve → validate → stage → render →
materialize → launch → teardown) with all artefacts in a session-scoped
directory. Deterministic order, deterministic cleanup.

Consequences: predictable, auditable, testable. Slightly more upfront structure
than a loose collection of per-artefact handlers.

### Option B: Per-artefact lazy materialization

Each artefact type manages its own staging and cleanup independently. No
central pipeline. Artefacts are materialized on first access.

Consequences: simpler per-artefact code, but no guaranteed order, no single
cleanup path, and harder to audit session state.

### Option C: Orchestrator-managed materialization

The orchestrator stages all artefacts before calling Podbot. Podbot receives
only pre-staged filesystem paths and does no materialization itself.

Consequences: simplest Podbot implementation, but shifts significant complexity
to every orchestrator, prevents Podbot from enforcing read-only staging or
deterministic cleanup, and creates coupling between orchestrator staging layout
and agent discovery conventions.

| Topic                   | Option A (pipeline)  | Option B (lazy)    | Option C (orchestrator) |
| ----------------------- | -------------------- | ------------------ | ----------------------- |
| Determinism             | Guaranteed           | Not guaranteed     | Orchestrator-dependent  |
| Cleanup reliability     | Single teardown path | Per-artefact       | Orchestrator-dependent  |
| Testability             | Pipeline stages      | Per-artefact mocks | Integration-only        |
| Podbot complexity       | Medium               | Low                | Low                     |
| Orchestrator complexity | Low                  | Low                | High                    |

_Table 1: Comparison of materialization strategies._

## Decision outcome / proposed direction

**Option A: Fixed pipeline with session-scoped staging.**

The deterministic pipeline provides a clear contract for session setup and
teardown, makes debugging reproducible, and keeps the orchestrator's
responsibility limited to providing resolved artefact references rather than
managing agent-specific filesystem conventions.

## Goals and non-goals

- Goals:
  - Extend `LaunchRequest` with prompt, bundle, skill, hook, and MCP wire
    fields.
  - Define the seven-stage composition pipeline and its error handling.
  - Define skill materialization into agent-discoverable locations.
  - Define deterministic teardown.
- Non-goals:
  - Define the full `LaunchPlan` normalization logic (that belongs in design
    or implementation documentation).
  - Define artefact curation or selection policy (orchestrator concern).
  - Define MCP wire provisioning protocol (defined in the MCP server hosting
    design[^2]).

## Known risks and limitations

- Materializing skills into `~/.config/agents/skills/` assumes the agent
  container has a writable home directory at mount time. Mitigation: the
  sandbox image already provides a writable `/root` directory.
- The fixed pipeline may be too rigid for future artefact types that need
  different staging semantics. Mitigation: the pipeline stages are deliberately
  broad (resolve, validate, stage, render, materialize); new artefact types can
  be added to existing stages without restructuring the pipeline.
- Read-only staging prevents the agent from modifying materialized skills.
  This is intentional: skills should be immutable within a session. Agents that
  need to create or modify tool configurations should write to the workspace,
  not to the skill directory.

## Outstanding decisions

- Whether the rendered prompt should be injected as the initial agent
  message, written to a file the agent reads, or both.
- Whether skill materialization should use bind mounts or filesystem copies.
  Recommendation: bind mounts for performance, with copies as a fallback for
  container engines that do not support fine-grained bind mounts.
- Whether the staging directory should use `$XDG_RUNTIME_DIR` or a
  configurable base path. Recommendation: use `$XDG_RUNTIME_DIR` with a
  configuration override for non-standard environments.

______________________________________________________________________

[^1]: Podbot development roadmap. See `docs/podbot-roadmap.md`, Step 4.5:
    "Normalized launch contract."

[^2]: MCP server hosting design. See `docs/mcp-server-hosting-design.md`.

[^3]: Agent Skills specification. See <https://agentskills.io/specification>.
