# OrthoConfig derive for layered precedence

This ExecPlan is a living document. The sections `Constraints`, `Tolerances`,
`Risks`, `Progress`, `Surprises and discoveries`, `Decision Log`, and
`Outcomes and retrospective` must be kept up to date as work proceeds.

Status: IN PROGRESS

## Purpose and big picture

Implement `OrthoConfig` derive macros for the podbot configuration system to
enable layered configuration precedence. The precedence order (lowest to
highest) is: application defaults, configuration file
(`~/.config/podbot/config.toml`), environment variables (`PODBOT_*`),
command-line arguments.

Success is observable when:

- Configuration loads from file, environment, and CLI with correct precedence
- Unit tests using `MergeComposer` verify each precedence layer
- BDD scenarios cover happy and unhappy paths
- `make all` passes without warnings

## Constraints

- Keep all module-level `//!` documentation in place and update as needed.
- Use `camino::Utf8PathBuf` for path fields.
- Do not add new dependencies without explicit approval (ortho_config 0.7.0 is
  already present).
- Ensure no file exceeds 400 lines; split if needed.
- Use en-GB-oxendict spelling in documentation and comments.
- Tests must use `rstest` fixtures and `rstest-bdd` scenarios.
- Use Makefile targets and capture long outputs with `tee` plus
  `set -o pipefail`.
- No `unwrap` or `expect` calls outside test code.

## Tolerances (exception triggers)

- Scope: if the change requires edits to more than 10 files or more than 500
  lines of net changes, stop and ask for confirmation.
- Interface: if a public API outside `crate::config` must change in a breaking
  way, stop and ask for confirmation.
- Dependencies: if a new crate or feature flag is required, stop and ask for
  confirmation.
- Iterations: if tests fail after two fix attempts, stop and ask for
  confirmation with details.

## Risks

- Risk: `src/config/types.rs` may exceed 400 lines after adding OrthoConfig
  derives and attributes. Severity: low Likelihood: medium Mitigation: keep
  implementation compact; the nested configs are small and attributes are
  concise.

- Risk: OrthoConfig-generated CLI parser may conflict with existing clap-based
  Cli struct. Severity: medium Likelihood: low Mitigation: keep Cli struct for
  subcommand dispatch only; remove global flags that OrthoConfig now handles.

- Risk: Environment variable tests may be flaky due to global state. Severity:
  low Likelihood: medium Mitigation: use mutex guards for env var manipulation
  in tests; use MergeComposer for most precedence tests to avoid env pollution.

## Progress

- [x] (2026-01-19 UTC) Create execplan at
      `docs/execplans/1-3-6-ortho-config-derive.md`
- [ ] Add OrthoConfig derives to nested config structs in `types.rs`
- [ ] Add OrthoConfig derive with discovery attrs to `AppConfig`
- [ ] Implement `PostMergeHook` for `AppConfig`
- [ ] Create `loader.rs` with error bridging
- [ ] Update `mod.rs` to export loader
- [ ] Update `cli.rs` to remove global flags handled by OrthoConfig
- [ ] Update `main.rs` to use layered config loading
- [ ] Add OrthoConfig error variant to `error.rs`
- [ ] Add unit tests using `MergeComposer` for layer precedence
- [ ] Add BDD scenarios for layer precedence
- [ ] Update `docs/users-guide.md` with any behaviour changes
- [ ] Mark task complete in `docs/podbot-roadmap.md`
- [ ] Run `make check-fmt`, `make lint`, `make test` and capture logs

## Surprises and discoveries

(To be updated during implementation)

## Decision Log

- Decision: Keep existing Cli struct for subcommand dispatch; OrthoConfig
  handles global config loading. Rationale: Separation of concerns - Cli
  handles command routing, AppConfig handles configuration values. The
  `--config`, `--engine-socket`, and `--image` global flags move to
  OrthoConfig's generated parser. Date/Author: 2026-01-19 / Terry.

- Decision: Nested configs use single-underscore prefixes (e.g., `GITHUB`)
  resulting in double-underscore env vars (e.g., `PODBOT_GITHUB__APP_ID`).
  Rationale: This follows OrthoConfig's standard nested field naming convention
  and matches the documented env vars in `docs/users-guide.md`. Date/Author:
  2026-01-19 / Terry.

