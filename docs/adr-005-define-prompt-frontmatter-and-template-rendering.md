# Architectural decision record (ADR) 005: Define prompt frontmatter and template rendering

## Status

Proposed.

## Date

2026-03-16.

## Context and problem statement

Podbot prompt artefacts (`.prompt.md` files, defined in ADR 004) need a
harmonized frontmatter schema and a rendering model for template interpolation.
Two external conventions inform this design:

- The Agent Skills specification[^1] defines YAML frontmatter for `SKILL.md`
  with core fields: `name`, `description`, `license`, `compatibility`,
  `metadata`, and `allowed-tools`.
- Goose recipes support Jinja-style interpolation in prompt-like fields,
  including `{{ ... }}` variable substitution, `{% ... %}` control flow, block
  inheritance, escaping for literal template text, and the `indent()` filter.

Without a decision, each team that writes prompts will invent its own
frontmatter keys and interpolation conventions, producing a dialect soup that
future maintainers will curse.

This ADR settles the frontmatter schema for Podbot prompt artefacts and the
exact Jinja rendering subset that Podbot supports.

## Decision drivers

- Reuse existing field semantics from the Agent Skills specification where
  they mean the same thing, to reduce cognitive overhead for authors who
  already write `SKILL.md` files.[^1]
- Adopt Goose/Jinja-style interpolation rather than inventing a home-brew
  templating dialect, to provide a practical compatibility target.
- Structural frontmatter (capability lists, hook names, MCP wire names) must
  stay literal. Templating structural metadata turns validation into a two-pass
  problem and makes static analysis unreliable.
- Template evaluation must be sandboxed: no filesystem access, no network
  access, no arbitrary helper functions.

## Requirements

### Functional requirements

- Prompt files use YAML frontmatter delimited by `---` fences, followed by a
  Markdown body.
- Frontmatter reuses Agent Skills core fields (`name`, `description`) with
  the same semantics.[^1]
- Podbot-specific fields are added for capabilities, hooks, MCP wires,
  inputs, and output expectations.
- The Markdown body supports Jinja-style interpolation for variable
  substitution and simple control flow.
- Structural frontmatter fields are never subject to template interpolation.

### Technical requirements

- Template rendering uses `StrictUndefined` semantics: referencing an
  undefined variable is an error, not a silent empty string.
- Permitted template constructs: `{{ variable }}`, `{% if %}` / `{% endif %}`,
  `{% for %}` / `{% endfor %}`, escaping via `{% raw %}` / `{% endraw %}`, and
  the `indent()` filter.
- Prohibited template constructs: filesystem access (`include`, `import` from
  files), network access, and arbitrary callable helpers.
- Template evaluation takes a flat context object (inputs, workspace
  metadata) and produces rendered Markdown. It does not modify frontmatter.

## Frontmatter schema

### Core fields (Agent Skills compatible)

| Field         | Required | Type   | Description                                          |
| ------------- | -------- | ------ | ---------------------------------------------------- |
| `name`        | Yes      | String | Prompt identifier. Same constraints as Agent Skills. |
| `description` | Yes      | String | What the prompt does and when to use it.             |

_Table 1: Core frontmatter fields compatible with the Agent Skills
specification._

### Podbot-specific fields

| Field          | Required | Type   | Description                                          |
| -------------- | -------- | ------ | ---------------------------------------------------- |
| `apiVersion`   | Yes      | String | Schema version (e.g. `podbot.dev/v1alpha1`).         |
| `kind`         | Yes      | String | Must be `Prompt`.                                    |
| `inputs`       | No       | Object | JSON Schema for prompt parameters.                   |
| `capabilities` | No       | Object | Capability requirements (see below).                 |
| `mcp`          | No       | Object | MCP wire requirements.                               |
| `hooks`        | No       | Object | Hook subscription declarations.                      |
| `output`       | No       | Object | Expected output format or constraints.               |
| `metadata`     | No       | Object | Arbitrary key-value pairs (Agent Skills compatible). |

_Table 2: Podbot-specific frontmatter fields._

### Capabilities block

The `capabilities` block declares what the prompt expects from the agent
runtime:

```yaml
capabilities:
  require:
    - hook:pre-commit
    - mcp.wire:weaver
  prefer:
    - mcp.wire:search
  forbid:
    - acp.terminal
    - acp.fs
```

- `require`: capabilities that must be available. Validation (ADR 006)
  reports `Invalid` disposition if a required capability is unavailable.
- `prefer`: capabilities that improve the prompt but are not essential.
  Validation reports `Ignored` if unavailable.
- `forbid`: capabilities that must not be active. Useful for prompts designed
  to work without ACP host tools.

### MCP block

```yaml
mcp:
  wires:
    - name: weaver
      server_ref: "mcp_servers/weaver"
    - name: search
      server_ref: "mcp_servers/search"
```

