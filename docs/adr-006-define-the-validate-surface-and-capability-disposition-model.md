# Architectural decision record (ADR) 006: Define the validate surface and capability disposition model

## Status

Proposed.

## Date

2026-03-16

## Context and problem statement

The roadmap commits Podbot to a stable library boundary (Step 5.3[^1]) and a
normalized launch contract (Step 4.5[^1]), but nothing yet defines how an
orchestrator or operator validates a prompt against a specific hosted agent
before launching a session. Without a validation surface, the only way to
discover that a prompt requires capabilities the agent cannot honour is to
launch a session and observe failures at runtime — an expensive feedback loop
that wastes compute and produces confusing error reports.

This ADR defines the validation endpoint: its API shape, request model,
response model, and the capability disposition taxonomy that tells the caller
exactly how each requested capability will be treated by the target agent
runtime.

## Decision drivers

- Podbot already enforces Anthropic Computer Use Protocol (ACP) capability
  masking (`terminal/*` and `fs/*`) for hosted agents.[^2] Validation must
  surface this enforcement before launch, not only at runtime.
- The response must not be a binary pass/fail. Capabilities may be natively
  supported, host-enforced by Podbot, translated, silently ignored, or outright
  invalid. Each disposition has different operational implications.
- Validation must be usable both as a library function (for orchestrator
  embedding) and optionally as a CLI command (for operators and continuous
  integration (CI) pipelines).
- Validation must not have side effects: it inspects artefacts and agent
  configuration, but it does not launch containers, modify the workspace, or
  consume secrets.

## Requirements

### Functional requirements

- The validation surface accepts a prompt document, agent runtime
  specification, and optional bundle and wire references.
- The response includes field-level diagnostics with severity, code, message,
  and frontmatter location.
- The response includes a capability disposition report for each capability
  the prompt declares (required, preferred, or forbidden).
- The response optionally includes a canonicalized frontmatter
  representation and a rendered preview of the prompt body.
- Validation can report missing input variables (from the prompt's `inputs`
  schema) without requiring actual input values.

### Technical requirements

- The validation function is side-effect free and secret-free (see ADR 008).
- The function returns a typed `ValidatePromptResponse`, not a stringly
  typed JSON blob.
- The CLI exposure emits structured JSON for machine consumption and
  human-readable text for operator consumption.

## Capability disposition model

The core of the validation response is the disposition model. Each capability
declared in a prompt's frontmatter receives one of the following dispositions:

| Disposition    | Meaning                                                         |
| -------------- | --------------------------------------------------------------- |
| `Native`       | The agent runtime supports this capability directly.            |
| `HostEnforced` | Podbot enforces or provides this capability outside the agent.  |
| `Translated`   | Podbot translates this capability into an equivalent mechanism. |
| `Ignored`      | The capability will be silently dropped. Prompt can proceed.    |
| `Invalid`      | The capability is required but unavailable. Prompt must fail.   |

_Table 1: Capability disposition taxonomy._

The distinction matters because:

- `Native` capabilities work without Podbot intervention (for example, a Model
  Context Protocol (MCP) wire the agent can directly consume).
- `HostEnforced` capabilities are provided by Podbot's hosting layer (for
  example, hook execution, ACP masking). The agent does not know about them.
- `Translated` capabilities require Podbot to bridge between what the prompt
  requests and what the agent supports (for example, materializing a skill into
  a discovery location the agent scans).
- `Ignored` capabilities are declared but will have no effect. This is a
  warning, not a failure — the prompt can still run, but the author should know
  the capability is ineffective.
- `Invalid` capabilities are required by the prompt but cannot be provided
  by any component. This is a validation failure.

## API shape

### Library function

Screen reader: Rust async function signature for the validate_prompt API.

```rust,no_run
pub async fn validate_prompt(
    request: ValidatePromptRequest,
) -> Result<ValidatePromptResponse, ValidateError> { /* ... */ }
```

### Request type

Screen reader: Rust struct definition for ValidatePromptRequest with agent,
prompt, and bundle fields.

```rust,no_run
pub struct ValidatePromptRequest {
    pub agent: AgentRuntimeSpec,
    pub prompt: PromptDocument,
    pub bundle_refs: Vec<BundleRef>,
    pub assumed_mcp_wires: Vec<String>,
    pub assumed_hooks: Vec<String>,
    pub input_values: Option<serde_json::Value>,
}
```

- `agent`: The target agent's kind, mode, and configuration (see design
  document for `AgentKind` and `AgentMode`).
- `prompt`: Parsed frontmatter and raw body of the prompt artefact.
- `bundle_refs`: Optional bundle references for cross-referencing skill and
  MCP server definitions.
- `assumed_mcp_wires`: Wire names the orchestrator intends to provision.
- `assumed_hooks`: Hook names the orchestrator intends to subscribe.
- `input_values`: Optional input values for template rendering preview. When
  absent, validation reports missing variables without rendering.

### Response type

Screen reader: Rust struct definitions for ValidatePromptResponse and related
diagnostic types.

