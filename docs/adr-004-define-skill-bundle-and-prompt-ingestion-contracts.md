# Architectural decision record (ADR) 004: Define skill bundle and prompt ingestion contracts

## Status

Proposed.

## Date

2026-03-16.

## Context and problem statement

Podbot needs to accept skills, bundles, and prompts from the orchestrator and
materialize them for the hosted agent. Three external standards bear on this
design:

- The Agent Skills specification[^1] defines a portable skill unit: a
  directory with `SKILL.md`, optional `scripts/`, `references/`, and `assets/`
  subdirectories, plus standard YAML frontmatter fields (`name`, `description`,
  `license`, `compatibility`, `metadata`, `allowed-tools`).
- Agent runtimes such as Goose and Claude Code already discover skills from
  portable shared locations such as `~/.config/agents/skills/` and
  `./.agents/skills/`.[^1]
- Anthropic's enterprise guidance pushes role-based bundles with catalogue
  metadata such as owner, version, dependencies, and evaluation status.

No current Podbot specification defines what artefacts the orchestrator
provides, what Podbot does with them, or where the boundary sits between
orchestrator curation and Podbot materialization.

## Decision drivers

- Podbot is a runtime executor, not a catalogue or policy engine. Bundle
  curation, logical naming, and approval workflows are orchestrator concerns
  (consistent with the MCP server hosting design, which assigns policy and
  registry to the orchestrator and transport and lifecycle to Podbot[^2]).
- Standard skill directories should be preserved untouched. Podbot should not
  require skills to be repackaged into a proprietary format.
- Podbot must stage artefacts deterministically and clean them up on session
  teardown.
- Prompts need a Podbot-specific artefact definition that works alongside
  skills but serves a different purpose (agent instructions versus tool
  capabilities).

## Requirements

### Functional requirements

- Podbot accepts resolved artefacts or content-addressed references from the
  orchestrator, not logical catalogue names.
- Skills conforming to the Agent Skills specification[^1] are staged without
  modification.
- Podbot defines a bundle manifest format that groups skills, prompts, Model
  Context Protocol (MCP) server definitions, and hook artefacts into a single
  deployable unit.
- Podbot defines a prompt artefact format (`*.prompt.md`) that uses YAML
  frontmatter plus Markdown body (see ADR 005 for frontmatter schema).
- The orchestrator can supply artefacts as filesystem paths (for host-mounted
  workspaces) or as content-addressed references that Podbot resolves from a
  staging area.

### Technical requirements

- Artefact staging is idempotent: re-staging the same content-addressed
  artefact produces the same filesystem layout.
- Staged artefacts are read-only within the session area to prevent
  accidental mutation.
- Artefact cleanup on session teardown is deterministic and complete.

## Artefact taxonomy

### Skills

A skill is a directory conforming to the Agent Skills specification.[^1] The
minimum required content is a `SKILL.md` file with `name` and `description`
frontmatter fields. Podbot treats the skill directory as opaque: it stages the
directory into the appropriate discovery location without inspecting or
transforming its contents beyond frontmatter validation during prompt
validation (ADR 006).

### Prompts

A prompt is a Markdown file with YAML frontmatter that provides agent
instructions for a specific task or workflow. Podbot identifies prompt
artefacts by the `.prompt.md` suffix. The frontmatter schema is defined in ADR
005.

### Bundles

A bundle is a directory containing a manifest (`bundle.yaml`) that references
skills, prompts, MCP server definitions, and hook artefacts. The bundle
manifest is a Podbot-specific format; it is not part of the Agent Skills
specification.

### Bundle manifest schema

```yaml
# bundle.yaml
apiVersion: podbot.dev/v1alpha1
kind: Bundle
name: <bundle-name>
description: <human-readable description>
version: <semver>

skills:
  - id: <skill-name>
    path: skills/<skill-name>/SKILL.md

prompts:
  - id: <prompt-name>
    path: prompts/<prompt-name>.prompt.md

mcp_servers:
  - id: <server-name>
    source:
      stdio:
        command: <command>
        args: [<arg>, ...]
        env: []
      # OR
      stdio_container:
        image: <image-ref>
        command: [<cmd>, ...]
        env: []
        repo_access: none  # none | read_only | read_write
      # OR
      streamable_http:
        url: <url>
        headers: []

hooks:
  - id: <hook-name>
    artefact:
      kind: inline_script  # or container_image
      path: hooks/<hook-name>.sh
      digest: "sha256:..."
    workspace_access: read_only  # none | read_only | read_write
```

The `apiVersion` field uses a versioned namespace to allow schema evolution.
The `v1alpha1` version indicates this schema is experimental and may change
before stabilization.

## Ingestion contract

