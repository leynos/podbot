# Step 1.4: Hosting schema migration and compatibility matrix

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: COMPLETE

PLANS.md is not present in the repository root, so this plan stands alone.

## Purpose / Big Picture

Podbot's current configuration model only supports the legacy interactive
shape: `agent.mode = "podbot"` and a clone-oriented workspace rooted at
`workspace.base_dir`. Step 1.4 extends that schema so hosted app-server
configurations can be represented, loaded, normalized, and validated as the
project moves from the initial interactive-only shape toward the hosted design.

Success is observable in three ways. First, the expanded configuration schema
loads deterministically with explicit defaults for new fields such as
`workspace.source`, `agent.command`, `agent.args`, `agent.env_allowlist`, and
the new MCP hosting defaults. Second, the current in-repo configuration
variants continue to parse and validate according to the newly chosen schema
rules, even if those rules differ from earlier draft assumptions. Third,
invalid combinations of subcommand, `agent.kind`, `agent.mode`, and
`workspace.source` fail with semantic errors that tell the operator what is
wrong and what to do next.

This plan now records the implemented Step 1.4 delivery. The roadmap entry in
`docs/podbot-roadmap.md` was updated after the implementation, tests,
documentation, and quality gates passed.

## Constraints

- Optimize for a coherent hosted-era schema rather than preserving speculative
  pre-release compatibility guarantees. Determinism matters; frozen legacy
  behaviour does not.
- Keep `AppConfig` library-facing and Clap-free. CLI-specific parsing must stay
  in `src/cli/mod.rs`.
- Keep semantic validation in library code, not spread across ad hoc CLI
  branches. The design document explicitly wants
  `(agent.kind, agent.mode, workspace.source)` handling centralized.
- Use `camino::Utf8PathBuf` and capabilities-oriented filesystem APIs for new
  path-bearing configuration fields.
- Use `rstest` for unit coverage and `rstest-bdd` v0.5.0 for behavioural
  coverage. The repository already pins `rstest-bdd = "0.5.0"`.
- Avoid direct process-environment mutation in tests. Continue using
  `mockable::Env` and loader injection patterns.
- Keep all Rust modules under the 400-line limit. Split `src/config/` further
  if adding hosting types or validation pushes a file over the limit.
- Use en-GB-oxendict spelling in comments and documentation.
- Run all required quality gates before considering the feature complete:
  `make check-fmt`, `make lint`, and `make test`. If Markdown changes are made,
  also run `make fmt`, `make markdownlint`, and `make nixie`.

## Tolerances

- Scope: if the implementation requires changes outside configuration, CLI,
  tests, and the explicitly named documentation files, stop and reassess the
  step boundary before continuing.
- Interface: if introducing hosted-schema validation requires a new public API
  beyond `crate::config` and the CLI adapter scaffolding, stop and document the
  proposed surface before proceeding.
- Dependencies: if the feature cannot be delivered with the current dependency
  set, stop and escalate instead of adding crates opportunistically.
- CLI coupling: if enforcing subcommand legality requires fully implementing
  `podbot host` in this step rather than adding minimal scaffolding or shared
  validation inputs, stop and confirm the intended scope split with the user.
- Validation churn: if the first two iterations of the config-validation design
  still produce unclear or unstable error messages, stop and record the
  competing approaches in the `Decision Log`.

## Risks

- Risk: the roadmap and design require validation against `podbot host`, but
  the current CLI only exposes `run`, `token-daemon`, `ps`, `stop`, and `exec`.
  Severity: high. Likelihood: high. Mitigation: treat the missing `host`
  subcommand as an explicit Stage B deliverable or introduce a minimal
  subcommand intent enum shared by CLI and library validation.
- Risk: `src/config/types.rs`, `src/config/env_vars.rs`, and existing
  behaviour-driven development (BDD) helpers may exceed the 400-line limit once
  hosting-era fields and cases are added. Severity: medium. Likelihood: high.
  Mitigation: pre-plan extraction into submodules such as `agent.rs`,
  `workspace.rs`, `hosting.rs`, or `validation.rs`.
- Risk: the schema is still fluid enough that early assumptions from Step 1.3
  or from draft docs may no longer be the right defaults. Severity: medium.
  Likelihood: high. Mitigation: record the chosen defaults explicitly in the
  design doc and update the configuration matrix tests to match the new source
  of truth rather than preserving earlier draft behaviour.
- Risk: `rstest-bdd` feature files are compile-time inputs, so changed feature
  text can appear stale under incremental builds. Severity: low. Likelihood:
  medium. Mitigation: document `cargo clean -p podbot` as the recovery step if
  scenario text appears out of sync.
