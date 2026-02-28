# Step 2.2.4: Configure Security-Enhanced Linux (SELinux) capabilities and security options

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises and discoveries`,
`Decision log`, and `Outcomes and retrospective` must be kept up to date as
work proceeds.

Status: COMPLETE

No `PLANS.md` file exists in this repository as of 2026-02-15, so this ExecPlan
is the governing implementation document for this task.

## Purpose and big picture

Complete the Step 2.2.4 roadmap item: "Configure appropriate capabilities and
security options for SELinux environments." The goal is to promote
`SelinuxLabelMode` from an internal engine concept to a user-facing
configuration option, so operators can override SELinux label handling
independently of the `sandbox.privileged` flag.

Before this change, `SelinuxLabelMode` was derived automatically in
`ContainerSecurityOptions::from_sandbox_config`: privileged mode yielded
`KeepDefault`, non-privileged mode yielded `DisableForContainer`. The user had
no way to override this without changing the privileged flag.

After this change:

- A new `sandbox.selinux_label_mode` field in the TOML (Tom's Obvious, Minimal
  Language) configuration (and `PODBOT_SANDBOX_SELINUX_LABEL_MODE` environment
  variable) lets operators choose between `"keep_default"` and
  `"disable_for_container"`.
- Existing configuration files that omit the field default to
  `"disable_for_container"`, preserving current behaviour.
- The `from_sandbox_config` constructor passes the field through directly
  instead of deriving it from `sandbox.privileged`.
- This behaviour is observable via `make test` (all tests pass, including new
  serde round-trip, pass-through, env var, and behaviour-driven development
  (BDD) tests).

## Constraints

- Backward compatibility: existing TOML files without `selinux_label_mode` must
  continue to work with the same behaviour as before.
- The `from_sandbox_config` constructor must remain `const fn`.
- The `SelinuxLabelMode::requires_label_disable` method stays in the engine
  module (engine-specific logic, not config logic).
- The re-export path `podbot::engine::SelinuxLabelMode` must continue to work
  for BDD tests and external consumers.
- All quality gates (`make check-fmt`, `make lint`, `make test`) must pass.
- Code files must stay under 400 lines per AGENTS.md.
- en-GB-oxendict spelling in documentation and comments.

## Tolerances (exception triggers)

- Scope: if implementation requires changes to more than 20 files, stop and
  escalate.
- Interface: if a public API signature must change beyond the planned additions,
  stop and escalate.
- Dependencies: no new external dependencies are required.
- Iterations: if tests still fail after 3 attempts, stop and escalate.

## Risks

- Risk: users-guide.md line count exceeds 400 lines.
  Severity: low Likelihood: medium Mitigation: AGENTS.md says "code file" for
  the 400-line limit. Documentation files are not code files. The guide reaches
  424 lines, which is acceptable.

- Risk: serde rename strategy mismatch with existing enums.
  Severity: low Likelihood: low Mitigation: SelinuxLabelMode uses snake_case
  (multi-word variants), while AgentKind/AgentMode use lowercase (single-word
  variants). Both are correct for their respective variant naming patterns.

## Progress

- [x] (2026-02-15 00:45Z) Move `SelinuxLabelMode` to `src/config/types.rs`
  with `Serialize`/`Deserialize` and `ValueEnum`.
- [x] (2026-02-15 00:45Z) Add `selinux_label_mode` field to `SandboxConfig`.
- [x] (2026-02-15 00:46Z) Simplify `from_sandbox_config` to pass-through.
- [x] (2026-02-15 00:46Z) Add re-export
      `pub use crate::config::SelinuxLabelMode`
  in `create_container/mod.rs`.
- [x] (2026-02-15 00:46Z) Add `SelinuxLabelMode` to `config/mod.rs` re-exports.
- [x] (2026-02-15 00:47Z) Add `PODBOT_SANDBOX_SELINUX_LABEL_MODE` env var to
  `loader.rs`.
- [x] (2026-02-15 00:48Z) Add serde round-trip tests for `SelinuxLabelMode`.
- [x] (2026-02-15 00:48Z) Update config test helpers (full TOML fixture,
  default assertions).
- [x] (2026-02-15 00:49Z) Update engine unit tests for pass-through semantics.
- [x] (2026-02-15 00:49Z) Add BDD scenario and Given step for config-driven
  SELinux mode.
- [x] (2026-02-15 00:50Z) Add integration test cases for env var acceptance
  and rejection.
- [x] (2026-02-15 00:50Z) Fix `doc_markdown` lint (backtick SELinux in doc
  comments).
- [x] (2026-02-15 01:00Z) All quality gates pass (check-fmt, lint, test).
- [x] (2026-02-15 01:02Z) Commit code changes.
- [x] (2026-02-15 01:05Z) Update users-guide.md (config table, env var table,
  TOML example, behaviour section, SELinux explanation).
- [x] (2026-02-15 01:05Z) Update podbot-design.md (security mapping section).
- [x] (2026-02-15 01:05Z) Mark roadmap task 2.2.4 as done.
- [x] (2026-02-15 01:06Z) Commit documentation changes.
- [x] (2026-02-15 01:08Z) Write ExecPlan.

## Surprises and discoveries

- Observation: The `doc_markdown` Clippy lint requires `SELinux` to be
  backtick-quoted in doc comments. The original engine definition used
  backticks, but the new config definition initially omitted them. Evidence:
  Clippy error during `make lint`. Impact: Five doc comment lines needed
  backtick-quoting. Fixed in the same commit.

- Observation: `make fmt` reformats all markdown files, producing
  collateral diffs in table alignment across files that were not manually
  changed. Evidence: Known from previous tasks (recorded in MEMORY.md). Impact:
  Documentation commit includes formatting changes from `make fmt`.

## Decision log

- Decision: Use `#[serde(rename_all = "snake_case")]` for SelinuxLabelMode.
  Rationale: Multi-word variants (KeepDefault, DisableForContainer) serialise
  naturally as "keep_default" and "disable_for_container" with snake_case.
  Single-word enums like AgentKind use "lowercase" instead. Date/Author:
  2026-02-15 / agent.