The orchestrator provides artefacts to Podbot through the launch request (ADR
007). Podbot's responsibilities during ingestion are:

1. **Accept:** Receive artefact references (paths or content-addressed refs)
   from the orchestrator.
2. **Validate:** Parse manifests and frontmatter. Report structural errors
   via the session event stream (ADR 002) without aborting unless errors are
   fatal.
3. **Stage:** Copy or link artefacts into a read-only session staging area.
4. **Materialize:** Place skills into agent-discoverable locations (see
   ADR 007 for materialization rules).
5. **Clean up:** Remove the session staging area on session teardown.

Podbot does **not**:

- Resolve logical catalogue names to artefact content. The orchestrator
  provides resolved references.
- Evaluate bundle approval status, version compatibility, or tenant scoping.
  These are orchestrator policy concerns.
- Modify skill directory contents. Skills are staged as provided.

## Options considered

### Option A: Podbot-specific bundle manifest with standard skill directories

Define `bundle.yaml` as a Podbot-specific grouping format. Keep Agent Skills
directories untouched. Prompts use a Podbot-specific `.prompt.md` format.

Consequences: clean separation of concerns. Podbot defines runtime staging; the
orchestrator defines curation. Skills remain portable.

### Option B: Extend the Agent Skills specification with bundle semantics

Propose bundle grouping as an extension to the Agent Skills spec, using the
`metadata` field or a new top-level manifest.

Consequences: broader ecosystem alignment, but requires upstream coordination
and slows Podbot's delivery timeline. The Agent Skills spec is focused on
individual skill units, not multi-artefact bundles.

### Option C: Podbot accepts only individual artefacts, no bundle concept

The orchestrator sends individual skill paths, prompt content, and hook
artefacts. Podbot has no concept of a bundle.

Consequences: simpler Podbot implementation, but shifts all grouping and
dependency logic to the orchestrator with no standard interchange format. Makes
it harder to version and distribute coherent artefact sets.

| Topic                   | Option A (Podbot bundle) | Option B (extend spec) | Option C (no bundles) |
| ----------------------- | ------------------------ | ---------------------- | --------------------- |
| Delivery speed          | Fast                     | Slow (upstream coord)  | Fastest               |
| Skill portability       | Preserved                | Preserved              | Preserved             |
| Bundle versioning       | Podbot-owned             | Spec-owned             | Orchestrator-owned    |
| Ecosystem alignment     | Partial                  | Full                   | Minimal               |
| Orchestrator complexity | Moderate                 | Moderate               | High                  |

_Table 1: Comparison of artefact ingestion strategies._

## Decision outcome / proposed direction

**Option A: Podbot-specific bundle manifest with standard skill directories.**

This preserves Agent Skills portability, gives Podbot a concrete manifest
format for grouping related artefacts, and keeps delivery within Podbot's
control without requiring upstream specification changes.

The bundle manifest schema uses `apiVersion: podbot.dev/v1alpha1` to signal
experimental status. Graduation to `v1` requires a superseding ADR.

## Goals and non-goals

- Goals:
  - Define what Podbot accepts from the orchestrator for skills, bundles,
    and prompts.
  - Define what Podbot refuses to own (curation, policy, catalogue naming).
  - Keep standard Agent Skills directories untouched.
  - Provide a versioned bundle manifest format.
- Non-goals:
  - Define the prompt frontmatter schema (see ADR 005).
  - Define artefact materialization order and locations (see ADR 007).
  - Define bundle curation policy or approval workflows (orchestrator
    concern).
  - Define MCP server policy or registry semantics (orchestrator concern,
    per the MCP server hosting design[^2]).

## Known risks and limitations

- The `bundle.yaml` format is Podbot-specific and not yet adopted by any
  other tool. Mitigation: keep the format simple and document a migration path
  if a broader bundle specification emerges.
- Content-addressed references require the orchestrator to compute and
  provide digests. Mitigation: path-based references remain supported for
  host-mounted workspaces where content addressing is unnecessary.

## Outstanding decisions

- Whether `bundle.yaml` should support dependency declarations between
  bundles (for example, "this bundle requires bundle X at version Y").
  Recommendation: defer until a concrete use case emerges; bundle dependencies
  add significant resolution complexity.
- Whether Podbot should validate skill `SKILL.md` frontmatter during
  ingestion or only during prompt validation (ADR 006). Recommendation:
  validate during ingestion and report diagnostics, but do not block staging on
  validation warnings.
- The `apiVersion` graduation path from `v1alpha1` to `v1`.

______________________________________________________________________

[^1]: Agent Skills specification. See <https://agentskills.io/specification>.

[^2]: MCP server hosting design. See `docs/mcp-server-hosting-design.md`.