- Risk: public enums expanded for hosting mode may require wildcard match arms
  in integration tests if they become `#[non_exhaustive]` later. Severity: low.
  Likelihood: medium. Mitigation: keep behavioural assertions resilient and
  avoid exhaustive matching in external-style tests unless required.

## Progress

- [x] (2026-03-29 00:00Z) Reviewed roadmap, design, MCP hosting design, current
  config types, loader, CLI, unit tests, and BDD coverage.
- [x] (2026-03-29 00:00Z) Collected planning input from agent team members for
  design intent and codebase seams.
- [x] (2026-03-29 02:00Z) Draft approved by user.
- [x] (2026-03-29 03:00Z) Extend configuration schema and defaults for
  hosting-era fields and MCP hosting defaults.
- [x] (2026-03-29 03:00Z) Implement deterministic migration rules and
  centralized semantic validation.
- [x] (2026-03-29 03:00Z) Add rstest unit coverage and rstest-bdd behavioural
  coverage for the compatibility matrix.
- [x] (2026-03-29 03:00Z) Update design and user documentation.
- [x] (2026-03-29 03:00Z) Mark Step 1.4 complete in `docs/podbot-roadmap.md`
  after all validations pass.
- [x] (2026-03-29 03:00Z) Run and capture validation commands.

## Surprises & Discoveries

- The current configuration surface is still the Step 1.3 shape:
  `AgentConfig` only contains `kind` and `mode`, and `WorkspaceConfig` only
  contains `base_dir`.
- Semantic config validation is almost empty today. `AppConfig::post_merge(...)`
  is still a placeholder, while only `GitHubConfig::validate()` performs
  explicit field checks.
- The design already defines the hosting-era schema and examples in
  `docs/podbot-design.md`, including
  `workspace.source = "github_clone" | "host_mount"`,
  `agent.mode = "podbot" | "codex_app_server" | "acp"`,
  `agent.kind = "claude" | "codex" | "custom"`, and `agent.command` /
  `agent.args` for custom hosted agents.
- The roadmap Step 1.4 task list includes schema fields
  (`workspace.source`, `workspace.host_path`, `workspace.container_path`,
  `agent.command`, `agent.args`, `agent.env_allowlist`) that were omitted from
  the user prompt but remain part of the authoritative step definition.
- The repository already pins `rstest-bdd` and `rstest-bdd-macros` at `0.5.0`,
  so no dependency update is needed for the requested behavioural coverage.
- Adding hosted-era fields pushed the config surface past the existing
  `src/config/types.rs` shape, so the implementation split config into
  `agent.rs`, `workspace.rs`, `hosting.rs`, and `validation.rs` to stay below
  the repository's 400-line limit.
- Command-specific legality works best as loader input rather than a pure
  post-merge hook. The implementation added `CommandIntent` to
  `ConfigLoadOptions` so both the CLI and library embedders can request the
  same semantic checks.

## Decision Log

- Decision: plan Step 1.4 around the full roadmap scope, not only the subset in
  the user prompt. Rationale: `docs/podbot-roadmap.md` is the source of truth
  for the step, and the omitted schema fields are required to make the hosting
  migration coherent. Date/Author: 2026-03-29 / Codex.
- Decision: keep the plan in DRAFT state and do not mark the roadmap entry
  complete yet. Rationale: the execplans skill requires an approval gate before
  implementation, and the feature has not been delivered. Date/Author:
  2026-03-29 / Codex.
- Decision: treat subcommand legality as a first-class validation input rather
  than a CLI-only concern. Rationale: the design document requires centralized
  legality checks across `run` and `host` flows, and the library should own the
  semantic rules. Date/Author: 2026-03-29 / Codex.
- Decision: do not treat backward compatibility for unreleased configurations
  as a hard requirement for this step. Rationale: the user explicitly clarified
  that Podbot is still in an early build stage, so Step 1.4 should optimize for
  a clean hosted-era schema rather than preserving every pre-release shape.
  Date/Author: 2026-03-29 / Codex.
- Decision: treat `agent.args` as optional with a deterministic default of
  `[]`, while still requiring `agent.command` for `agent.kind = "custom"`.
  Rationale: a launcher command is the essential invariant, but forcing at
  least one argument would reject valid commands whose protocol mode is implied
  by the executable itself. Date/Author: 2026-03-29 / Codex.
