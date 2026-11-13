# Architectural decision record (ADR) 008: Define secrets and trust boundaries for hooks, prompts, and validation

## Status

Proposed.

## Date

2026-03-16.

## Context and problem statement

Podbot's existing design draws crisp trust lines around host socket ownership,
`agent.env_allowlist`, host-mount safety checks, and Model Context Protocol
(MCP) helper-container repository access.[^1] [^2] The new API surfaces defined
in ADRs 003–007 (hooks, prompts, bundles, validation, session composition)
introduce additional trust boundaries that must be specified before
implementation lands.

Without explicit trust rules, the new surfaces risk poking holes through the
existing security model: validation might accidentally inspect secrets, hooks
might silently inherit the agent's full environment, unpinned hook images might
introduce supply-chain risk, and hook output might leak credentials into
diagnostic logs.

This ADR answers five questions:

1. May validation inspect the workspace or receive secrets?
2. How do hook runners inherit (or not inherit) the agent environment?
3. Must hook container images be pinned or allowlisted?
4. How does log redaction work for hook output?
5. Is hook workspace access distinct from MCP `RepoAccess`?

## Decision drivers

- The existing security model treats the host socket, credential families,
  and `env_allowlist` as explicit trust boundaries.[^1] New surfaces must
  conform to or deliberately extend these boundaries, never circumvent them.
- Validation is a pre-launch diagnostic tool. It must not have side effects
  or require secrets to function.[^1]
- Hooks execute governance logic on behalf of the orchestrator, not the
  agent. Their trust level is closer to the orchestrator than to the agent
  container.
- MCP `RepoAccess` and hook workspace access govern different trust
  boundaries and must remain separate types (established in ADR 003).

## Requirements

### Functional requirements

- Validation is side-effect free: it does not create containers, modify the
  workspace, or make network requests.
- Validation is secret-free by default: it does not receive or inspect
  environment secrets from `agent.env_allowlist`.
- Hooks receive an explicit environment allowlist, separate from the agent's
  `env_allowlist`.
- Hook container images must be digest-pinned for non-inline hooks.
- Hook stdout and stderr are subject to log redaction of configured sensitive
  keys before being included in session events.
- Hook workspace access uses a dedicated type, not MCP `RepoAccess`.

### Technical requirements

- The validation function signature does not accept secrets or environment
  bindings.
- Hook environment injection uses a dedicated allowlist field on
  `HookSubscription` (ADR 003), not automatic inheritance from the agent or
  host environment.
- Image pinning is enforced at launch request validation (ADR 006): an
  unpinned image reference for a non-inline hook produces an `Error`-severity
  diagnostic.
- Redaction uses a configurable set of key patterns applied to captured hook
  output before it enters the session event stream (ADR 002).

## Trust boundary definitions

### Validation trust boundary

| Property           | Rule                                               |
| ------------------ | -------------------------------------------------- |
| Workspace access   | None. Validation does not mount or read workspace. |
| Secret access      | None. No env vars, no credential files.            |
| Container creation | None. Validation is a pure function.               |
| Network access     | None.                                              |
| Side effects       | None.                                              |
| Input              | Prompt document, agent spec, artefact refs.        |
| Output             | `ValidatePromptResponse` (ADR 006).                |

_Table 1: Validation trust boundary._

Validation operates on artefact metadata and agent configuration, not on
runtime state. This makes it safe to call from continuous integration (CI)
pipelines, pre-commit hooks, or orchestrator planning stages without risk of
data exfiltration or state mutation.

**Exception:** If a future use case requires workspace-aware validation (for
example, checking that referenced files exist), it must be gated behind an
explicit opt-in flag and documented as a trust boundary extension.

### Hook execution trust boundary

| Property           | Rule                                                 |
| ------------------ | ---------------------------------------------------- |
| Workspace access   | Governed by `HookWorkspaceAccess` (ADR 003).         |
| Secret access      | Explicit `env_allowlist` per hook subscription.      |
| Container creation | Sibling container on host engine (not agent inner).  |
| Network access     | Governed by container network policy (default: yes). |
| Agent environment  | Not inherited. Hook gets its own allowlist.          |
| Agent socket       | No access to agent container's inner Podman.         |
| Host socket        | Managed by Podbot; hook container does not receive.  |

_Table 2: Hook execution trust boundary._

Hooks run in an isolated context managed by Podbot on the host-side engine.
They do not share a process namespace, network namespace (unless explicitly
configured), or filesystem namespace with the agent container.

### Prompt and bundle trust boundary

| Property            | Rule                                                |
| ------------------- | --------------------------------------------------- |
| Template rendering  | Sandboxed: no filesystem, network, or callables.    |
| Frontmatter parsing | Pure parse; no code execution.                      |
| Staged artefacts    | Read-only mount into agent container (ADR 007).     |
| Secret embedding    | Prohibited. Prompts must not contain secret values. |

_Table 3: Prompt and bundle trust boundary._

## Hook environment policy

Hook runners do **not** automatically inherit the agent's environment or the
host process environment. Instead:

1. Each `HookSubscription` declares its own `env_allowlist` (ADR 003).
2. At hook execution time, Podbot resolves the allowlisted environment
   variables from the host process environment (the same source as
   `agent.env_allowlist`, but filtered through the hook's own allowlist).
3. Missing allowlisted variables are skipped by default (consistent with the
   agent credential injection contract[^1]).
4. The hook's container environment receives only the resolved variables,
   plus Podbot-injected metadata variables:
   - `PODBOT_HOOK_NAME`: the hook name.
   - `PODBOT_HOOK_TRIGGER`: the trigger that fired.
   - `PODBOT_SESSION_ID`: the session identifier.
   - `PODBOT_INVOCATION_ID`: the hook invocation identifier.

This separation ensures that a hook subscription cannot silently escalate its
access by piggybacking on the agent's broader environment.

## Hook image pinning

For `HookArtefactKind::ContainerImage` artefacts (ADR 003):

- The image reference **must** include a digest (for example,
  `ghcr.io/org/hook:latest@sha256:abc123...`).
- Image references without a digest are rejected during launch request
  validation (ADR 006) with an `Error`-severity diagnostic.
- Inline script hooks (`HookArtefactKind::InlineScript`) do not require
  image pinning because they execute in a Podbot-managed default image.

Digest pinning prevents tag-based supply-chain attacks where a mutable tag
(`latest`, `v1`) is replaced with a compromised image between validation and
execution.

### Operator override

An operator may configure an allowlist of unpinned image patterns (for example,
`ghcr.io/internal/*`) for development or testing environments. This override:

- Must be explicit in Podbot configuration (not a default).
- Produces a `Warning`-severity diagnostic during validation.
- Is logged as a trust boundary relaxation in session events.

## Log redaction

Hook stdout and stderr are captured into bounded byte buffers (ADR 003) and
included in `HookCompletedEvent` session events (ADR 002). Before captured
output enters the event stream:

1. Podbot applies a configurable set of redaction patterns to the captured
   bytes.
2. Redaction patterns are derived from:
   - The hook's `env_allowlist` variable names (values are redacted if they
     appear in output).
   - An explicit `redaction_patterns` list in Podbot configuration (for
     additional patterns such as API key formats).
3. Redacted values are replaced with `[REDACTED]`.
4. Redaction is best-effort: it cannot catch secrets that are transformed
   (base64-encoded, split across lines, and similar).

This matches the existing credential injection contract, which requires that
"secret values are never emitted on stdout, and stderr logging must redact
configured sensitive keys."[^1]

## Hook workspace access versus MCP `RepoAccess`

As established in ADR 003, `HookWorkspaceAccess` and `RepoAccess` are separate
types governing separate trust boundaries:

| Property       | `HookWorkspaceAccess`                             | `RepoAccess`                                |
| -------------- | ------------------------------------------------- | ------------------------------------------- |
| Governs        | Hook runner workspace mount                       | MCP helper container repo mount             |
| Trust question | Should this check inspect or alter the workspace? | Should this server read or modify the repo? |
| Default        | `None`                                            | `None`                                      |
| Defined in     | ADR 003                                           | MCP server hosting design[^2]               |
| Mutation scope | Workspace files                                   | Repository volume                           |

_Table 4: Comparison of workspace access types._

Conflating them would allow a change to MCP access policy to silently alter
hook access policy, or vice versa. They must remain separate enums with
separate configuration paths.

## Goals and non-goals

- Goals:
  - Prevent the new API surfaces from poking holes through existing trust
    boundaries.
  - Make validation side-effect free and secret-free.
  - Give hooks explicit, per-subscription environment control.
  - Require digest-pinned images for hook containers.
  - Apply log redaction to hook output before it enters session events.
  - Keep hook workspace access and MCP `RepoAccess` separate.
- Non-goals:
  - Define network egress policy for hook containers (future enhancement).
  - Define secret rotation or vault integration (orchestrator concern).
  - Define tenant-scoped secret isolation (orchestrator concern).

## Known risks and limitations

- Best-effort redaction cannot catch all secret leakage patterns (encoded,
  split, or transformed secrets). Mitigation: hooks should avoid logging secret
  values, and orchestrators should treat hook output as potentially-sensitive
  material regardless of redaction.
- Digest pinning adds friction to hook image updates. Mitigation: the
  operator override for unpinned images is available for development
  environments, and CI workflows can automate digest resolution.
- Separate environment allowlists for agent and hooks mean the orchestrator
  must configure two lists. Mitigation: bundle manifests (ADR 004) can declare
  hook `env_allowlist` alongside hook artefacts, reducing manual configuration.

## Outstanding decisions

- Whether hook containers should have network access by default or require
  explicit opt-in. Recommendation: default to the container engine's default
  network policy (typically: network access enabled) and add explicit network
  restriction as a future enhancement.
- Whether Podbot should support a "dry-run" hook mode that validates the
  hook artefact and environment without executing, for CI and pre-merge checks.
- Whether redaction patterns should be regular expressions or literal
  string matches. Recommendation: start with literal matches of environment
  variable values, extend to regex patterns if needed.

______________________________________________________________________

[^1]: Podbot design document. See `docs/podbot-design.md`, "Credential
    injection contract" and "Security model" sections.

[^2]: MCP server hosting design. See `docs/mcp-server-hosting-design.md`,
    `RepoAccess` enum.
