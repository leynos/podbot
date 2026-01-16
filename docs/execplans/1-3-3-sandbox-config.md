# SandboxConfig validation and testing

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: COMPLETE

## Purpose / Big Picture

Complete the `SandboxConfig` implementation by adding comprehensive test coverage
and documentation. The struct already exists in `src/config.rs` with two boolean
fields (`privileged` and `mount_dev_fuse`) and sensible defaults. Unlike
`GitHubConfig`, `SandboxConfig` does not require a `validate()` method because
both fields are booleans with no invalid states—any boolean combination is
semantically valid.

Success is observable when `make test` passes with new test coverage, the
user's guide is updated, and the roadmap task is marked complete.

## Constraints

- Keep all module-level `//!` documentation in place and update it if the
  configuration surface changes.
- Do not add new dependencies without explicit approval.
- Use en-GB-oxendict spelling in documentation and comments.
- Tests must use `rstest` fixtures and `rstest-bdd` scenarios for unit and
  behavioural coverage.
- Use Makefile targets and capture long outputs with `tee` plus
  `set -o pipefail`.
- No `unwrap` or `expect` calls outside test code.

## Tolerances (Exception Triggers)

- Scope: if the change requires edits to more than 6 files or more than 300
  lines of net changes, stop and ask for confirmation.
- Interface: if a public application programming interface (API) outside
  `crate::config` must change, stop and ask for confirmation.
- Dependencies: if a new crate or feature flag is required, stop and ask for
  confirmation.
- Iterations: if tests fail after two fix attempts, stop and ask for
  confirmation with details.

## Risks

- Risk: `src/config.rs` is currently 589 lines; adding tests may push it further
  over the 400-line limit. Severity: low Likelihood: high Mitigation: The file
  already exceeds the limit. Future work should consider splitting the config
  module, but this task extends existing patterns.

- Risk: Adding behaviour-driven development (BDD) scenarios may require new step
  definitions that conflict with existing ones. Severity: low Likelihood: low
  Mitigation: Review existing step definitions in `tests/bdd_config.rs` before
  adding new ones.

## Progress

- [x] (2026-01-16 Coordinated Universal Time (UTC)) Create ExecPlan at
      `docs/execplans/1-3-3-sandbox-config.md`.
- [x] (2026-01-16 UTC) Verify existing unit tests in `src/config.rs` for
      SandboxConfig.
- [x] (2026-01-16 UTC) Add additional unit tests for Tom's Obvious, Minimal
      Language (TOML) serialization/deserialization.
- [x] (2026-01-16 UTC) Add BDD scenarios for sandbox configuration.
- [x] (2026-01-16 UTC) Add step definitions to `tests/bdd_config.rs`.
- [x] (2026-01-16 UTC) Update `docs/users-guide.md` with sandbox configuration
      details.
- [x] (2026-01-16 UTC) Mark the SandboxConfig task as done in
      `docs/podbot-roadmap.md`.
- [x] (2026-01-16 UTC) Run `make check-fmt`, `make lint`, `make test` and
      capture logs.

## Surprises & Discoveries

- Observation: The existing `src/config.rs` already had tests for SandboxConfig
  default values. Evidence: `sandbox_config_default_values` test existed at line
  361. Impact: Extended the existing test suite rather than creating all tests
  from scratch.

- Observation: Clippy lint `needless_raw_string_hashes` was triggered by raw
  string literals using `r#"..."#` when no hashes were needed. Evidence:
  Compilation error in new tests. Impact: Changed to `r"..."` format for TOML
  string literals.

## Decision Log

- Decision: No `validate()` method for SandboxConfig. Rationale: Both fields are
  booleans with no invalid states. Any combination of `privileged` and
  `mount_dev_fuse` is semantically valid. This differs from GitHubConfig where
  fields can be missing or invalid. Date/Author: 2026-01-16 / Terry.

## Outcomes & Retrospective

Successfully completed the SandboxConfig task:

- Added 7 unit tests covering TOML serialization, round-trip, all boolean
  combinations, and default value handling.
- Added 3 BDD scenarios covering dev/fuse disabled, minimal mode, and
  privileged mode with all options.
- Added 4 step definitions and 3 scenario bindings to `tests/bdd_config.rs`.
- Updated `docs/users-guide.md` with comprehensive sandbox configuration
  documentation including security trade-offs.
- All validation gates pass: `make check-fmt`, `make lint`, `make test`.
- Roadmap task marked complete.
- Total: 54 unit tests, 11 BDD scenarios.

## Context and Orientation

Configuration lives in `src/config.rs`, which currently defines CLI arguments
and configuration structs including `SandboxConfig`. Behavioural coverage
resides in `tests/bdd_config.rs` with feature definitions in
`tests/features/configuration.feature`.

