# Agent and workspace configuration

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: DRAFT

PLANS.md is not present in the repository.

## Purpose / Big Picture

Define the missing pieces of the configuration module for agent execution and
workspace location. The goal is to add `agent.mode` alongside `agent.kind`,
confirm the default workspace base directory, and prove the behaviour with unit
and behavioural tests. Success is observable when configuration parsing accepts
valid agent modes, rejects invalid ones, and `make all` passes without warnings.

## Constraints

- Keep all module-level `//!` documentation in place and update it if the
  configuration surface changes.
- Use `camino::Utf8PathBuf` for filesystem paths; do not introduce
  `std::path::PathBuf` in configuration types.
- Use en-GB-oxendict spelling in documentation and comments.
- Do not add new dependencies or feature flags without explicit approval.
- Avoid mutating global environment variables in tests; use dependency
  injection patterns if environment interactions are required.
- Prefer Makefile targets and capture long output with `set -o pipefail` and
  `tee`.
- No `unwrap` or `expect` outside test code.
- No single code file may exceed 400 lines; if adding fields increases
  `src/config.rs` further, split into submodules under `src/config/`.

## Tolerances (Exception Triggers)

- Scope: if the change requires edits to more than 8 files or more than 300
  lines of net change, stop and ask for confirmation.
- Interface: if a public API outside `crate::config` must change, stop and ask
  for confirmation.
- Dependencies: if a new crate or feature flag is required, stop and ask for
  confirmation.
- Ambiguity: if the allowed values for `agent.mode` are not clearly stated in
  `docs/podbot-design.md` or `docs/users-guide.md`, stop and request a decision
  before implementing.
- Iterations: if tests fail after two fix attempts, stop and ask for
  confirmation with the captured log paths.

## Risks

- Risk: `src/config.rs` already exceeds the 400-line limit, so adding fields
  may require a refactor into submodules. Severity: medium. Likelihood: high.
  Mitigation: split the configuration module into smaller files if any edits
  are required, and update imports/tests accordingly.
- Risk: `agent.mode` semantics are currently underspecified, which could lead
  to incompatible defaults. Severity: medium. Likelihood: medium. Mitigation:
  confirm the intended modes from the design and user documentation and record
  the decision in `docs/podbot-design.md`.
- Risk: new BDD steps may conflict with existing step definitions. Severity:
  low. Likelihood: low. Mitigation: review `tests/bdd_config.rs` and reuse
  helper patterns to avoid duplicate step names.

## Progress

- [ ] (2026-01-17 UTC) Inspect current configuration code and docs for agent
  mode and workspace expectations.
- [ ] Define `AgentMode`, update `AgentConfig`, and ensure `WorkspaceConfig`
  stays consistent with design defaults.
- [ ] Add unit tests with `rstest` for agent mode and workspace base
  directory, including unhappy-path cases.
- [ ] Add `rstest-bdd` scenarios and step definitions for agent mode and
  workspace overrides.
- [ ] Update documentation (`docs/podbot-design.md`, `docs/users-guide.md`) and
  mark the roadmap entry as done.
- [ ] Run validation (`make check-fmt`, `make lint`, `make test`, `make all`,
  plus doc tooling if docs changed) with logs captured.

## Surprises & Discoveries

- Observation: `AgentConfig` and `WorkspaceConfig` already exist in
  `src/config.rs`, but `AgentConfig` lacks `mode`. Evidence: current struct
  definitions in `src/config.rs`. Impact: focus on adding `agent.mode` rather
  than introducing new structs.

## Decision Log

- Decision: _Pending._ Confirm the allowed values and default for
  `agent.mode` before implementation and document the rationale in
  `docs/podbot-design.md`.

## Outcomes & Retrospective

_To be completed after implementation._

## Context and Orientation

Configuration types and CLI parsing live in `src/config.rs`. Unit tests for
configuration live at the bottom of `src/config.rs`. Behavioural configuration
coverage uses `tests/bdd_config.rs` with scenarios in
`tests/features/configuration.feature`. The design intent for configuration is
in `docs/podbot-design.md`, while user-facing behaviour is documented in
`docs/users-guide.md`. The roadmap entry for this task is under Step 1.3 in
`docs/podbot-roadmap.md`.

`agent.kind` currently supports `claude` and `codex` via `AgentKind`. The
configuration example in the design document already includes
`agent.mode = "podbot"`, which implies an additional field and a defined set
of valid values.

## Plan of Work

Stage A: Inspection and clarification. Review `src/config.rs`,
`docs/podbot-design.md`, and `docs/users-guide.md` to confirm expectations for
agent mode and workspace defaults. If the set of `agent.mode` values is unclear
beyond `podbot`, stop and request clarification before implementing.

Stage B: Configuration definitions. Add an `AgentMode` enum and extend
`AgentConfig` with a `mode` field. Ensure defaults align with the design
example. If required to meet the 400-line limit, split configuration into
submodules (for example `src/config/agent.rs`, `src/config/workspace.rs`, and
`src/config/mod.rs`) and update module paths. Keep `WorkspaceConfig` using
`Utf8PathBuf` and ensure the default remains `/work` unless the design states
otherwise.

Stage C: Unit tests. Use `rstest` to cover:
- default `AgentConfig` values (kind and mode),
- serialisation/deserialisation of `AgentMode`,
- invalid `agent.mode` values returning a parse error,
- workspace base directory defaults and explicit overrides.

