# Define AppConfig root configuration

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: COMPLETE

PLANS.md is not present in the repository.

## Purpose / Big Picture

Define the root `AppConfig` structure that represents podbot's configuration in
one place, with clear defaults and nested sections that match the design and
user documentation. Success is observable when `cargo test` passes with new
unit and behavioural coverage and the configuration struct can be deserialised
from a sample config file while preserving defaults for missing fields.

## Constraints

- Keep all module-level `//!` documentation in place and update it if the
  configuration surface changes.
- Use `camino::Utf8PathBuf` for path fields and avoid `std::path::PathBuf` in
  new configuration types.
- Do not add new dependencies without explicit approval.
- Ensure no file exceeds 400 lines; if a file would grow past this, refactor
  into smaller modules.
- Use en-GB-oxendict spelling in documentation and comments.
- Tests must use `rstest` fixtures and `rstest-bdd` scenarios for unit and
  behavioural coverage.
- Use Makefile targets and capture long outputs with `tee` plus
  `set -o pipefail`.

## Tolerances (Exception Triggers)

- Scope: if the change requires edits to more than 6 files or more than 300
  lines of net changes, stop and ask for confirmation.
- Interface: if a public API outside `crate::config` must change, stop and ask
  for confirmation.
- Dependencies: if a new crate or feature flag is required, stop and ask for
  confirmation.
- Iterations: if tests fail after two fix attempts, stop and ask for
  confirmation with details.
- Ambiguity: if documentation disagrees on the default config file path (for
  example `~/.config/podbot/config.toml` vs another location), stop and confirm
  which is authoritative before proceeding.

## Risks

    - Risk: `AppConfig` already exists with fields that differ from the design
      document.
      Severity: medium
      Likelihood: medium
      Mitigation: inspect `src/config.rs` first and adjust only what is needed
      to align with `docs/podbot-design.md` and `docs/users-guide.md`.

    - Risk: existing tests and BDD scenarios assume defaults that conflict with
      the intended defaults.
      Severity: medium
      Likelihood: low
      Mitigation: update tests and feature files in the same commit and ensure
      defaults are described once in the design and user guide.

    - Risk: `OrthoConfig` derive requirements introduce extra attributes or
      defaults not accounted for.
      Severity: low
      Likelihood: medium
      Mitigation: follow `docs/ortho-config-users-guide.md` and keep changes
      minimal, deferring layering logic to the next roadmap tasks.

## Progress

    - [x] (2026-01-14 09:31Z) Inspect existing configuration structs and
          documentation for alignment.
    - [x] (2026-01-14 13:20Z) Implement or refine `AppConfig` and nested config
          defaults in `src/config.rs`.
    - [x] (2026-01-14 13:20Z) Add/update unit tests (`rstest`) and behavioural
          tests (`rstest-bdd`) for happy, unhappy, and edge cases.
    - [x] (2026-01-14 13:20Z) Update `docs/podbot-design.md` with any design
          decisions and `docs/users-guide.md` with user-facing behaviour.
    - [x] (2026-01-14 13:20Z) Mark Step 1.3 task as done in
          `docs/podbot-roadmap.md`.
    - [x] (2026-01-14 13:20Z) Run formatting, linting, tests, and full
          validation (`make all`) with logs captured.

## Surprises & Discoveries

    - Observation: The design documentation used the legacy project naming.
      Evidence: `docs/podbot-design.md` "Configuration" and CLI sections.
      Impact: Updated documentation and examples to `podbot` once confirmed.
    - Observation: `cargo doc` emits a warning about the renamed
      `missing_crate_level_docs` lint.
      Evidence: `make lint` and `make all` output includes the rename warning.
      Impact: No functional impact, but documentation builds remain noisy until
      the lint name is updated.

## Decision Log

    - Decision: Replace legacy project-name references with `podbot` for paths,
      directories, and CLI examples.
      Rationale: User confirmed the canonical naming and default path.
      Date/Author: 2026-01-14 / Codex

## Outcomes & Retrospective

Completed AppConfig validation and documentation alignment, added unhappy-path
coverage, and validated with formatting, lint, and test gates. The remaining
noise is a pre-existing rustdoc lint rename warning.

## Context and Orientation

Configuration lives in `src/config.rs`, which currently defines CLI arguments
and configuration structs. Behavioural coverage resides in
`tests/bdd_config.rs` with feature definitions in
`tests/features/configuration.feature`. Design intent is captured in
`docs/podbot-design.md`, while user-facing configuration guidance sits in
`docs/users-guide.md`. The roadmap entry for this task is in
`docs/podbot-roadmap.md` under Step 1.3.

`AppConfig` should be the root configuration container for the application. It
must aggregate nested configuration structs (GitHub, sandbox, agent, workspace,
credentials) with defaults and optional values that make sense when fields are
missing. The struct should be serialisable/deserialisable with `serde` and
remain compatible with the eventual OrthoConfig layering flow described in
`docs/ortho-config-users-guide.md`.

## Plan of Work