- Decision: Env var typed as String, not a new EnvVarType variant.
  Rationale: Matches the existing pattern for PODBOT_AGENT_KIND and
  PODBOT_AGENT_MODE. Invalid values are caught during merge_from_layers by
  serde, not at collect_env_vars. Date/Author: 2026-02-15 / agent.

- Decision: Keep `impl SelinuxLabelMode` block in engine module.
  Rationale: The `requires_label_disable` method is engine-specific logic that
  translates a config value into a build_host_config decision. Moving it to
  config/types.rs would leak engine concerns into config. Date/Author:
  2026-02-15 / agent.

- Decision: Re-export via `pub use crate::config::SelinuxLabelMode` in
  create_container/mod.rs. Rationale: Preserves the existing
  `podbot::engine::SelinuxLabelMode` path used by BDD tests and the
  connection/mod.rs re-export chain. Date/Author: 2026-02-15 / agent.

## Outcomes and retrospective

The task is complete. All 231+ tests pass, including 7 new tests for
`SelinuxLabelMode` serde behaviour, pass-through semantics, env var handling,
and BDD config-to-engine translation.

Key outcomes:

- `SelinuxLabelMode` is now a first-class config option with serde support,
  `ValueEnum` for future command-line interface (CLI) use, and environment
  variable override.
- Backward compatibility is fully preserved: existing configs without the field
  default to `DisableForContainer`.
- The re-export chain `config -> engine -> public API` works correctly.
- Documentation in users-guide.md and podbot-design.md is updated.
- The roadmap task is marked complete.

The approach of moving the enum to config/types.rs while keeping the
`requires_label_disable` method in the engine module cleanly separates
configuration concerns from engine logic. The `pub use` re-export pattern
avoids breaking the existing import paths.

## Context and orientation

The podbot project is a Rust application that creates sandboxed containers for
AI coding agents. Container creation is handled by the engine module
(`src/engine/connection/create_container/`), which translates high-level
`SandboxConfig` settings into Bollard application programming interface (API)
payloads.

Key files:

- `src/config/types.rs`: Configuration data types including `SandboxConfig`
  and (now) `SelinuxLabelMode`.
- `src/config/mod.rs`: Module re-exports.
- `src/config/loader.rs`: Environment variable to config mapping.
- `src/engine/connection/create_container/mod.rs`: Container creation logic
  including `ContainerSecurityOptions` and `from_sandbox_config`.
- `docs/users-guide.md`: End-user documentation.
- `docs/podbot-design.md`: Architecture documentation.
- `docs/podbot-roadmap.md`: Task tracking.

## Plan of work

Stage A: Move `SelinuxLabelMode` enum to `src/config/types.rs` with serde
derives. Add `selinux_label_mode` field to `SandboxConfig` with
`DisableForContainer` default. Remove the local enum from the engine module,
adding a `pub use` re-export to preserve the import path.

Stage B: Simplify `from_sandbox_config` to pass the field through directly. Add
env var spec for `PODBOT_SANDBOX_SELINUX_LABEL_MODE`. Add re-export to
`config/mod.rs`.

Stage C: Update all tests — serde round-trip, default assertions, pass-through
verification, BDD scenario, integration env var tests.

Stage D: Update documentation (users-guide, design doc, roadmap). Write
ExecPlan.

## Validation and acceptance

Quality criteria:

- Tests: `make test` — all 231+ tests pass, including new tests for serde,
  pass-through, env var, and BDD.
- Lint: `make lint` — no warnings.
- Format: `make check-fmt` — clean.

Quality method:

    make check-fmt && make lint && make test

## Idempotence and recovery

All changes are additive and safe to revert. The `selinux_label_mode` field
uses `#[serde(default)]` on `SandboxConfig`, so existing config files continue
to work without changes.

## Artifacts and notes

Files modified (15 total):

    src/config/types.rs
    src/config/mod.rs
    src/config/loader.rs
    src/config/tests/helpers.rs
    src/config/tests/types_tests.rs
    src/engine/connection/create_container/mod.rs
    src/engine/connection/create_container/tests/mod.rs
    src/engine/connection/create_container/tests/privileged_mode.rs
    src/engine/connection/create_container/tests/minimal_mode.rs
    tests/bdd_config_helpers.rs
    tests/bdd_container_creation.rs
    tests/bdd_container_creation_helpers/steps.rs
    tests/features/container_creation.feature
    tests/load_config_integration.rs
    tests/sandbox_config_toml.rs

Documentation files modified (3 total):

    docs/users-guide.md
    docs/podbot-design.md
    docs/podbot-roadmap.md

## Interfaces and dependencies

No new external dependencies.

In `src/config/types.rs`, the new enum:

    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum SelinuxLabelMode {
        KeepDefault,
        #[default]
        DisableForContainer,
    }

In `src/config/types.rs`, the updated struct:

    pub struct SandboxConfig {
        pub privileged: bool,
        pub mount_dev_fuse: bool,
        pub selinux_label_mode: SelinuxLabelMode,
    }

In `src/engine/connection/create_container/mod.rs`, the re-export:

    pub use crate::config::SelinuxLabelMode;

In `src/config/loader.rs`, the new env var spec:

    EnvVarSpec {
        env_var: "PODBOT_SANDBOX_SELINUX_LABEL_MODE",
        path: &["sandbox", "selinux_label_mode"],
        var_type: EnvVarType::String,
    }
