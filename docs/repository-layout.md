# Repository layout

This document explains the responsibilities of the main paths in the Podbot
repository. It is an orientation guide for contributors; it does not replace
the source tree as the authoritative structure.

## Top-level structure

The following tree is intentionally compact. It highlights paths that carry
long-lived source, test, and documentation responsibilities.

```plaintext
.
|-- AGENTS.md
|-- Cargo.toml
|-- Makefile
|-- README.md
|-- rust-toolchain.toml
|-- .github/
|-- docs/
|   |-- contents.md
|   |-- repository-layout.md
|   |-- *-design.md
|   |-- adr-*.md
|   `-- execplans/
|-- src/
|   |-- api/
|   |-- bin_tests/
|   |-- cli/
|   |-- config/
|   |-- engine/
|   |-- github/
|   |-- error.rs
|   |-- lib.rs
|   `-- main.rs
`-- tests/
    |-- bdd_*_helpers/
    |-- features/
    |-- fixtures/
    |-- test_support/
    `-- ui/
```

_Figure 1: Compact repository layout for contributor orientation._

## Path responsibilities

| Path | Responsibility |
| ---- | -------------- |
| `AGENTS.md` | Normative agent and contributor instructions for code style, documentation upkeep, Rust conventions, and quality gates. |
| `Cargo.toml` and `Cargo.lock` | Workspace package metadata, dependency declarations, feature configuration, and the locked dependency graph. |
| `Makefile` | Canonical build, lint, test, formatting, Markdown, and diagram validation entrypoints. |
| `README.md` | Public project overview and first contact for readers outside the maintainer workflow. |
| `rust-toolchain.toml` | Rust toolchain pin used by local builds and continuous integration (CI). |
| `.github/` | GitHub automation such as dependency updates and CI workflows. |
| `docs/` | Long-lived project knowledge base, including guides, design documents, Architecture Decision Records (ADRs), roadmaps, and plans. |
| `src/api/` | Stable and experimental library-facing API surfaces for embedders and integration callers. |
| `src/bin_tests/` | Support code used by binary-facing tests. |
| `src/cli/` | Command-line interface parsing, command wiring, and operator-facing behaviour. |
| `src/config/` | Configuration models, layered loading, environment overrides, validation, and related testable helpers. |
| `src/engine/` | Container-engine integration, execution modes, protocol proxying, repository cloning, credential upload, and git-identity configuration. |
| `src/github/` | GitHub App authentication, credential validation, installation-token acquisition, and GitHub error classification. |
| `src/error.rs` | Project error boundary and domain error types. |
| `src/lib.rs` | Public library root and exported module boundary. |
| `src/main.rs` | Binary entrypoint that wires the command-line application to the library. |
| `tests/` | Integration, behaviour, feature, fixture, and helper code used to validate externally observable workflows. |

_Table 1: Top-level path responsibilities._

## Documentation directories

The `docs/` directory is the canonical home for durable project knowledge.
[Documentation contents](contents.md) is the index for the set and must be
updated whenever documents are added, renamed, or removed.

Design documents use `*-design.md` names and describe architecture, rationale,
constraints, and intended evolution. ADRs use `adr-NNN-short-description.md`
names and record accepted or proposed architectural decisions. Execution plans
belong under `docs/execplans/` and describe task-specific implementation work
when a branch needs a living plan.

## Source tree

The `src/` tree contains the Rust library and command-line application.
Feature-specific modules should stay grouped around the behaviour they own
rather than split by technical layer.

- `src/lib.rs` defines the library entry point and public module surface.
- `src/main.rs` defines the command-line interface (CLI) binary entry point.
- `src/api/` contains API-facing orchestration types and functions.
- `src/cli/` contains command-line parsing and argument conversion.
- `src/config/` contains configuration types, layered loading, environment
  overrides, and validation.
- `src/engine/` contains container-engine integration, execution, repository
  cloning, credential upload, and git-identity configuration.
- `src/github/` contains GitHub App authentication, credential validation,
  installation-token acquisition, and GitHub error classification.
- `src/bin_tests/` contains support code used by binary-facing tests.
- `src/error.rs` defines the project error boundary and domain error types.

## Test layout

Repository-level tests live under `tests/` and are grouped by observable
behaviour or support responsibility.

- `tests/bdd_*.rs` files are behaviour-driven development (BDD) scenario entry
  points.
- `tests/bdd_*_helpers/` directories contain scenario state, step definitions,
  fixtures, and assertions for BDD suites.
- `tests/features/` contains Gherkin feature files used by BDD tests.
- `tests/fixtures/` contains reusable test data.
- `tests/test_support/` contains shared support utilities for integration and
  behaviour tests.
- `tests/ui/` contains compile-contract fixtures used by trybuild tests.
- Other `tests/*.rs` files are integration, embedding, configuration, and
  Makefile-target tests.

Keep helper code close to the behaviour it supports unless it is genuinely
shared across unrelated suites. Shared support should make test intent clearer
without hiding the scenario being exercised.

## Generated and transient artefacts

Build outputs belong under Cargo's `target/` directory and should not be
committed. Command logs, scratch files, and temporary validation output should
be written outside the repository, typically under `/tmp`, so review diffs stay
focused on source and documentation changes.