Stage A covers inspection and alignment. Read `src/config.rs`,
`docs/podbot-design.md`, and `docs/users-guide.md` to confirm the expected
fields, defaults, and configuration file path. If the existing `AppConfig`
matches the design, keep changes minimal and focus on tests and documentation.
If the design and docs conflict, stop and confirm the authoritative source
before proceeding.

Stage B focuses on defining or refining `AppConfig`. Ensure the root struct
contains optional `engine_socket` and `image` fields, plus nested configuration
structs for GitHub, sandbox, agent, workspace, and credentials. Apply `Default`
and `serde` traits to the root and nested structs so missing fields fall back
cleanly. Add or adjust module-level documentation to keep the public-facing
example accurate.

Stage C adds validation coverage. Unit tests should use `rstest` fixtures to
cover default values, TOML deserialisation, and an unhappy path such as an
invalid agent kind or missing nested sections. Behavioural tests should use
`rstest-bdd` to assert the user-facing defaults and an unhappy or edge case
scenario, such as a malformed configuration entry leading to a reported error
or a missing optional section remaining `None`. Keep tests isolated and avoid
mutating global environment variables; if environment interaction is needed,
use dependency injection per
`docs/reliable-testing-in-rust-via-dependency-injection.md`.

Stage D updates documentation and the roadmap. Record any design decisions in
`docs/podbot-design.md`, update `docs/users-guide.md` for user-visible changes,
then mark the Step 1.3 "Define AppConfig" task as done in
`docs/podbot-roadmap.md`.

Stage E runs formatting, linting, and test gates. Use Makefile targets and the
`tee` + `set -o pipefail` pattern to capture logs for review.

## Concrete Steps

1) Inspect current configuration definitions and documentation.

    - Command:

          rg -n "AppConfig|GitHubConfig|SandboxConfig|AgentConfig|WorkspaceConfig|CredsConfig" \
            src/config.rs
    - Command: `sed -n '1,220p' src/config.rs`
    - Command: `sed -n '1,200p' docs/podbot-design.md`
    - Command: `sed -n '1,200p' docs/users-guide.md`

2) Update `src/config.rs` to define or adjust `AppConfig` and its defaults.

    - Ensure `AppConfig` derives `Debug`, `Clone`, `Default`, `Serialize`, and
      `Deserialize`.
    - Ensure nested config structs derive `Default` and `serde` traits with
      `#[serde(default)]` so missing fields default correctly.

3) Add or update unit tests in `src/config.rs` using `rstest`.

    - Happy path: deserialise a TOML snippet into `AppConfig` and assert values.
    - Edge path: deserialise TOML missing nested sections and assert defaults.
    - Unhappy path: attempt to deserialise an invalid agent kind and assert the
      error message or failure mode.

4) Extend behavioural tests in `tests/bdd_config.rs` and
   `tests/features/configuration.feature`.

    - Add a scenario that captures an unhappy or edge case (for example, a
      malformed agent kind or missing optional section) and assert the outcome.

5) Update documentation.

    - Record any configuration structure or default decisions in
      `docs/podbot-design.md`.
    - Update `docs/users-guide.md` to reflect any user-facing changes or
      clarified defaults.

6) Mark the roadmap entry as done.

    - Update the checkbox for "Define AppConfig as the root configuration
      structure" in `docs/podbot-roadmap.md`.

7) Run validation with captured logs.

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

    - Expected: all commands exit 0 with no warnings or lint errors.

## Validation and Acceptance

Success looks like:

- `make check-fmt`, `make lint`, `make test`, and `make all` all succeed with
  no warnings.
- Unit tests in `src/config.rs` demonstrate defaults, deserialisation, and an
  unhappy path for invalid configuration input.
- Behavioural tests in `tests/bdd_config.rs` pass and include at least one
  unhappy or edge scenario.
- `docs/podbot-design.md` reflects any decisions taken, and
  `docs/users-guide.md` reflects user-facing behaviour.
- The Step 1.3 task in `docs/podbot-roadmap.md` is marked as done.

## Idempotence and Recovery

The steps above are safe to rerun. If a command fails, fix the underlying issue
and re-run the same command. If a test or lint failure is not understood after
two attempts, stop and escalate with the captured log file path.

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

At completion, the root configuration interface should be available at
`crate::config::AppConfig` and follow this shape:

    pub struct AppConfig {
        pub engine_socket: Option<String>,
        pub image: Option<String>,
        pub github: GitHubConfig,
        pub sandbox: SandboxConfig,
        pub agent: AgentConfig,
        pub workspace: WorkspaceConfig,
        pub creds: CredsConfig,
    }

Nested types should live in `src/config.rs` and remain serialisable with
`serde`. Any path fields should use `camino::Utf8PathBuf`. If this task
requires `OrthoConfig` derive attributes, they should be minimal and compatible
with the layered precedence described in `docs/ortho-config-users-guide.md`.

## Revision note (required when editing an ExecPlan)

Updated status to COMPLETE, marked all progress steps done, and recorded the
lint warning observation plus the final outcomes summary.
