# Architectural decision record (ADR) 001: Define the stable public library boundary

## Status

Proposed.

## Date

2026-03-16.

## Context and problem statement

Podbot is delivered through two first-class surfaces: a CLI binary for terminal
operators and an embeddable Rust library for larger agent-hosting systems such
as Corbusier.[^1] The design document commits to typed library APIs with
semantic errors, and the roadmap explicitly tasks Step 5.3 with documenting
"supported public modules and request/response types."[^2] However, no
governing decision yet defines which modules and types constitute the stable
public contract, which remain internal implementation details, and how
experimental surfaces graduate to stability.

Without an explicit boundary, every `pub` item in `podbot` becomes a de facto
semver commitment the moment an external consumer depends on the crate. That
creates two failure modes: premature stabilisation of internal types that need
to evolve, and accidental breakage of types that consumers reasonably expected
to remain stable.

## Decision drivers

- Podbot must be embeddable as a Rust library dependency with documented,
  versioned APIs and no CLI coupling requirements.[^2]
- Library functions must return typed request and response values plus semantic
  errors (`PodbotError`) so host applications can integrate their own logging,
  retries, scheduling, and policy controls.[^1]
- Library APIs must not depend on CLI-only types and must not perform direct
  process termination.[^1]
- The existing module structure (`api`, `config`, `engine`, `error`, `github`)
  already separates orchestration from infrastructure, but nothing marks which
  items are supported under semver.[^3]
- Future surfaces (hooks, prompts, bundles, validation, Model Context Protocol
  (MCP) wiring) will need a clear graduation path from experimental to stable.

## Requirements

### Functional requirements

- External consumers can import a deliberately small set of stable modules
  and types to launch, configure, and interact with Podbot sessions.
- Internal implementation details (engine connection mechanics, tar archive
  construction, credential file layout) are not part of the public API.
- Experimental or provisional surfaces are clearly separated from stable ones
  and do not create semver obligations until explicitly graduated.

### Technical requirements

- Stable public types use semantic error enums derived from `thiserror`;
  `eyre::Report` never appears in public signatures.[^1]
- Public request and response types are `Clone`, `Debug`, `Serialize`, and
  `Deserialize` where they cross library boundaries.
- CLI-only dependencies and code paths are gated behind a feature boundary or
  binary target so library consumers do not pull in `clap` or presentation
  logic.

## Options considered

### Option A: Re-export a curated public surface from `lib.rs`

Stabilise a small set of modules re-exported from `lib.rs`. Keep implementation
modules `pub(crate)` or behind `#[doc(hidden)]`. Provisional surfaces live
behind a Cargo feature flag (for example, `experimental`) and are excluded from
semver guarantees until graduated.

Consequences: clear boundary, minimal surface, easy to audit. Requires
discipline to keep re-exports narrow.

### Option B: Stabilise entire modules wholesale

Mark complete modules (`api`, `config`, `error`) as public and commit to their
full contents under semver.

Consequences: simpler to explain, but exposes internal helper types, builder
intermediates, and implementation-coupled structures that constrain future
refactoring.

### Option C: Feature-flag every new surface individually

Each new capability (hooks, prompts, validation) gets its own Cargo feature
flag, and callers opt in per feature.

Consequences: maximum granularity, but creates a combinatorial testing matrix
and makes the dependency experience fragile for consumers who need several
features together.

| Topic                     | Option A              | Option B               | Option C                  |
| ------------------------- | --------------------- | ---------------------- | ------------------------- |
| Semver surface size       | Deliberately small    | Module-sized           | Per-feature               |
| Internal refactoring room | High                  | Low                    | Medium                    |
| Consumer ergonomics       | Clear top-level paths | Familiar module layout | Complex feature selection |
| Graduation path           | Single flag           | N/A (all-or-nothing)   | Per-feature promotion     |
| Testing matrix            | Manageable            | Simple                 | Combinatorial             |

_Table 1: Comparison of public boundary strategies._

## Decision outcome / proposed direction

**Option A: Re-export a curated public surface from `lib.rs`.**

The initial stable module set is deliberately small:

- `podbot::launch` — `LaunchRequest`, `LaunchPlan`, and the primary
  orchestration entry points for `run` and `host` flows.
- `podbot::session` — `HostedSession` handle and session lifecycle types
  (proposed in ADR 002).
- `podbot::config` — `AppConfig` and the library-facing configuration loader
  (excluding Clap-dependent CLI adapter types).
- `podbot::error` — `PodbotError` and its semantic variants.
- `podbot::mcp` — MCP wire provisioning request and response types as defined
  in the MCP server hosting design.[^4]

The following modules remain internal (`pub(crate)`) at initial release:

- `engine` — Bollard connection, container creation, exec attachment.
- `github` — Octocrab integration, token daemon internals.
- Any CLI adapter modules.

Implementation structs within stable modules should be opaque where possible,
exposing behaviour through methods rather than public fields, so that internal
representation can evolve without semver-breaking changes.

### Experimental namespace

Surfaces that are not yet ready for semver commitment are placed behind a Cargo
feature flag named `experimental`. Types within this namespace carry an
explicit documentation warning that they may change or be removed in any
release. The `experimental` feature is excluded from the default feature set.

Graduation from `experimental` to stable requires:

1. At least one release cycle of use behind the feature flag.
2. Review confirming the type surface is consistent with the stable API
   conventions (semantic errors, serialisable request/response types, no CLI
   coupling).
3. An update to this ADR or a superseding ADR recording the graduation.

Planned experimental surfaces include:

- `podbot::hooks` — Hook subscription, event, and acknowledgement types
  (see ADR 003).
- `podbot::prompts` — Prompt document and frontmatter types (see ADR 005).
- `podbot::validate` — Validation request, response, and capability
  disposition types (see ADR 006).

## Goals and non-goals

- Goals:
  - Define which modules and types Podbot supports under semver.
  - Provide a clear graduation path for experimental surfaces.
  - Keep the stable contract deliberately boring: small, typed, and
    serialisable.
- Non-goals:
  - Define the internal architecture of any module (that belongs in the
    design document).
  - Define policy for which consumers may embed Podbot (that is an
    orchestrator concern).

## Known risks and limitations

- The initial stable set may prove too narrow if a consumer needs engine-level
  access for a legitimate use case. Mitigation: expose targeted extension
  points (for example, a trait for custom container engines) rather than
  wholesale module stabilisation.
- The `experimental` feature flag adds a small maintenance burden. Mitigation:
  keep the flag singular rather than per-feature (avoiding Option C's
  combinatorial cost) and review quarterly.

## Outstanding decisions

- Whether `podbot::session` should be part of the initial stable set or begin
  in `experimental`, pending ADR 002 acceptance.
- Whether `podbot::config` should expose `AppConfig` fields directly or only
  through builder/loader APIs.
- The exact Cargo feature name (`experimental`, `unstable`, or
  `preview`) — `experimental` is recommended for clarity.

______________________________________________________________________

[^1]: Podbot design document. See `docs/podbot-design.md`, "Dual delivery
    model" section.

[^2]: Podbot development roadmap. See `docs/podbot-roadmap.md`, Step 5.3:
    "Stabilize public library boundaries."

[^3]: Current module structure: `api`, `config`, `engine`, `error`, `github`.
    See `src/lib.rs`.

[^4]: MCP server hosting design. See `docs/mcp-server-hosting-design.md`.
