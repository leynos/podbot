# Repository layout

This document explains where important project files live and what each area is
responsible for.

## Top-level files

- `AGENTS.md` contains repository instructions for coding agents and
  contributors.
- `Cargo.toml` and `Cargo.lock` define the Rust crate, dependencies, features,
  and locked dependency graph.
- `Makefile` defines the canonical quality gates used locally and in
  continuous integration (CI).
- `README.md` introduces the project at a high level.
- `rust-toolchain.toml` pins the Rust toolchain.
- `.github/` contains GitHub Actions workflows and repository automation.

## Source tree

- `src/lib.rs` defines the library entry point and public module surface.
- `src/main.rs` defines the command-line interface (CLI) binary entry point.
- `src/api/` contains stable and experimental API-facing orchestration types
  and functions.
- `src/cli/` contains command-line parsing and argument conversion.
- `src/config/` contains configuration types, layered loading, environment
  overrides, and validation.
- `src/engine/` contains container-engine integration, execution, repository
  cloning, credential upload, and git-identity configuration.
- `src/github/` contains GitHub App authentication, credential validation,
  installation-token acquisition, and GitHub error classification.
- `src/bin_tests/` contains support code used by binary-facing tests.
- `src/error.rs` defines the project error boundary and domain error types.

## Tests

- `tests/bdd_*.rs` files are behaviour-driven development (BDD) scenario entry
  points.
- `tests/bdd_*_helpers/` directories contain scenario state, step definitions,
  fixtures, and assertions for BDD suites.
- `tests/features/` contains Gherkin feature files used by BDD tests.
- `tests/fixtures/` contains reusable test data.
- `tests/test_support/` contains shared test support code.
- `tests/ui/` contains compile-contract fixtures used by trybuild tests.
- Other `tests/*.rs` files are integration, embedding, configuration, and
  Makefile-target tests.

## Documentation

- `docs/contents.md` indexes the documentation set.
- `docs/users-guide.md` documents user-facing workflows and configuration.
- `docs/developers-guide.md` documents maintainer workflows and contribution
  practices.
- `docs/podbot-design.md` is the primary design document.
- `docs/repository-layout.md` is this repository map.
- `docs/documentation-style-guide.md` defines documentation conventions.
- `docs/adr-*.md` files record accepted architectural decisions.
- `docs/execplans/` contains execution plans and implementation history.

## Generated files

- `target/` is Cargo's build output and must not be edited by hand.