- Decision: Create a separate `loader.rs` module rather than adding loading
  logic to `types.rs`. Rationale: Keeps types.rs focused on struct definitions;
  loader.rs handles orchestration and error bridging. Maintains single
  responsibility. Date/Author: 2026-01-19 / Terry.

- Decision: GitHub validation remains explicit (called by subcommand handlers)
  rather than in PostMergeHook. Rationale: Not all commands require GitHub
  config (e.g., `podbot ps`), so automatic validation would break valid use
  cases. This matches existing behaviour. Date/Author: 2026-01-19 / Terry.

## Outcomes and retrospective

(To be completed after implementation)

## Context and orientation

Configuration lives in `src/config/` with:

- `types.rs` - Configuration structs (AppConfig, GitHubConfig, SandboxConfig,
  etc.)
- `cli.rs` - Clap-based CLI definitions (Cli, Commands, subcommand args)
- `mod.rs` - Module exports
- `tests.rs` - Unit tests

Behavioural tests in:

- `tests/bdd_config.rs` - Scenario bindings
- `tests/bdd_config_helpers.rs` - Step definitions and state
- `tests/features/configuration.feature` - Gherkin scenarios

The `ortho_config = "0.7.0"` dependency is already present in `Cargo.toml`.

## Plan of work

### Stage A: Foundation (types enhancement)

Add OrthoConfig derives to all configuration structs in `types.rs`.

1. Add imports for OrthoConfig, OrthoResult, PostMergeContext, PostMergeHook
2. Add OrthoConfig derive to nested configs with prefixes:
   - `GitHubConfig` with `#[ortho_config(prefix = "GITHUB")]`
   - `SandboxConfig` with `#[ortho_config(prefix = "SANDBOX")]`
   - `AgentConfig` with `#[ortho_config(prefix = "AGENT")]`
   - `WorkspaceConfig` with `#[ortho_config(prefix = "WORKSPACE")]`
   - `CredsConfig` with `#[ortho_config(prefix = "CREDS")]`
3. Add `#[ortho_config(default = expr)]` attributes for fields with defaults
4. Add OrthoConfig derive to `AppConfig` with discovery:

   ```rust
   #[derive(Debug, Clone, Default, Deserialize, Serialize, OrthoConfig)]
   #[ortho_config(
       prefix = "PODBOT",
       post_merge_hook,
       discovery(
           app_name = "podbot",
           env_var = "PODBOT_CONFIG_PATH",
           config_file_name = "config.toml",
           dotfile_name = ".podbot.toml",
           config_cli_long = "config",
           config_cli_short = 'c',
           config_cli_visible = true,
       )
   )]
   pub struct AppConfig { ... }
   ```

5. Implement `PostMergeHook` for `AppConfig` (empty for now, placeholder for
   future normalisation)

### Stage B: Loader module

Create `src/config/loader.rs` to bridge OrthoConfig loading with crate errors.

```rust
//! Configuration loading with layered precedence.

use ortho_config::OrthoResult;
use crate::config::AppConfig;
use crate::error::{ConfigError, PodbotError, Result};

/// Load configuration with full layer precedence.
pub fn load_config() -> Result<AppConfig> {
    AppConfig::load().map_err(|e| {
        PodbotError::Config(ConfigError::OrthoConfig(e))
    })
}

/// Load configuration from specific args iterator.
pub fn load_config_from_iter<I, T>(iter: I) -> Result<AppConfig>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    AppConfig::load_from_iter(iter).map_err(|e| {
        PodbotError::Config(ConfigError::OrthoConfig(e))
    })
}
```

Update `mod.rs` to export loader functions.

### Stage C: Error handling

Add OrthoConfig error variant to `src/error.rs`:

```rust
/// The OrthoConfig library returned an error during configuration loading.
#[error("configuration loading failed: {0}")]
OrthoConfig(#[from] std::sync::Arc<ortho_config::OrthoError>),
```

### Stage D: CLI integration

Update `src/config/cli.rs`:

- Remove `--config`, `--engine-socket`, `--image` global flags from Cli struct
  (these are now handled by OrthoConfig's generated parser)

Update `src/main.rs`:

- Replace `Cli::parse()` with OrthoConfig loading
- Parse CLI subcommand separately for dispatch
- Pass loaded `AppConfig` to subcommand handlers

### Stage E: Testing

Add unit tests to `src/config/tests.rs` using `MergeComposer`:

- `test_file_overrides_defaults` - config file values beat defaults
- `test_env_overrides_file` - env vars beat file values
- `test_cli_overrides_env` - CLI args beat env vars
- `test_nested_config_env_vars` - nested fields via double-underscore

Add BDD scenarios to `tests/features/configuration.feature`:

- "Environment variable overrides configuration file"
- "CLI argument overrides environment variable"
- "Configuration file is loaded from XDG path"
- "PODBOT_CONFIG_PATH overrides default discovery"

Add step definitions to `tests/bdd_config_helpers.rs` for env var handling.

### Stage F: Documentation

Update `docs/users-guide.md`:

- Document the `-c/--config` flag (now visible)
- Document `PODBOT_CONFIG_PATH` env var for explicit path override
- Document config file discovery paths

### Stage G: Completion

Mark tasks complete in `docs/podbot-roadmap.md`:

- `[x] Implement OrthoConfig derive for layered precedence.`
- `[x] Support configuration file at ~/.config/podbot/config.toml.`
- `[x] Add validation ensuring required fields are present.`

## Concrete steps

1) Add OrthoConfig imports and derives to `src/config/types.rs`

2) Create `src/config/loader.rs` with load functions

3) Update `src/config/mod.rs` to add `mod loader` and exports

4) Add `OrthoConfig` variant to `ConfigError` in `src/error.rs`

5) Update `src/config/cli.rs` to remove global flags

6) Update `src/main.rs` to use layered loading

7) Add MergeComposer tests to `src/config/tests.rs`

8) Add BDD scenarios to `tests/features/configuration.feature`

9) Add step definitions to `tests/bdd_config_helpers.rs`

10) Update `docs/users-guide.md`

11) Mark tasks complete in `docs/podbot-roadmap.md`

12) Run validation:

    ```bash
    set -o pipefail
    make check-fmt 2>&1 | tee /tmp/podbot-check-fmt.log

    set -o pipefail
    make lint 2>&1 | tee /tmp/podbot-lint.log

    set -o pipefail
    make test 2>&1 | tee /tmp/podbot-test.log
    ```

## Validation and acceptance

Success looks like:

- `make check-fmt`, `make lint`, and `make test` all succeed with no warnings
- Unit tests demonstrate layer precedence with MergeComposer
- BDD scenarios pass for environment and CLI overrides
- Configuration loads correctly from `~/.config/podbot/config.toml`
- Environment variables `PODBOT_*` override file values
- CLI arguments override environment values
- Roadmap tasks marked complete

## Idempotence and recovery

The steps above are safe to rerun. If a command fails, fix the underlying issue
and re-run the same command. If a test or lint failure is not understood after
two attempts, stop and escalate with the captured log file path.

## Artefacts and notes

Keep the following log files for review if needed:

- `/tmp/podbot-check-fmt.log`
- `/tmp/podbot-lint.log`
- `/tmp/podbot-test.log`

## Files to modify

| File                                          | Action | Estimated Lines |
| --------------------------------------------- | ------ | --------------- |
| `docs/execplans/1-3-6-ortho-config-derive.md` | Create | ~300            |
| `src/config/types.rs`                         | Modify | +80 lines       |
| `src/config/loader.rs`                        | Create | ~50 lines       |
| `src/config/mod.rs`                           | Modify | +5 lines        |
| `src/config/cli.rs`                           | Modify | -10 lines       |
| `src/main.rs`                                 | Modify | +15 lines       |
| `src/error.rs`                                | Modify | +10 lines       |
| `src/config/tests.rs`                         | Modify | +60 lines       |
| `tests/bdd_config_helpers.rs`                 | Modify | +40 lines       |
| `tests/features/configuration.feature`        | Modify | +30 lines       |
| `docs/users-guide.md`                         | Modify | +20 lines       |
| `docs/podbot-roadmap.md`                      | Modify | +3 lines        |

## Interfaces and dependencies

At completion, the public API changes:

- `load_config()` function exported from `podbot::config`
- `load_config_from_iter()` function exported from `podbot::config`
- `ConfigError::OrthoConfig` variant added
- Global CLI flags (`--config`, `--engine-socket`, `--image`) removed from `Cli`
  struct (now handled by OrthoConfig)