- Decision: use a concrete `[mcp]` config section with
  `bind_strategy`, `idle_timeout_secs`, `max_message_size_bytes`,
  `auth_token_policy`, and `allowed_origin_policy`. Rationale: the roadmap
  required explicit defaults now, and these names map directly onto the design
  document's policy categories. Date/Author: 2026-03-29 / Codex.

## Outcomes & Retrospective

Step 1.4 is now implemented. The configuration model supports hosted-era agent
and workspace fields, a concrete `[mcp]` defaults section, shared semantic
validation through `CommandIntent`, a minimal `podbot host` scaffold, and
compatibility coverage across unit, integration, and BDD tests. Validation
evidence was captured with `make fmt`, `make markdownlint`, `make nixie`,
`make check-fmt`, `make lint`, and `make test`, all of which passed on
2026-03-29.

## Context and Orientation

The configuration entry points live in four places:

- `src/config/types.rs` defines `AppConfig`, `AgentConfig`, `WorkspaceConfig`,
  and the existing enums.
- `src/config/env_vars.rs` maps `PODBOT_*` variables into the JSON layer used
  by `MergeComposer`.
- `src/config/loader.rs` composes defaults, file input, environment variables,
  and host overrides into `AppConfig`.
- `src/config/load_options.rs` defines the library-facing loader inputs used by
  embedders and by the CLI adapter.

The CLI surface lives in `src/cli/mod.rs`. A `Commands::Host` variant was added
to expose the `podbot host` subcommand, converting CLI flags into
`ConfigLoadOptions`. The main entry point in `src/main.rs` now dispatches
`host`, `run`, `token-daemon`, `ps`, `stop`, and `exec`.

The relevant existing tests are:

- `src/config/tests/types_tests.rs` for defaults and TOML
  serialize/deserialize coverage.
- `src/config/tests/validation.rs` for `GitHubConfig::validate()`.
- `tests/load_config_integration.rs` for end-to-end loader precedence and
  typed-environment handling.
- `tests/bdd_config.rs` and `tests/features/configuration.feature` for config
  BDD scenarios.
- `tests/bdd_config_loader.rs` and `tests/features/config_loader.feature` for
  library-facing loader behaviour.

The design sources to keep open during implementation are:

- `docs/podbot-roadmap.md`
- `docs/podbot-design.md`
- `docs/mcp-server-hosting-design.md`
- `docs/users-guide.md`
- `docs/ortho-config-users-guide.md`
- `docs/rust-testing-with-rstest-fixtures.md`
- `docs/rstest-bdd-users-guide.md`
- `docs/reliable-testing-in-rust-via-dependency-injection.md`
- `docs/rust-doctest-dry-guide.md`
- `docs/complexity-antipatterns-and-refactoring-strategies.md`

## Plan of Work

Stage A is to freeze the semantic target. Re-read the Step 1.4 section in
`docs/podbot-roadmap.md` together with the hosting examples and migration notes
in `docs/podbot-design.md`. Extract the exact target schema and validation
matrix before touching code. This stage ends when the implementation has a
written checklist of new fields, new enum values, defaults, and illegal
combinations.

Stage B is to extend the configuration schema and loader inputs. Add the
hosting-era fields that the roadmap requires: `workspace.source`,
`workspace.host_path`, `workspace.container_path`, `agent.command`,
`agent.args`, and `agent.env_allowlist`. Add the hosting defaults for MCP
exposure in a new configuration section or sub-structure consistent with the
design, covering bind strategy, idle timeout, maximum message size, auth token
policy, and allowed-origin policy. Expand execution-mode support so `AgentMode`
includes `codex_app_server` and `acp`, with defaults chosen for the new schema
rather than for backward compatibility. If needed, add `AgentKind::Custom` to
match the design examples.

Stage C is to define deterministic normalization and validation rules. Use the
post-merge phase or a dedicated validation layer owned by `crate::config` to
normalize missing hosting-era fields into the newly chosen defaults and to
reject illegal combinations with `ConfigError::InvalidValue` or another
semantic config error. Validation should cover at least these rules:

1. `podbot run` allows only `agent.mode = "podbot"`.
2. `podbot host` allows only hosted modes such as `codex_app_server` and
   `acp`.
3. `agent.kind = "custom"` requires `agent.command`, while `agent.args`
   remains optional and defaults to `[]`.
4. Built-in agent kinds must reject stray custom-command fields if that policy
   is chosen.
5. `workspace.source = "host_mount"` requires `workspace.host_path`;
   `workspace.container_path` defaults to `"/workspace"` when omitted.
6. `workspace.source = "github_clone"` must preserve `workspace.base_dir`
   semantics and should reject host-mount-only fields if that policy is chosen.

