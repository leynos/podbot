# Documentation contents

This index points to the project documents most useful for users,
contributors, and maintainers.

## Start here

- [Documentation contents](contents.md) is this index.
- [Users' guide](users-guide.md) explains user-facing workflows,
  configuration, and diagnostics.
- [Developers' guide](developers-guide.md) explains build, test, lint,
  integration, and maintenance practices.
- [Repository layout](repository-layout.md) explains where important code,
  tests, and documentation live.
- [Documentation style guide](documentation-style-guide.md) defines the writing
  and Markdown conventions used across the repository.

## Design and roadmap

- [Podbot design](podbot-design.md) describes the architecture, boundaries,
  crate choices, and design rationale for Podbot.
- [Podbot roadmap](podbot-roadmap.md) tracks planned delivery work and the
  staged implementation sequence.
- [MCP server hosting design](mcp-server-hosting-design.md) describes the
  hosted Model Context Protocol (MCP) server direction.
- [Corbusier conformance design](corbusier-conformance-design-for-agents-mcp-wires-and-hooks.md)
  records related conformance design for agents, MCP wires, and hooks.

## Decision records

- [ADR 001: Stable public library boundary](adr-001-define-the-stable-public-library-boundary.md)
  defines the stable public application programming interface (API) boundary.
- [ADR 002: Hosted session API and control channel](adr-002-define-the-hosted-session-api-and-control-channel.md)
  defines hosted-session control contracts.
- [ADR 003: Hook execution primitive and suspend acknowledgement protocol](adr-003-define-the-hook-execution-primitive-and-suspend-ack-protocol.md)
  defines hook execution and suspend acknowledgement behaviour.
- [ADR 004: Skill bundle and prompt ingestion contracts](adr-004-define-skill-bundle-and-prompt-ingestion-contracts.md)
  defines skill and prompt ingestion contracts.
- [ADR 005: Prompt frontmatter and template rendering](adr-005-define-prompt-frontmatter-and-template-rendering.md)
  defines prompt metadata and rendering rules.
- [ADR 006: Validate surface and capability disposition model](adr-006-define-the-validate-surface-and-capability-disposition-model.md)
  defines validation and capability disposition concepts.
- [ADR 007: Session composition and artefact materialization](adr-007-define-session-composition-and-artefact-materialisation.md)
  defines session composition and generated artefacts.
- [ADR 008: Secrets and trust boundaries](adr-008-define-secrets-and-trust-boundaries-for-hooks-prompts-and-validation.md)
  defines secret-handling and trust-boundary rules.
- [ADR 009: Control-plane observability, recovery, and replay](adr-009-define-control-plane-observability-recovery-and-replay.md)
  defines observability and replay expectations.

## Reference guides

- [Complexity antipatterns and refactoring strategies](complexity-antipatterns-and-refactoring-strategies.md)
  explains code-health smells and refactoring approaches.
- [Reliable testing in Rust via dependency injection](reliable-testing-in-rust-via-dependency-injection.md)
  explains testability patterns for Rust systems.
- [Rust doctest DRY guide](rust-doctest-dry-guide.md) explains reusable
  documentation-test patterns.
- [Rust testing with rstest fixtures](rust-testing-with-rstest-fixtures.md)
  explains fixture-driven Rust testing.
- [rstest-bdd users' guide](rstest-bdd-users-guide.md) explains behaviour
  testing with rstest-bdd.
- [OrthoConfig users' guide](ortho-config-users-guide.md) explains layered
  configuration through OrthoConfig.
- [Scripting standards](scripting-standards.md) records standards for scripts
  and command automation.

## Execution plans

- [Execution plans](execplans/) contains implementation plans and completed
  task records. Use this directory for historical task context rather than as
  the source of truth for current architecture.