The roadmap entry for this task is in `docs/podbot-roadmap.md` under Step 1.3:
"Establish SandboxConfig for privileged mode and /dev/fuse mount options."

### Existing Implementation

`SandboxConfig` already exists (lines 110-128 of `src/config.rs`):

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct SandboxConfig {
    pub privileged: bool,
    pub mount_dev_fuse: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            privileged: false,
            mount_dev_fuse: true,
        }
    }
}
```

Existing unit tests verify default values (line 361-364):

```rust
#[rstest]
fn sandbox_config_default_values(sandbox_config: SandboxConfig) {
    assert!(!sandbox_config.privileged);
    assert!(sandbox_config.mount_dev_fuse);
}
```

Existing BDD coverage includes:

- Scenario: "Default configuration values" - verifies sandbox is not privileged
- Scenario: "Configuration file overrides defaults" - verifies privileged mode

## Plan of Work

Stage A adds unit tests for TOML serialization. Add parameterized tests covering
all four boolean combinations of `privileged` and `mount_dev_fuse`.

Stage B adds BDD scenarios to `tests/features/configuration.feature` covering
sandbox configuration edge cases. Add corresponding step definitions.

Stage C updates documentation. Update `docs/users-guide.md` with details about
the security trade-offs between privileged and minimal modes. Mark the task
complete in `docs/podbot-roadmap.md`.

Stage D runs validation gates with captured logs.

## Concrete Steps

1) Add unit tests for TOML serialization to `src/config.rs`:

    - Test: `sandbox_config_serializes_to_toml` - verify round-trip serialization
    - Test: `sandbox_config_all_combinations` - parameterized test for all four
      boolean combinations

2) Add BDD scenarios to `tests/features/configuration.feature`:

    - Scenario: "Sandbox configuration with dev/fuse disabled"
    - Scenario: "Sandbox configuration in minimal mode"

3) Add step definitions to `tests/bdd_config.rs`:

    - `#[given("a configuration file with dev/fuse mounting disabled")]`
    - `#[given("a configuration file in minimal mode")]`
    - `#[then("dev/fuse mounting is disabled")]`
    - Add scenario bindings for each new scenario

4) Update `docs/users-guide.md`:

    - Add a subsection under Configuration explaining the sandbox settings
    - Document the security trade-offs between privileged and minimal modes
    - Document that `mount_dev_fuse = true` is the default for fuse-overlayfs

5) Update `docs/podbot-roadmap.md`:

    - Mark the checkbox: `[x] Establish SandboxConfig for privileged mode and
      /dev/fuse mount options.`

6) Run validation with captured logs:

    ```bash
    set -o pipefail
    make check-fmt 2>&1 | tee /tmp/podbot-check-fmt.log

    set -o pipefail
    make lint 2>&1 | tee /tmp/podbot-lint.log

    set -o pipefail
    make test 2>&1 | tee /tmp/podbot-test.log
    ```

    Expected: all commands exit 0 with no warnings or lint errors.

## Validation and Acceptance

Success looks like:

- `make check-fmt`, `make lint`, and `make test` all succeed with no warnings.
- Unit tests in `src/config.rs` demonstrate TOML serialization for SandboxConfig.
- Behavioural tests in `tests/bdd_config.rs` pass with new scenarios covering
  sandbox configuration.
- `docs/users-guide.md` includes documentation about sandbox settings.
- The SandboxConfig task in `docs/podbot-roadmap.md` is marked as done.

## Idempotence and Recovery

The steps above are safe to rerun. If a command fails, fix the underlying issue
and re-run the same command. If a test or lint failure is not understood after
two attempts, stop and escalate with the captured log file path.

## Artefacts and Notes

Keep the following log files for review if needed:

- `/tmp/podbot-check-fmt.log`
- `/tmp/podbot-lint.log`
- `/tmp/podbot-test.log`

## Interfaces and Dependencies

At completion, `SandboxConfig` should expose:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct SandboxConfig {
    /// Run the container in privileged mode (less secure but more compatible).
    pub privileged: bool,

    /// Mount /dev/fuse in the container for fuse-overlayfs support.
    pub mount_dev_fuse: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            privileged: false,
            mount_dev_fuse: true,
        }
    }
}
```

No validation method is required because all boolean combinations are valid.

## Files to Modify

- `docs/execplans/1-3-3-sandbox-config.md` - Create this execplan document
- `src/config.rs` - Add unit tests (lines ~588+)
- `tests/bdd_config.rs` - Add step definitions and scenario bindings
- `tests/features/configuration.feature` - Add new BDD scenarios
- `docs/users-guide.md` - Add sandbox configuration documentation
- `docs/podbot-roadmap.md` - Mark task complete