Stage D is to connect validation to the CLI and library entry points. Because
the current CLI lacks `podbot host`, decide whether this step introduces a
minimal `host` subcommand scaffold now or a shared subcommand-intent type that
can be passed into config validation from the existing CLI and later host
implementation. Keep the semantic legality checks in shared code so both CLI
and embedders receive the same rules and error messages.

Stage E is to add the configuration matrix tests. Unit coverage should use
`rstest` fixtures and parameterized cases for interactive-only defaults,
hosting-era defaults, mixed-layer overrides, invalid enum values, illegal field
combinations, and normalization behaviour. Behavioural coverage should use
`rstest-bdd` feature scenarios covering both happy and unhappy paths, including
at least one scenario proving that the new interactive default path is still
valid and at least one scenario proving that an invalid hosting combination
emits an actionable semantic error. Prefer scenario state and injected mock
environment patterns over live environment mutation.

Stage F is to update documentation. Record the final migration rules and
validation decisions in `docs/podbot-design.md`. Update `docs/users-guide.md`
to document new configuration keys, environment variables, and the user-visible
difference between `run` and `host` modes once the validation behaviour is in
place. If the MCP hosting defaults become concrete enough to matter to
operators, update `docs/mcp-server-hosting-design.md` as well. Only after the
feature is implemented and validated should `docs/podbot-roadmap.md` mark Step
1.4 as done.

Stage G is validation and evidence capture. Run the required Rust quality gates
with `tee` and `set -o pipefail`, then run the documentation gates because this
step necessarily changes Markdown. Capture short transcripts showing passing
tests and the new compatibility scenarios.

## Concrete Steps

1. Record the target schema in working notes before editing code.

   Confirm the design examples and migration bullets in:

   ```markdown
   docs/podbot-roadmap.md
   docs/podbot-design.md
   docs/mcp-server-hosting-design.md
   ```

2. Refactor `src/config/types.rs` if needed before adding new fields.

   Likely extractions:

   - `src/config/agent.rs` for `AgentKind`, `AgentMode`, and `AgentConfig`
   - `src/config/workspace.rs` for `WorkspaceConfig` and `WorkspaceSource`
   - `src/config/hosting.rs` for MCP hosting defaults
   - `src/config/validation.rs` for semantic normalization and legality checks

3. Extend configuration enums and structs.

   Minimum target additions:

   - `AgentMode::{Podbot, CodexAppServer, Acp}`
   - `AgentKind::Custom` if aligned with the design
   - `AgentConfig.command: Option<String>`
   - `AgentConfig.args: Vec<String>`
   - `AgentConfig.env_allowlist: Vec<String>`
   - `WorkspaceSource::{GithubClone, HostMount}`
   - `WorkspaceConfig.source`
   - `WorkspaceConfig.host_path: Option<Utf8PathBuf>`
   - `WorkspaceConfig.container_path: Option<Utf8PathBuf>`
   - MCP hosting defaults structure on `AppConfig`

4. Update defaults and loader serialization.

   Ensure `AppConfig::default()` models the newly chosen default behaviour:

   - `agent.kind = claude`
   - `agent.mode = podbot`
   - `workspace.source = github_clone` or the new explicit default chosen for
     the hosted-era schema
   - `workspace.base_dir = /work`
   - hosting defaults present and explicit

5. Extend environment-variable mapping in `src/config/env_vars.rs`.

   Add every new `PODBOT_*` variable required for the hosting schema, update
   `env_var_names()`, and keep path-conflict validation intact.

6. Extend library-facing overrides and/or validation inputs if needed.

   If subcommand legality needs explicit intent, introduce a shared type such
   as `CommandIntent` or a config-validation request model rather than hiding
   the rule inside `main.rs`.

7. Implement normalization and validation.

   Prefer one entry point that can be reused by both CLI and library callers.
   The validation layer should emit actionable messages such as:

   - hosted modes require `podbot host`
   - `workspace.source = "host_mount"` requires both mount paths
   - `agent.kind = "custom"` requires `agent.command`

8. Add rstest unit coverage.

   Update or add tests in:

   - `src/config/tests/types_tests.rs`
   - `src/config/tests/validation.rs`
   - `src/config/tests/layer_precedence_tests.rs`
   - `tests/load_config_integration.rs`

   Cover:

   - interactive default config
   - explicit interactive config file
   - hosting explicit config file
   - env/file/override precedence for hosting fields
   - invalid combination errors
   - normalization from omitted hosting-era fields

