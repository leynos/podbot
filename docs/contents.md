# Documentation contents

This index points to the project documents most useful for users,
contributors, and maintainers. Use it to confirm the canonical names for each
long-lived guide.

## Start here

- [Documentation contents](contents.md) is this index.
- [Users' guide](users-guide.md) explains user-facing workflows,
  configuration, and diagnostics.
- [Developer's guide](developers-guide.md) explains build, test, lint,
  integration, and maintenance practices.
- [Repository layout](repository-layout.md) explains where source code, tests,
  documentation, fixtures, and generated artefacts belong.
- [Documentation style guide](documentation-style-guide.md) defines the writing,
  Markdown, and document-type conventions used across the repository.

## User and contributor guides

- [OrthoConfig user's guide](ortho-config-users-guide.md) explains
  layered configuration through OrthoConfig.
- [rstest-bdd users' guide](rstest-bdd-users-guide.md) explains behaviour
  testing with rstest-bdd.
- [Scripting standards](scripting-standards.md) records standards for scripts
  and command automation.

## Design and roadmap

- [Podbot design](podbot-design.md) describes the architecture, boundaries,
  crate choices, and design rationale for Podbot.
- [Podbot roadmap](roadmap.md) tracks planned delivery work and the
  staged implementation sequence.
- [MCP server hosting design](mcp-server-hosting-design.md) describes the
  hosted Model Context Protocol (MCP) server direction.
- [Corbusier conformance design for agents, MCP wires, and hooks](corbusier-conformance-design-for-agents-mcp-wires-and-hooks.md)
  records related conformance design for agents, MCP wires, and hooks.

## Decision records

- [ADR 001: Define the stable public library boundary](adr-001-define-the-stable-public-library-boundary.md)
  records the accepted public application programming interface (API) boundary.
- [ADR 002: Define the hosted session API and control channel](adr-002-define-the-hosted-session-api-and-control-channel.md)
  records hosted-session control contracts.
- [ADR 003: Define the hook execution primitive and suspend-ack protocol](adr-003-define-the-hook-execution-primitive-and-suspend-ack-protocol.md)
  records hook execution and suspend acknowledgement behaviour.
- [ADR 004: Define skill bundle and prompt ingestion contracts](adr-004-define-skill-bundle-and-prompt-ingestion-contracts.md)
  records skill and prompt ingestion contracts.
- [ADR 005: Define prompt frontmatter and template rendering](adr-005-define-prompt-frontmatter-and-template-rendering.md)
  records prompt metadata and rendering rules.
- [ADR 006: Define the validate surface and capability disposition model](adr-006-define-the-validate-surface-and-capability-disposition-model.md)
  records validation and capability disposition concepts.
- [ADR 007: Define session composition and artefact materialization](adr-007-define-session-composition-and-artefact-materialization.md)
  records session composition and generated artefacts.
- [ADR 008: Define secrets and trust boundaries for hooks, prompts, and validation](adr-008-define-secrets-and-trust-boundaries-for-hooks-prompts-and-validation.md)
  records secret-handling and trust-boundary rules.
- [ADR 009: Define control-plane observability, recovery, and replay](adr-009-define-control-plane-observability-recovery-and-replay.md)
  records observability and replay expectations.

## Reference material

- [Complexity antipatterns and refactoring strategies](complexity-antipatterns-and-refactoring-strategies.md)
  explains code-health smells and refactoring approaches.
- [Reliable testing in Rust via dependency injection](reliable-testing-in-rust-via-dependency-injection.md)
  explains testability patterns for Rust systems.
- [Rust doctest dry guide](rust-doctest-dry-guide.md) explains reusable
  documentation-test patterns.
- [Rust testing with rstest fixtures](rust-testing-with-rstest-fixtures.md)
  explains fixture-driven Rust testing.

## Execution plans

- [Execution plans](execplans/) contains implementation plans and completed
  task records. Use this directory for historical task context rather than as
  the source of truth for current architecture.