Wire names are literal strings used for validation cross-referencing. The
`server_ref` field references an MCP server definition in the bundle manifest
(ADR 004) or the orchestrator's registry.

### Hooks block

```yaml
hooks:
  subscribe:
    - hook_name: pre-commit
      trigger: pre_commit
      workspace_access: read_only
```

Hook declarations in prompt frontmatter are informational: they tell the
validation surface (ADR 006) which hooks the prompt expects. Actual hook
subscription and artefact binding happen in the launch request (ADR 007).

### Inputs block

```yaml
inputs:
  schema:
    type: object
    required: [task_id]
    properties:
      task_id:
        type: string
      focus:
        type: string
        default: "correctness"
```

The `inputs.schema` field uses JSON Schema (draft 2020-12) to declare prompt
parameters. Template rendering receives the resolved input values.

## Template rendering model

### Rendering scope

Template interpolation applies to:

- The Markdown body of the prompt file.
- Any frontmatter field explicitly marked as free-text (currently: none).

Template interpolation does **not** apply to:

- Structural frontmatter fields (`name`, `capabilities`, `mcp`, `hooks`,
  `inputs`, `apiVersion`, `kind`).
- The `description` field (used for skill discovery matching, must be
  stable).

### Rendering context

The template context is a flat object assembled from:

- `inputs.*` — Resolved prompt parameters after schema validation.
- `workspace.container_path` — The agent's workspace path within the
  container.
- `session.id` — The session identifier.

Additional context keys may be added by extending the context schema in a
future ADR. New keys do not constitute a semver-breaking change because
`StrictUndefined` only errors on _referenced_ undefined variables, not on
_unreferenced_ new context keys.

### Permitted constructs

| Construct             | Syntax                         | Permitted |
| --------------------- | ------------------------------ | --------- |
| Variable substitution | `{{ inputs.task_id }}`         | Yes       |
| Conditional           | `{% if %}` / `{% endif %}`     | Yes       |
| Loop                  | `{% for %}` / `{% endfor %}`   | Yes       |
| Escaping              | `{% raw %}` / `{% endraw %}`   | Yes       |
| Indent filter         | `{{ value &#124; indent(4) }}` | Yes       |
| File inclusion        | `{% include "..." %}`          | No        |
| File import           | `{% from "..." import ... %}`  | No        |
| Arbitrary callables   | `{{ func() }}`                 | No        |
| Network or filesystem | Any                            | No        |

_Table 3: Permitted and prohibited Jinja constructs._

### Undefined variable handling

Podbot uses `StrictUndefined` (or the Rust equivalent) for template evaluation.
Referencing a variable not present in the rendering context produces a template
error, not a silent empty string. This catches typos and missing input bindings
early, before the prompt reaches the agent.

## Prompt file example

```markdown
---
apiVersion: podbot.dev/v1alpha1
kind: Prompt
name: review-and-fix
description: >-
  Review a change set, run configured hooks, and propose a minimal fix.
inputs:
  schema:
    type: object
    required: [task_id]
    properties:
      task_id:
        type: string
      focus:
        type: string
        default: "correctness"
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

1. Load the relevant files using Weaver.
2. Analyse the repository for {{ inputs.focus }} risks.
3. Before proposing a patch, ensure the `pre-commit` hook has been
   acknowledged and allowed by policy.
4. Produce:
   - a short diagnosis
   - a change plan
   - a validation plan
```

## Goals and non-goals

- Goals:
  - Harmonize prompt frontmatter with Agent Skills core fields.
  - Adopt Goose/Jinja-style interpolation with a sandboxed subset.
  - Keep structural frontmatter literal and validatable without rendering.
- Non-goals:
  - Define the validation response model (see ADR 006).
  - Define bundle-level metadata curation (orchestrator concern).
  - Support full Jinja2 feature set (inheritance, macros, custom filters).

## Known risks and limitations

- Restricting template constructs may frustrate authors who expect full
  Jinja2. Mitigation: the permitted subset covers the majority of prompt
  authoring needs (variables, conditionals, loops, indentation). Authors
  requiring advanced logic should move that logic to hook scripts or skill code.
- The `StrictUndefined` policy may cause friction during prompt development.
  Mitigation: the validation surface (ADR 006) can provide a dry-run rendering
  mode that reports undefined variables without failing.

## Outstanding decisions

- Whether to implement rendering via a Rust Jinja library (such as
  `minijinja`) or by invoking a Python Jinja2 subprocess. Recommendation:
  `minijinja`, to avoid a Python runtime dependency and keep rendering
  in-process.
- Whether `description` should eventually become a renderable field (for
  dynamic prompt descriptions). Recommendation: keep it literal for now;
  dynamic descriptions would complicate skill discovery.
- Whether to support custom filters beyond `indent()`. Recommendation:
  defer until a concrete use case justifies the added evaluation surface.

______________________________________________________________________

[^1]: Agent Skills specification. See <https://agentskills.io/specification>.