9. Add rstest-bdd behavioural coverage.

   Update:

   - `tests/features/configuration.feature`
   - `tests/features/config_loader.feature`
   - `tests/bdd_config.rs`
   - `tests/bdd_config_helpers.rs`
   - `tests/bdd_config_loader.rs`
   - `tests/bdd_config_loader_helpers.rs`

   Add scenarios for:

   - interactive config loads with podbot defaults
   - hosting config loads with host-mount fields
   - invalid hosted mode under `run`
   - missing host-mount path fields
   - custom agent without command

10. Update documentation.

    Required files:

    - `docs/podbot-design.md`
    - `docs/users-guide.md`

    Conditional file:

    - `docs/mcp-server-hosting-design.md`

    Deferred until implementation passes:

    - `docs/podbot-roadmap.md`

11. Run validation with logs.

    Use sequential execution for Markdown tooling so formatters do not race the
    linter:

    ```shell
    set -o pipefail
    make fmt 2>&1 | tee /tmp/podbot-fmt.log
    ```

    ```shell
    set -o pipefail
    MDLINT=/root/.bun/bin/markdownlint-cli2 make markdownlint 2>&1 | \
      tee /tmp/podbot-markdownlint.log
    ```

    ```shell
    set -o pipefail
    make nixie 2>&1 | tee /tmp/podbot-nixie.log
    ```

    ```shell
    set -o pipefail
    make check-fmt 2>&1 | tee /tmp/podbot-check-fmt.log
    ```

    ```shell
    set -o pipefail
    make lint 2>&1 | tee /tmp/podbot-lint.log
    ```

    ```shell
    set -o pipefail
    make test 2>&1 | tee /tmp/podbot-test.log
    ```

12. Mark the roadmap entry complete only after the implementation is finished
    and all commands above pass.

## Validation and Acceptance

The implementation is acceptable only when all of the following are true:

- Hosting-era configuration files can express hosted agent modes, host-mounted
  workspaces, custom agent commands, environment allowlists, and MCP hosting
  defaults.
- The chosen post-Step-1.4 defaults are deterministic and covered by both unit
  and behavioural tests, even where they supersede earlier draft assumptions.
- Illegal combinations of subcommand, `agent.kind`, `agent.mode`, and
  `workspace.source` fail with actionable semantic errors rather than opaque
  parse failures.
- Unit coverage exists for happy paths, unhappy paths, and edge cases using
  `rstest`.
- Behavioural coverage exists for happy paths, unhappy paths, and
  normalization/defaulting scenarios using `rstest-bdd` v0.5.0.
- `docs/podbot-design.md` records the defaulting/normalization decisions and
  `docs/users-guide.md` reflects the user-visible behaviour.
- `make check-fmt`, `make lint`, and `make test` pass. Because this step edits
  Markdown, `make fmt`, `make markdownlint`, and `make nixie` must also pass.

## Idempotence and Recovery

All implementation steps should be repeatable. Configuration tests should
continue using injected environment values so repeated runs do not depend on or
damage global process state. If a behavioural feature file change appears not
to take effect, run:

```shell
cargo clean -p podbot
```

Then rerun the relevant `cargo test` or `make test` command.

If refactoring is required to stay under the 400-line file limit, do it as part
of the same feature branch while preserving behaviour with passing tests before
and after the split.

## Artefacts and Notes

Capture concise evidence in the implementation turn:

- a passing unit test excerpt for the configuration matrix
- a passing BDD excerpt for one interactive and one hosting scenario
- the final `make check-fmt`, `make lint`, and `make test` success lines
- any new semantic error strings that users will see

If the implementation introduces a non-obvious normalization rule or validation
constraint, store a Qdrant project-memory note after the code lands.

## Interfaces and Dependencies

Expected primary edit set:

- `src/config/mod.rs`
- `src/config/types.rs` or new `src/config/*.rs` submodules
- `src/config/env_vars.rs`
- `src/config/load_options.rs`
- `src/config/loader.rs`
- `src/cli/mod.rs`
- `src/main.rs`
- `src/error.rs` if new semantic config errors are needed
- `src/config/tests/*.rs`
- `tests/load_config_integration.rs`
- `tests/bdd_config*.rs`
- `tests/features/configuration.feature`
- `tests/features/config_loader.feature`
- `docs/podbot-design.md`
- `docs/users-guide.md`
- `docs/podbot-roadmap.md` after completion

No new dependencies are expected. The implementation should rely on the
existing `ortho_config`, `mockable`, `rstest`, and `rstest-bdd` stack.

## Revision Note

Initial draft created on 2026-03-29 from the roadmap, design documents, code
inspection, and agent-team reconnaissance. This draft awaits user approval
before implementation begins.