```rust,no_run
pub struct ValidatePromptResponse {
    pub valid: bool,
    pub diagnostics: Vec<PromptDiagnostic>,
    pub capability_report: Vec<CapabilityDispositionEntry>,
    pub canonicalized_frontmatter: Option<CanonicalFrontmatter>,
    pub rendered_preview: Option<String>,
    pub missing_variables: Vec<String>,
}

pub struct PromptDiagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
    pub location: Option<FrontmatterLocation>,
}

pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

pub struct CapabilityDispositionEntry {
    pub capability: String,
    pub source: CapabilitySource,
    pub disposition: CapabilityDisposition,
    pub details: Option<String>,
}

pub enum CapabilitySource {
    Required,
    Preferred,
    Forbidden,
}

#[non_exhaustive]
pub enum CapabilityDisposition {
    Native,
    HostEnforced,
    Translated,
    Ignored,
    Invalid,
}
```

The `valid` field is `true` when no diagnostic has `Error` severity. Warnings
and info diagnostics do not cause validation failure.

### CLI exposure

Screen reader: Command-line example showing podbot validate-prompt with agent
and prompt arguments.

```plaintext
podbot validate-prompt \
  --agent-kind codex \
  --agent-mode acp \
  --prompt prompts/review-and-fix.prompt.md \
  --format json
```

The CLI command is diagnostic only. It does not launch a session, create
containers, or consume secrets. Output formats:

- `json`: Machine-parseable `ValidatePromptResponse` serialized as JSON.
- `text` (default): Human-readable summary with colour-coded severity.

## Options considered

### Option A: Typed disposition model with field-level diagnostics

Return a structured response with per-capability dispositions and per-field
diagnostics. The `valid` field summarizes overall status.

Consequences: maximum precision. Orchestrators can make fine-grained decisions.
Slightly more complex response type.

### Option B: Binary pass/fail with error list

Return `valid: bool` and a flat list of error strings.

Consequences: simple to implement and consume, but loses the distinction
between ignored, translated, and native capabilities. Orchestrators cannot
distinguish "this will work but a feature is dropped" from "this will not work
at all."

### Option C: Score-based model

Return a compatibility score (0–100) with weighted capability assessments.

Consequences: compact summary, but introduces arbitrary weighting that obscures
root causes and makes thresholds a source of policy disputes.

| Topic                   | Option A (dispositions) | Option B (pass/fail) | Option C (score) |
| ----------------------- | ----------------------- | -------------------- | ---------------- |
| Diagnostic precision    | Per-capability          | Per-error            | Aggregated       |
| Orchestrator actionable | Yes                     | Partially            | No               |
| Implementation cost     | Medium                  | Low                  | Medium           |
| Policy flexibility      | Orchestrator-driven     | Binary               | Threshold-based  |

_Table 2: Comparison of validation response models._

## Decision outcome / proposed direction

**Option A: Typed disposition model with field-level diagnostics.**

This provides the precision necessary for orchestrators to make informed launch
decisions. An orchestrator can:

- Proceed when all required capabilities are `Native`, `HostEnforced`, or
  `Translated`.
- Warn operators when preferred capabilities are `Ignored`.
- Block the launch when required capabilities are `Invalid`.
- Audit which capabilities Podbot is enforcing on behalf of the agent.

### ACP masking example

When validating a prompt against an ACP-mode agent:

- `acp.terminal` declared as `forbid` → disposition `HostEnforced`
  (Podbot masks it), severity `Info`.
- `acp.terminal` declared as `require` → disposition `Invalid` (the
  prompt requires a capability that Podbot blocks), severity `Error`.
- `acp.terminal` not declared → no entry (not relevant to validation).

### MCP wire example

- `mcp.wire:weaver` declared as `require`, weaver in `assumed_mcp_wires` →
  disposition `Native`, severity `Info`.
- `mcp.wire:search` declared as `prefer`, search not in
  `assumed_mcp_wires` → disposition `Ignored`, severity `Warning`.

## Goals and non-goals

- Goals:
  - Define the validation API shape (library function and CLI command).
  - Define the capability disposition taxonomy.
  - Ensure validation is side-effect free and secret-free.
- Non-goals:
  - Define validation policy (the orchestrator decides what to do with the
    response).
  - Define full prompt rendering semantics (see ADR 005).
  - Define agent capability discovery (Podbot uses static configuration,
    not runtime introspection).

## Known risks and limitations

- The disposition model is based on static analysis of frontmatter and agent
  configuration. It cannot detect runtime failures (for example, an MCP server
  that is registered but offline). Mitigation: runtime failures are handled by
  the session event stream (ADR 002), not validation.
- The `#[non_exhaustive]` annotation on `CapabilityDisposition` means
  orchestrators must handle unknown variants gracefully. Mitigation: this is
  standard Rust API practice and allows the taxonomy to grow without semver
  breakage.

## Outstanding decisions

- Whether validation should accept a `LaunchRequest` (ADR 007) directly
  instead of decomposed fields, to reduce the chance of validating against a
  configuration that differs from the actual launch.
- Whether `canonicalized_frontmatter` should normalize field order, fill
  defaults, and strip unknown keys, or preserve the original structure with
  annotations.
- Whether the rendered preview should apply `StrictUndefined` or fall back
  to placeholder markers for missing variables.

______________________________________________________________________

[^1]: Podbot development roadmap. See `docs/podbot-roadmap.md`.

[^2]: Podbot design document. See `docs/podbot-design.md`, "Execution flow"
    section.