Stage D: Behavioural tests. Add `rstest-bdd` scenarios to
`tests/features/configuration.feature` that verify:
- the default `agent.mode` when no configuration is supplied,
- invalid `agent.mode` entries are rejected,
- workspace base directory overrides when provided in a configuration file.
Update `tests/bdd_config.rs` with step definitions and scenario bindings.

Stage E: Documentation and roadmap. Record the `agent.mode` decision and
behaviour in `docs/podbot-design.md`, update `docs/users-guide.md` to include
`agent.mode` in the configuration example and environment variable table, and
mark the relevant Step 1.3 tasks as done in `docs/podbot-roadmap.md`.

Stage F: Validation. Run the required Makefile targets with captured logs,
including documentation tooling if any Markdown files were modified.

## Concrete Steps

1) Inspect current configuration and documentation.

    - Command:

          rg -n "AgentConfig|AgentKind|WorkspaceConfig" src/config.rs

    - Command:

          sed -n '1,260p' src/config.rs

    - Command:

          sed -n '180,230p' docs/podbot-design.md

    - Command:

          sed -n '80,160p' docs/users-guide.md

2) Define `AgentMode` and update `AgentConfig`.

    - Add a new enum (for example `AgentMode`) with a `serde` lowercase
      representation and `clap::ValueEnum` support.
    - Add `mode: AgentMode` to `AgentConfig` with a default that matches the
      design example (currently `podbot`).
    - If required to meet the 400-line limit, split `src/config.rs` into
      submodules and update module exports plus test imports.

3) Ensure `WorkspaceConfig` is present and defaults to `/work`.

    - Confirm `WorkspaceConfig` uses `Utf8PathBuf` and update tests if the
      default changes.

4) Add or extend unit tests in `src/config.rs` (or new config submodules) with
   `rstest`.

    - Happy paths: default `AgentConfig` values and TOML deserialisation with
      `agent.mode` and `workspace.base_dir`.
    - Unhappy path: invalid `agent.mode` string emits a parse error.

5) Add BDD scenarios and step definitions.

    - Update `tests/features/configuration.feature` with scenarios for agent
      mode defaults, invalid mode rejection, and workspace override.
    - Update `tests/bdd_config.rs` with `#[given]`/`#[then]` steps and
      `#[scenario]` bindings.

6) Update documentation.

    - Record the final `agent.mode` decision in `docs/podbot-design.md`.
    - Update `docs/users-guide.md` to include `agent.mode` in the TOML example
      and add `PODBOT_AGENT_MODE` to the environment variable table.

7) Mark the roadmap entry as done.

    - Update `docs/podbot-roadmap.md` checkboxes for:
      - "Specify AgentConfig for agent kind and execution mode."
      - "Add WorkspaceConfig for base directory."

8) Run validation with captured logs.

    - Command:

          set -o pipefail
          make check-fmt 2>&1 | tee /tmp/podbot-check-fmt.log

    - Command:

          set -o pipefail
          make lint 2>&1 | tee /tmp/podbot-lint.log

    - Command:

          set -o pipefail
          make test 2>&1 | tee /tmp/podbot-test.log

    - Command:

          set -o pipefail
          make all 2>&1 | tee /tmp/podbot-make-all.log

    - If documentation changed, also run:

          set -o pipefail
          make fmt 2>&1 | tee /tmp/podbot-fmt.log

          set -o pipefail
          make markdownlint 2>&1 | tee /tmp/podbot-markdownlint.log

          set -o pipefail
          make nixie 2>&1 | tee /tmp/podbot-nixie.log

    - Expected: all commands exit 0 with no warnings.

## Validation and Acceptance

Success looks like:

- `make check-fmt`, `make lint`, `make test`, and `make all` all succeed with
  no warnings or lint errors.
- Unit tests prove `AgentMode` and `WorkspaceConfig` defaults and
  deserialisation, and an invalid `agent.mode` string fails parsing.
- Behavioural tests cover the default agent mode, invalid mode rejection, and
  workspace base directory overrides.
- `docs/podbot-design.md` reflects the final `agent.mode` decision and
  `docs/users-guide.md` documents the new configuration key and environment
  variable.
- The Step 1.3 tasks for AgentConfig and WorkspaceConfig are marked as done in
  `docs/podbot-roadmap.md`.

## Idempotence and Recovery

The steps above are safe to re-run. If a command fails, fix the underlying
issue and re-run the same command. If a failure persists after two attempts,
stop and escalate with the log file path.

## Artifacts and Notes

Keep the following log files for review if needed:

- `/tmp/podbot-check-fmt.log`
- `/tmp/podbot-lint.log`
- `/tmp/podbot-test.log`
- `/tmp/podbot-make-all.log`
- `/tmp/podbot-fmt.log`
- `/tmp/podbot-markdownlint.log`
- `/tmp/podbot-nixie.log`

## Interfaces and Dependencies

At completion, configuration should expose the following in
`crate::config` (locations may move if `src/config.rs` is split):

    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
    #[serde(rename_all = "lowercase")]
    pub enum AgentMode {
        #[default]
        Podbot,
    }

    #[derive(Debug, Clone, Deserialize, Serialize)]
    #[serde(default)]
    pub struct AgentConfig {
        /// The type of agent to run.
        pub kind: AgentKind,
        /// Execution mode for the agent (default: podbot).
        pub mode: AgentMode,
    }

    #[derive(Debug, Clone, Deserialize, Serialize)]
    #[serde(default)]
    pub struct WorkspaceConfig {
        /// Base directory for cloned repositories inside the container.
        pub base_dir: Utf8PathBuf,
    }

If additional `AgentMode` variants are required beyond `podbot`, update the
enum and document the decision in `docs/podbot-design.md` before writing code.
