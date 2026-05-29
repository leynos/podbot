# Step 4.2.2: Implement safe host-mounted workspaces

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises and discoveries`,
`Decision log`, and `Outcomes and retrospective` must be kept up to date as
work proceeds.

Status: DRAFT

This plan must be approved by the user before implementation begins. No code
under `src/`, no documentation under `docs/`, and no test files should be
edited as part of implementing this plan until that approval is recorded in
the `Decision log`.

## Purpose and big picture

Complete roadmap task 4.2.2 from `docs/podbot-roadmap.md`: "Implement safe
host-mounted workspaces."

`workspace.source = "host_mount"` is already a permitted configuration shape
(landed in Step 1.4.1), but no code path currently turns that choice into an
actual container bind mount. Today, asking for a host-mounted workspace is a
no-op at container-create time: `CreateContainerRequest::from_app_config` reads
`config.image` and `config.sandbox` only, and `build_host_config` never
populates `HostConfig.mounts` or `HostConfig.binds`. The container therefore
launches without the requested host workspace bound in, and the operator
silently sees an empty container filesystem instead of their working tree.

After this change, Podbot will have a library-owned host-mount planning slice
that:

- canonicalises `workspace.host_path` at the host trust boundary before any
  engine call;
- rejects host paths whose canonical form does not descend from a
  configuration-supplied allowlisted root;
- detects and rejects symlink-derived escapes from those roots, surfacing a
  semantic error rather than silently mounting the escaped target;
- validates that the rootless engine can write to the resolved host path,
  surfacing actionable EACCES / EPERM / EROFS diagnostics before container
  start;
- materialises the validated plan as a typed `bollard::models::Mount` with
  hardened bind options (`RPRIVATE` propagation, `non_recursive`, an explicit
  `read_only` choice driven by configuration), inserted into the existing
  `HostConfig` only when `workspace.source = "host_mount"`;
- documents the residual threat-model boundary so embedders understand what
  Podbot does and does not protect against once they enable host mounts.

Observable success when this plan is implemented:

- a new public library entry point (`api::plan_host_mount_workspace`) accepts a
  validated `AppConfig` and returns either a typed `HostMountPlan` or a
  semantic Podbot error;
- a new `[mounts]` configuration section carries an allowlist of mount roots
  and a per-mount read-only default, with deterministic schema loading via
  the existing `ortho_config` layer;
- `EngineConnector::create_container_async` attaches the bind mount when the
  resolved configuration calls for it, leaving the existing `github_clone`
  path byte-identical;
- `make check-fmt`, `make lint`, and `make test` all pass, including new
  `rstest` unit cases, new `rstest-bdd` scenarios, a new `testcontainers`-
  backed end-to-end scenario that proves a real rootless engine can read and
  write the bound directory, and `proptest` coverage for the path-prefix
  invariant;
- `docs/podbot-design.md`, `docs/users-guide.md`, and `docs/developers-guide.md`
  document the new public surface, the threat-model boundary, and the
  hexagonal split between domain and adapters;
- `docs/podbot-roadmap.md` step 4.2.2 is marked done.

Step 4.2.2 deliberately stops at workspace materialisation. It does not start
the agent, does not configure helper-container sharing, and does not change
the credential injection contract; those remain Step 4.3, Step 4.6, and Step
4.4 concerns respectively.

## Constraints

Hard invariants that must hold throughout implementation. Violation requires
escalation, not workarounds.

- Files must remain below 400 lines each. Split modules early if hosting the
  domain types, the probe port, the adapter, and tests in one file would push
  past the limit.
- Every Rust module must begin with a `//!` module-level doc comment.
- Use en-GB-oxendict spelling in code comments and documentation, except where
  quoting external APIs verbatim.
- The library boundary must remain Clap-free. Configuration values reach the
  domain through `AppConfig` and library-owned newtypes, not `clap` parse
  types.
- Public library APIs must return `crate::error::Result<T>` and must not
  print to stdout, print to stderr, or call `std::process::exit`.
- Validation must happen in the library, not in the binary. The CLI adapter
  may only translate user input and presentation.
- Do not silently default to "no allowlist required". If
  `workspace.source = "host_mount"` is selected but `mounts.allowed_roots` is
  empty, validation must fail with a `ConfigError` that names the missing
  field.
- Do not embed `:Z` SELinux relabelling or `:z` shared-label flags in
  generated mounts. SELinux labelling is an operator policy decision and is
  destructive to host content.
- Path canonicalisation must consult the filesystem. Lexical-only crates such
  as `path-clean` or `normpath` are not acceptable substitutes; they cannot
  detect symlink escapes.
- Domain (`src/api/host_mount.rs`) must not perform I/O directly. All
  filesystem and engine probing flows through a `HostPathProbe` driven port
  with a default `cap_std`/`std::fs` adapter and a `mockall`-generated test
  adapter.
- Avoid `unwrap`, `expect`, `panic!`, `std::process::exit`, and `print!`
  family macros in production code, in accordance with `Cargo.toml`'s Clippy
  policy.
- Use `camino::Utf8PathBuf` for all configuration path-bearing fields, and
  `cap_std::fs_utf8` for capability-oriented filesystem access where the
  operation must remain confined to a dirfd.
- Run all required gates before committing each milestone: `make check-fmt`,
  `make lint`, and `make test`. Documentation changes additionally require
  `make fmt`, `make markdownlint`, and `make nixie`.

## Tolerances (exception triggers)

These thresholds bound autonomous action. Reaching one of them means
implementation pauses and documents the situation in `Decision log` before
asking for direction.

- Scope: stop and escalate if implementation requires changes to more than 22
  files or more than 950 net lines of code.
- Interface: stop and escalate if the change requires breaking any stable
  public API listed in `adr-001-define-the-stable-public-library-boundary.md`
  (notably `AppConfig`, `WorkspaceConfig`, `WorkspaceSource`, `ConfigError`,
  `FilesystemError`, `ContainerError`, `PodbotError`, `CommandOutcome`,
  `ConfigLoadOptions`, `ConfigOverrides`, `ExecRequest`, `ExecMode`,
  `ExecContext`, `exec`, `RunRequest`, or the existing `api::*` re-exports).
  Additive changes (new error variants, new types, new module re-exports) are
  in scope; renames and removals are not.
- Dependencies: stop and escalate before adding any non-test dependency.
  `nix` for `geteuid`/`getegid`/`statvfs` is a likely candidate; do not add it
  without explicit approval, and prefer reusing the existing `cap_std`
  surface for read-only probes.
- Iterations: if the same lint or test still fails after three focused fix
  attempts on a single milestone, stop and document the blocker.
- Time: if any milestone exceeds eight working hours of focused effort
  without producing a green gate run, stop and document.
- Ambiguity: stop and escalate if the implementation discovers that the
  allowlist source-of-truth needs to live outside `AppConfig` (for example, a
  separate operator-only policy file) before deciding the schema.
- Threat-model boundary: stop and escalate if a milestone discovers that the
  documented residual race (canonicalise → bollard create) cannot be closed
  by configuration alone and would require a kernel feature (`openat2`'s
  `RESOLVE_BENEATH`) that is unavailable on the project's supported targets.

## Risks

- Risk: TOCTOU between `std::fs::canonicalize` and the eventual `bollard`
  `create_container` syscall. Severity: medium. Likelihood: medium.
  Mitigation: pin allowlisted roots at config load time, document the
  remaining race openly in the threat-model section of `docs/podbot-design.md`,
  require allowlisted roots to be operator-owned directories not writable by
  unprivileged third parties, and reject any canonical workspace path whose
  parent is itself a symlink. Do not claim to close the race with userland
  Rust alone.
- Risk: rootless Podman with `--userns=auto`/`nomap` silently maps the host
  user to `nobody`, producing in-container EACCES that operators read as a
  Podbot bug. Severity: medium. Likelihood: high. Mitigation: run a probe
  write before declaring success, attach the active `idmap` mode to the
  error message, and document the `--userns=keep-id` recommendation in
  `docs/users-guide.md`.
- Risk: bollard 0.20 exposes both `HostConfig.binds` (legacy string form) and
  `HostConfig.mounts` (typed `Mount`). Using the string form would break on
  paths containing `:` and would not let us set propagation defensively.
  Severity: low. Likelihood: low. Mitigation: standardise on
  `HostConfig.mounts` with `MountTypeEnum::BIND`,
  `MountBindOptionsPropagationEnum::RPRIVATE`, and explicit `non_recursive`
  and `read_only` flags, and record this choice in `Decision log`.
- Risk: case-insensitive or Unicode-normalising filesystems (macOS APFS,
  NTFS) could let `Path::starts_with` accept paths whose displayed form
  differs from the allowlist root. Severity: low. Likelihood: low.
  Mitigation: always compare canonicalised forms on both sides, and run a
  property test that pre-canonicalises both inputs through `std::fs::
  canonicalize` against a temp tree generated by `tempfile`.
- Risk: container `target` path collisions with paths Podbot already manages
  (`/work`, `/root/.claude`, `/root/.codex`, `/run/secrets/ghapp_token`).
  Severity: low. Likelihood: low. Mitigation: validate `workspace
  .container_path` against a small constant denylist of Podbot-reserved
  prefixes at config-validation time.
- Risk: feature-file edits appear stale because `rstest-bdd` reads feature
  files at compile time. Severity: low. Likelihood: medium. Mitigation:
  document `cargo clean -p podbot` as the recovery step, mirroring Step
  4.2.1's experience.
- Risk: testcontainers requires a reachable Docker-compatible socket. The
  Step 4.2.1 plan documented a fallback chain (`DOCKER_HOST`,
  `/var/run/docker.sock`, rootless Podman). Severity: low. Likelihood:
  medium. Mitigation: reuse the existing helper pattern from
  `tests/bdd_repository_cloning_e2e.rs` and gate the new scenario behind
  the same socket-detection guard.
- Risk: introducing a `[mounts]` configuration section opens an unbounded
  design surface (per-mount UID maps, capability flags, container labels,
  SELinux relabel toggles). Severity: medium. Likelihood: medium.
  Mitigation: ship the minimum viable schema this step needs
  (`allowed_roots`, `default_read_only`) and explicitly call out future
  fields in a `Future work` section of the design document instead of
  smuggling them in.

## Progress

- [ ] Drafted the ExecPlan and captured current repository constraints, code
  seams, and prerequisite dependencies.
- [ ] User approved the plan and the status was moved to IN PROGRESS.
- [ ] Added `MountsConfig` with `allowed_roots` and `default_read_only`,
  wired into `AppConfig`, `env_vars.rs`, and the loader.
- [ ] Added domain types (`AllowlistedRoot`, `HostMountPlan`,
  `HostMountRequest`, `HostMountWriteProbe`) in `src/api/host_mount.rs` with
  a `HostPathProbe` trait used as the driven port.
- [ ] Added the default `HostPathProbe` adapter in
  `src/engine/connection/host_mount/probe.rs` (or equivalent path), with
  `mockall`-generated mock under `cfg(test)`.
- [ ] Added the engine integration in
  `src/engine/connection/create_container/mod.rs` so `build_create_body`
  attaches the typed `Mount` when the workspace plan calls for it.
- [ ] Extended `FilesystemError` with `PathOutsideAllowlist`,
  `SymlinkEscapeDetected`, `RootlessWriteProbeFailed`; extended
  `ConfigError` with `AllowlistEmpty` if needed.
- [ ] Added `rstest` unit coverage for the domain types (allowlist
  invariants, canonicalisation, denied prefixes, denied target paths) using
  the mocked probe.
- [ ] Added `proptest` coverage for the prefix invariant against
  `tempfile`-generated real directory trees.
- [ ] Added a new `rstest-bdd` feature
  `tests/features/host_mounted_workspaces.feature` plus helpers, exercising
  the public `plan_host_mount_workspace` entry point against the in-process
  filesystem.
- [ ] Added a new `testcontainers`-backed end-to-end scenario in
  `tests/bdd_host_mounted_workspaces_e2e.rs` that creates a container with a
  bind mount and proves the agent process can read and write the bound
  directory.
- [ ] Updated `docs/podbot-design.md` (Host-mount path safety policy and
  Security model sections), `docs/users-guide.md`, `docs/developers-guide.md`
  (hexagonal layering and probe injection notes), and `docs/podbot-roadmap.md`.
- [ ] Ran `make check-fmt`, `make markdownlint`, `make nixie`, `make lint`,
  and `make test` successfully, with logs captured under `/tmp`.
- [ ] Requested and resolved a `coderabbit review --agent` pass.
- [ ] Renamed the branch to `4-2-2-safe-host-mounted-workspaces`, tracking
  `origin/4-2-2-safe-host-mounted-workspaces`, pushed, and opened the draft
  PR for the execplan with the lody session reference.

## Surprises and discoveries

This section will be populated as work progresses. Anticipated entries:

- Whether `bollard` 0.20's `MountBindOptions` exposes `non_recursive` for the
  installed Linux kernel target; if not, fall back to documenting the
  limitation rather than reimplementing the bind syscall.
- Whether `cap_std::fs::Dir::canonicalize` returning a relative path can be
  hidden behind the adapter cleanly, or whether the adapter must accept
  `std::fs::canonicalize` directly. Step 4.2 already mixes both crates; the
  expectation is that `std::fs::canonicalize` will own the host-trust-boundary
  canonicalisation and `cap_std` will own any subsequent confined I/O.
- Whether the existing `tests/bdd_repository_cloning_e2e.rs` `DOCKER_HOST`
  detection helper can be lifted to a shared `tests/testcontainers_support.rs`
  module without violating the 400-line limit.

## Decision log

- Decision (2026-05-29): take a hexagonal split with `src/api/host_mount.rs`
  as the domain, `HostPathProbe` as the driven port, and the engine adapter
  living next to the existing `create_container` module.
  Rationale: matches the layering already established by `git_identity/`
  and `upload_credentials/`, keeps `cap_std` and `bollard` out of the
  domain, and lets the unit tests substitute a `mockall` adapter without a
  filesystem or container daemon, in line with
  `docs/reliable-testing-in-rust-via-dependency-injection.md` and the
  `hexagonal-architecture` skill's dependency rule. Date/Author: 2026-05-29 /
  Codex.
- Decision (2026-05-29): standardise on `bollard::models::Mount` with
  `MountTypeEnum::BIND`, `MountBindOptionsPropagationEnum::RPRIVATE`, and
  explicit `non_recursive` and `read_only` flags, rather than the legacy
  `HostConfig.binds` string form. Rationale: the string form parses colons
  inside paths and provides no typed way to set propagation, so it is unsafe
  in the presence of operator-supplied paths and prevents defensive
  hardening. Date/Author: 2026-05-29 / Codex.
- Decision (2026-05-29): require `mounts.allowed_roots` to be non-empty when
  `workspace.source = "host_mount"` is selected, rather than treating the
  feature as opt-out. Rationale: making the allowlist explicit forces
  operators to declare what host directories Podbot may expose, matches the
  Kubernetes `allowedHostPaths` UX, and is consistent with the design
  document's "explicit mount-boundary checks" requirement. Date/Author:
  2026-05-29 / Codex.
- Decision (2026-05-29): canonicalise via `std::fs::canonicalize` (wrapped
  in `Utf8PathBuf::from_path_buf`) for the host-trust-boundary check, and
  retain `cap_std::fs_utf8::Dir` for any subsequent in-process reads of the
  workspace. Rationale: `cap_std::fs::Dir::canonicalize` deliberately
  returns a relative path and would require post-processing before passing
  it to `bollard`, while we still want `cap_std` for any confined read or
  probe-file lifecycle. Date/Author: 2026-05-29 / Codex.
- Decision (2026-05-29): keep CLI changes minimal. Adding an
  `--allow-host-mount-root` global flag is out of scope; operators express
  allowlists via the configuration file or `PODBOT_MOUNTS_ALLOWED_ROOTS`
  environment variable. Rationale: keeps the CLI a thin adapter and avoids
  baking the allowlist UX into clap before the schema settles. Date/Author:
  2026-05-29 / Codex.
- Decision (2026-05-29): defer per-mount UID-map and SELinux relabel
  toggles. Rationale: they expand the security surface in ways that need
  their own ADR and risk silently destroying operator data when set wrong.
  Document in `Future work` and revisit if Step 4.4 or Step 4.6.3 requires
  them. Date/Author: 2026-05-29 / Codex.

## Outcomes and retrospective

To be completed on landing.

## Context and orientation

Podbot is a Rust library with a thin CLI binary. The hosting-era
configuration schema already understands host-mounted workspaces; this step
is the missing materialisation slice.

Relevant existing code (with full paths):

- `src/config/workspace.rs` defines `WorkspaceConfig`, `WorkspaceSource`,
  and the `default_host_mount_container_path()` helper. New
  `host_path` / `container_path` fields are already present and validated.
- `src/config/validation.rs::validate_host_mount_workspace` already requires
  `workspace.host_path` to be absolute when `WorkspaceSource::HostMount` is
  selected, defaults the container path to `/workspace`, and rejects
  host-mount-only fields under `WorkspaceSource::GithubClone`. This is the
  hook point for the new "must be inside an allowlisted root" rule.
- `src/config/types.rs::AppConfig` aggregates the existing config sections;
  the new `MountsConfig` will be added here alongside `mcp: McpConfig`.
- `src/config/hosting.rs` is the existing precedent for an additive
  configuration section with `#[serde(default)]` defaults and its own
  enum-bearing `Deserialize`/`Serialize` derives. The new `MountsConfig`
  should follow the same shape and live in `src/config/mounts.rs` to keep
  files under 400 lines.
- `src/config/env_vars.rs` lists the `PODBOT_*` environment-variable
  mappings via the `ENV_VAR_SPECS` table, including a `StringList` variant
  the new allowlist can reuse.
- `src/engine/connection/create_container/mod.rs::build_host_config`
  currently sets `privileged`, `cap_add`, `devices`, and `security_opt`
  only. It is the integration point for the new `mounts` field, and
  `from_app_config` is the call-site that needs to learn about
  `workspace.source`.
- `src/api/mod.rs` already re-exports `RepositoryRef`, `BranchName`,
  `WorkspacePath`, and `RunRequest`. The new `plan_host_mount_workspace`
  entry point and `HostMountPlan` / `HostMountRequest` value types will be
  re-exported alongside them.
- `src/error.rs` defines `ConfigError`, `ContainerError`, `GitHubError`,
  and `FilesystemError`. New variants belong on `FilesystemError` and (for
  the empty-allowlist case) `ConfigError`.
- `src/engine/connection/repository_clone/mod.rs` is the existing precedent
  for a focused submodule with its own request type, success type, and
  tests; `host_mount/` should mirror the layout and naming conventions.

Existing tests that constrain the change shape:

- `tests/features/hosting_configuration.feature` already exercises
  `Host-mounted workspace gains a default container path` and
  `Host mount requires an explicit host path`. New host-mount scenarios
  should live in a sibling feature file rather than overloading this one.
- `tests/bdd_repository_cloning.rs` and
  `tests/bdd_repository_cloning_helpers/` show the canonical
  `StepResult<T>`-based BDD layout that the new host-mount helpers should
  mirror.
- `tests/bdd_repository_cloning_e2e.rs` shows the
  `DOCKER_HOST` / rootless-Podman socket detection pattern and the
  `ContainerAsync` drop guard that the new e2e scenario must reuse.
- `src/engine/connection/create_container/tests/` (`mod.rs`,
  `minimal_mode.rs`, `privileged_mode.rs`) demonstrate how `HostConfig`
  field-by-field assertions are written today.

Design and reference documents to keep open during implementation:

- `docs/podbot-design.md` (sections "Execution flow",
  "Host-mount path safety policy", "Security model", "Threat model
  summary").
- `docs/podbot-roadmap.md` step 4.2.2.
- `docs/users-guide.md` for the operator-facing `workspace.source` table.
- `docs/adr-001-define-the-stable-public-library-boundary.md` for the
  public-API contract.
- `docs/developers-guide.md` for the testing layering and DI conventions.
- `docs/rust-testing-with-rstest-fixtures.md` for the unit-test pattern.
- `docs/rstest-bdd-users-guide.md` for the scenario style and
  `StepResult<T>` discipline.
- `docs/reliable-testing-in-rust-via-dependency-injection.md` for the
  probe-port pattern used here.
- `docs/complexity-antipatterns-and-refactoring-strategies.md` for
  function-size and parameter-grouping guidance once the planner grows.
- `docs/ortho-config-users-guide.md` for the `[mounts]` schema additions.
- `docs/rust-doctest-dry-guide.md` for examples on the new public types.

Skills the implementer should keep loaded:

- `execplans` — keep this document current.
- `leta` — semantic navigation for `WorkspaceConfig`, `AppConfig`,
  `HostConfig`, `build_host_config`, and downstream references.
- `rust-router` — route follow-up questions to the smallest Rust skill
  needed.
- `hexagonal-architecture` — keep the probe port and adapters honest.
- `rust-errors` — design new `FilesystemError` / `ConfigError` variants and
  preserve the existing semantic-error contract at the library boundary.
- `rust-types-and-apis` — keep the new newtypes (`AllowlistedRoot`,
  `HostMountPlan`) small and well-factored.
- `rust-memory-and-state` — when the probe lifetime crosses the
  configuration loader and the engine adapter.
- `domain-cli-and-daemons` — to keep the CLI honest as a thin adapter.
- `nextest` — for running the new tests sequentially per
  `AGENTS.md`.

## Plan of work

Each milestone ends with `make check-fmt`, `make lint`, and `make test`
running green. Documentation milestones additionally run `make fmt`,
`make markdownlint`, and `make nixie`. Implementation proceeds milestone by
milestone and commits at each green gate.

### Milestone A: Confirm prerequisites and freeze the domain shape

1. Re-read the design document sections listed above and confirm that no
   newer ADR changes the planned hexagonal split.
2. Inspect the current `WorkspaceConfig`, `AppConfig`, and
   `build_host_config` to verify no other in-flight branch is racing this
   change.
3. Write a short prose note in `Surprises and discoveries` covering any
   new findings from the inspection.
4. Decide the final domain shape, recording it in `Decision log` before any
   code change. The shape proposed by this plan is:

   - `AllowlistedRoot` — a validated, canonicalised `Utf8PathBuf` that
     points to a directory the operator has authorised as a mount root.
   - `HostMountRequest` — borrowed view of the `AppConfig` slice the planner
     needs: workspace source, host path, container path, default-read-only
     toggle, and the allowlist slice.
   - `HostMountPlan` — the validated, materialised plan: canonical host
     source, absolute container target, read-only flag, propagation mode,
     non-recursive flag, and the matched allowlist root.
   - `HostPathProbe` — a driven port with the methods the domain needs:
     `canonicalize`, `is_directory`, `is_symlink`, `parent_is_symlink`, and
     `probe_writable_temporary`. Implementations must be `Sync + Send`.
   - `plan_host_mount_workspace(config: &AppConfig, probe: &dyn
     HostPathProbe) -> Result<HostMountPlan>` — the only public entry
     point.

Acceptance: this section of the plan is updated with any deviations, and
no source files have been modified.

### Milestone B: Configuration surface

1. Add `src/config/mounts.rs` with a `MountsConfig` struct
   (`allowed_roots: Vec<Utf8PathBuf>`, `default_read_only: bool`),
   `Default` implementation, and `#[serde(default)]` derive consistent with
   `McpConfig`.
2. Embed it on `AppConfig` (`pub mounts: MountsConfig`) in
   `src/config/types.rs` with a `#[serde(default)]` attribute.
3. Extend `src/config/env_vars.rs` with three new `ENV_VAR_SPECS` entries:
   - `PODBOT_MOUNTS_ALLOWED_ROOTS` → `["mounts", "allowed_roots"]`,
     `StringList`.
   - `PODBOT_MOUNTS_DEFAULT_READ_ONLY` → `["mounts", "default_read_only"]`,
     `Bool`.
   - Verify `validate_no_path_conflicts()` still passes.
4. Re-export `MountsConfig` from `src/config/mod.rs`.
5. Extend semantic validation in `src/config/validation.rs` with a new rule:
   when `workspace.source = HostMount`, the allowlist must be non-empty.
   Emit `ConfigError::MissingRequired { field: "mounts.allowed_roots" }`.
6. Add a small constant denylist of Podbot-reserved container target
   prefixes (`/work`, `/workspace/.podbot`, `/root/.claude`,
   `/root/.codex`, `/run/secrets`, `/dev`, `/proc`, `/sys`) consulted by
   semantic validation when the operator overrides
   `workspace.container_path`.
7. Add unit coverage in `src/config/tests/`:
   - `mounts_types_tests.rs` for default values and TOML round trips.
   - Extend `hosting_layer_precedence_tests.rs` for env/file/CLI
     precedence over `mounts`.
   - Extend `semantic_validation_tests.rs` with cases for an empty
     allowlist under host mount, a reserved container target, and a happy
     host-mount path.

Acceptance: `make check-fmt`, `make lint`, `make test` pass; new tests
fail before the implementation and pass after.

### Milestone C: Domain types and planner

1. Create `src/api/host_mount.rs` containing the value types listed in
   Milestone A and the `plan_host_mount_workspace` function. The planner
   performs:
   - early return for `WorkspaceSource::GithubClone` (no plan needed);
   - validate `workspace.host_path` is `Some`;
   - canonicalise the host path through the probe;
   - reject if the canonical parent (or any component thereof) is a
     symlink not owned by an allowlist root;
   - confirm the canonical path is a directory;
   - check `canonical.starts_with(root.as_path())` against each allowlist
     entry; the first match wins and is recorded on the plan;
   - probe writability when `default_read_only` is `false` or the
     workspace config explicitly opts in (extension hook for a future
     per-mount `read_only` override);
   - assemble the `HostMountPlan` with `RPRIVATE` propagation,
     `non_recursive = true`, and the chosen `read_only` flag.
2. Add `From<&HostMountPlan> for bollard::models::Mount` (or an
   equivalent inherent method) in the engine adapter, not in the domain.
3. Re-export the public types and function from `src/api/mod.rs` so library
   embedders can call them directly. Mark experimental-only items behind
   the existing `feature = "internal"` gate if they should not be part of
   the stable surface yet.
4. Extend `src/error.rs` with the new variants:
   - `FilesystemError::PathOutsideAllowlist { path, root }`.
   - `FilesystemError::SymlinkEscapeDetected { path, resolved }`.
   - `FilesystemError::RootlessWriteProbeFailed { path, source }` where
     `source` carries the kernel errno via `std::io::Error`.
   Confirm `#[error]` strings match the existing voice and include
   actionable hints where appropriate (`AGENTS.md` directs error messages
   to remain operator-friendly).
5. Add `rstest` unit coverage in `src/api/host_mount.rs` exercising:
   - happy host-mount path resolved against a single allowlist root;
   - happy path against a multi-root allowlist (first match wins);
   - symlink escape rejected;
   - path outside allowlist rejected;
   - non-directory rejected;
   - write-probe failure propagated as `RootlessWriteProbeFailed`;
   - read-only plan does not probe writability.
6. Add `proptest` coverage proving that for any pair of canonicalised
   absolute paths `(root, child)` materialised under `tempfile`,
   `plan_host_mount_workspace` accepts the pair if and only if
   `child.starts_with(root)` and the canonical form lies inside the
   tempdir.

Acceptance: `make check-fmt`, `make lint`, `make test` pass; domain code
has no `bollard` or `cap_std::fs::Dir` imports.

### Milestone D: Default `HostPathProbe` adapter

1. Add `src/engine/connection/host_mount/mod.rs` with a
   `DefaultHostPathProbe` implementing `HostPathProbe` using:
   - `std::fs::canonicalize` for canonicalisation, then
     `Utf8PathBuf::from_path_buf` to recover the `camino` view;
   - `std::fs::symlink_metadata` for symlink detection (do not follow);
   - `cap_std::fs_utf8::Dir::open_ambient_dir(parent, ambient_authority())`
     plus `Dir::open_with` to perform the writability probe by creating
     and immediately removing a uniquely named scratch file
     (`.podbot-write-probe-<pid>-<nanos>`). The probe must always remove
     its scratch file, even on partial failure.
2. Add a `mockall`-generated mock for `HostPathProbe` under `cfg(test)`
   exposed to integration tests via a `#[doc(hidden)] pub` helper, in line
   with the existing `MockExecClient` pattern.
3. Add tests in `src/engine/connection/host_mount/tests.rs`:
   - canonicalises real symlinked directories produced by `tempfile`;
   - returns `SymlinkEscapeDetected` when the resolved path leaves the
     allowlist;
   - returns `RootlessWriteProbeFailed` when the probe directory is
     read-only (`tempdir` with read-only permissions);
   - leaves no probe file behind after success or failure (asserted via a
     direct directory listing).

Acceptance: `make check-fmt`, `make lint`, `make test` pass; no test
mutates global filesystem state outside `tempfile::TempDir`.

### Milestone E: Engine integration

1. Extend `src/engine/connection/create_container/mod.rs`:
   - Replace `build_create_body(request: &CreateContainerRequest)` with a
     small refactor that accepts a borrowed `Option<&HostMountPlan>` (or
     a new `CreateContainerInputs` parameter grouping struct, per
     `AGENTS.md`'s parameter-grouping guidance).
   - Populate `HostConfig.mounts` only when a plan is supplied. Leave
     `binds` untouched; both paths must remain free of unintended
     side effects.
2. Update `CreateContainerRequest::from_app_config` to accept an
   `Option<HostMountPlan>` (or take the plan separately and merge before
   `build_create_body`). Update callers to provide the plan when
   `workspace.source = HostMount`.
3. Refresh the `create_container/tests/` suite to assert:
   - `HostConfig.mounts` is `None` under `WorkspaceSource::GithubClone`;
   - `HostConfig.mounts` contains exactly one `BIND` mount with the
     expected source, target, propagation, `non_recursive`, and
     `read_only` values under `WorkspaceSource::HostMount`;
   - privileged mode still ignores SELinux toggles even with a host
     mount attached.

Acceptance: `make check-fmt`, `make lint`, `make test` pass.

### Milestone F: Behavioural and end-to-end coverage

1. Add `tests/features/host_mounted_workspaces.feature` with scenarios:
   - "Host-mounted workspace inside an allowlisted root produces a plan";
   - "Host-mounted workspace outside every allowlisted root is rejected";
   - "Symlink escape from an allowlisted root is rejected";
   - "Empty allowlist rejects host-mount source";
   - "Rootless engine write-probe failure is mapped to a semantic error";
   - "Reserved container target path is rejected".
2. Add `tests/bdd_host_mounted_workspaces.rs` and a
   `tests/bdd_host_mounted_workspaces_helpers/` module set mirroring the
   `bdd_repository_cloning` layout (`mod.rs`, `state.rs`, `steps.rs`,
   `assertions.rs`). Inject a `mockall`-backed `HostPathProbe` so the
   suite remains daemon-free.
3. Add `tests/bdd_host_mounted_workspaces_e2e.rs` exercising the same
   planner through a real container created via `testcontainers`. The
   scenario:
   - creates a temp directory on the host;
   - configures `[mounts]` with that directory as the allowlist root;
   - drives `plan_host_mount_workspace` and feeds the plan into the
     `EngineConnector::create_container_async` path;
   - starts the container, executes a `sh -c 'echo proof > /workspace
     /probe && cat /workspace/probe'` exec, and asserts the host file is
     visible and that the host-side file content matches.
   Lift the rootless-socket detection helper into a shared
   `tests/testcontainers_support.rs` module if doing so keeps every file
   under 400 lines; otherwise duplicate the snippet with a small note.
4. Confirm the e2e suite skips gracefully when no Docker-compatible socket
   is available, in line with Step 4.2.1's behaviour.

Acceptance: `make check-fmt`, `make lint`, `make test` pass; the new
behavioural and e2e scenarios fail before this milestone and pass after.

### Milestone G: Documentation and roadmap

1. Update `docs/podbot-design.md`:
   - Expand the existing "Host-mount path safety policy" subsection with
     the new domain/probe split, the propagation defaults, and the
     write-probe protocol.
   - Update the "Threat model summary" to acknowledge the residual TOCTOU
     race and the operator responsibility for allowlist hygiene.
   - Add the new `[mounts]` section to the example configuration TOML.
2. Update `docs/users-guide.md`:
   - Document `mounts.allowed_roots`, `mounts.default_read_only`, and the
     two new `PODBOT_MOUNTS_*` environment variables.
   - Document the recommended `--userns=keep-id` rootless flag and the
     expected EACCES / EPERM diagnostics.
3. Update `docs/developers-guide.md`:
   - Record the hexagonal split for host mounts (domain in
     `src/api/host_mount.rs`, port `HostPathProbe`, default adapter in
     `src/engine/connection/host_mount/`).
   - Note the testcontainers helper relocation, if any.
4. Update `docs/podbot-roadmap.md` to mark step 4.2.2 as done only after
   every gate above has passed.
5. Run `make fmt`, `make markdownlint`, and `make nixie` and capture the
   logs.

Acceptance: documentation gates pass; the roadmap entry is updated; the
plan's `Progress` and `Outcomes and retrospective` sections are filled in.

### Milestone H: Review, branch rename, and PR

1. Commit each milestone individually so the history reads as a sequence
   of green gates.
2. Run `coderabbit review --agent` and resolve every concern that survives
   the deterministic gates. Do not request the review until `make
   check-fmt`, `make lint`, and `make test` are green, in line with the
   user prompt's escalation rule.
3. Rename the branch to `4-2-2-safe-host-mounted-workspaces`, tracking
   `origin/4-2-2-safe-host-mounted-workspaces`, and push.
4. Open a draft PR with `(4.2.2)` in the title, a body that references this
   ExecPlan and the roadmap entry, and a `## References` section linking
   the lody session URL derived from `${LODY_SESSION_ID}`.

Acceptance: the draft PR exists, the branch tracks the new remote, and
the lody session URL appears in the PR body.

## Concrete steps

Sequential commands the implementer will run during execution. Each
command captures output via `tee` under `/tmp` so the truncation behaviour
documented in `AGENTS.md` does not eat the diagnostic tail.

```bash
set -o pipefail
make check-fmt 2>&1 \
  | tee /tmp/check-fmt-podbot-$(git branch --show-current).out
```

```bash
set -o pipefail
make lint 2>&1 \
  | tee /tmp/lint-podbot-$(git branch --show-current).out
```

```bash
set -o pipefail
make test 2>&1 \
  | tee /tmp/test-podbot-$(git branch --show-current).out
```

For the optional documentation-only gates:

```bash
set -o pipefail
make fmt 2>&1 \
  | tee /tmp/fmt-podbot-$(git branch --show-current).out
```

```bash
set -o pipefail
make markdownlint 2>&1 \
  | tee /tmp/markdownlint-podbot-$(git branch --show-current).out
```

```bash
set -o pipefail
make nixie 2>&1 \
  | tee /tmp/nixie-podbot-$(git branch --show-current).out
```

For the testcontainers-backed e2e scenario, run nextest with the
`--features e2e` filter (matching Step 4.2.1's convention; confirm the
exact feature name during Milestone F):

```bash
set -o pipefail
cargo nextest run --workspace --features e2e -- bdd_host_mounted_workspaces_e2e 2>&1 \
  | tee /tmp/e2e-podbot-$(git branch --show-current).out
```

For the CodeRabbit review (once all deterministic gates are green):

```bash
coderabbit review --agent 2>&1 \
  | tee /tmp/coderabbit-podbot-$(git branch --show-current).out
```

For the branch rename and push:

```bash
git branch -m 4-2-2-safe-host-mounted-workspaces
git push -u origin 4-2-2-safe-host-mounted-workspaces
```

For the draft PR (filling in the lody session URL):

```bash
gh pr create --draft \
  --base main \
  --title 'Plan safe host-mounted workspaces (4.2.2)' \
  --body "$(cat <<EOF
## Summary

- Add an ExecPlan for roadmap step 4.2.2 that materialises host-mounted
  workspaces with explicit allowlists, canonicalisation, symlink-escape
  rejection, and rootless write-probe validation.
- Hexagonal split: domain in \\`src/api/host_mount.rs\\`, driven port
  \\`HostPathProbe\\`, default adapter in \\`src/engine/connection/host_mount/\\`.
- New \\`[mounts]\\` configuration section and two \\`PODBOT_MOUNTS_*\\`
  environment variables.

## Plan

See \\`docs/execplans/4-2-2-safe-host-mounted-workspaces.md\\`.

## Test plan

- [ ] make check-fmt
- [ ] make lint
- [ ] make test (rstest + rstest-bdd + proptest)
- [ ] testcontainers-backed end-to-end scenario for host-mount bind
- [ ] make markdownlint, make nixie

## References

- Roadmap: docs/podbot-roadmap.md step 4.2.2
- Lody session: https://lody.ai/leynos/sessions/${LODY_SESSION_ID}
EOF
)"
```

## Validation and acceptance

The implementation is acceptable only when all of the following hold:

- A new `HostMountPlan` returned by `api::plan_host_mount_workspace` is
  materialised into the container engine call when
  `workspace.source = "host_mount"`. Operators can observe this by
  configuring an allowlist, launching a container, and seeing the host
  directory under the configured container target.
- Host paths outside every allowlisted root, symlink escapes from an
  allowlisted root, missing allowlists, reserved container targets, and
  rootless write-probe failures all fail with semantic errors that include
  the offending path and an actionable hint.
- The new `rstest` unit suite, `rstest-bdd` behavioural suite, `proptest`
  invariants, and `testcontainers`-backed e2e scenario all fail before
  this change and pass after.
- `make check-fmt`, `make lint`, and `make test` are green. Documentation
  gates (`make fmt`, `make markdownlint`, `make nixie`) are green for the
  changed Markdown.
- `docs/podbot-design.md`, `docs/users-guide.md`, and
  `docs/developers-guide.md` reflect the implemented behaviour. The
  roadmap entry is marked done only after the feature lands.
- A `coderabbit review --agent` pass has been requested and every concern
  surviving the deterministic gates is resolved.

Quality method:

- Run the gates in the order above and capture logs under `/tmp`.
- For BDD scenarios that fail to pick up feature-file edits, run
  `cargo clean -p podbot` and retry, mirroring Step 4.2.1.
- For testcontainers scenarios that fail because no socket is available,
  the suite must skip cleanly rather than fail.

## Idempotence and recovery

- All steps must be safely re-runnable. New configuration fields default
  to empty / `false`, so re-running the loader on a default config is a
  no-op. The write probe always removes its scratch file, even on partial
  failure.
- If the engine adapter fails between probe and `bollard::create_container`
  (a real race), the container is not created and no state is left on the
  host; the operator may retry without manual cleanup.
- If a behavioural test fails because its feature file edit did not
  propagate, run `cargo clean -p podbot` and rerun `make test`.
- If a testcontainers scenario leaves a container behind, the
  `ContainerAsync` drop guard pattern from Step 4.2.1 still applies; clean
  up with `podman container ls -a --filter label=podbot.e2e=true` and
  `podman rm -f <id>`.

## Artefacts and notes

Capture, in the implementation turn, short transcripts proving:

- a passing `rstest` excerpt for the happy host-mount plan;
- a passing `rstest-bdd` excerpt for the empty-allowlist rejection;
- a passing `testcontainers` excerpt for the round-trip write;
- the final `make check-fmt`, `make lint`, and `make test` success lines;
- the new semantic error strings operators will see.

If the implementation introduces a non-obvious normalisation rule or
validation constraint, store a short developer-guide note rather than a
freestanding ADR unless the design boundary itself shifts.

## Interfaces and dependencies

The expected primary edit set is:

- `src/config/mod.rs` — re-export `MountsConfig`.
- `src/config/mounts.rs` (new) — `MountsConfig` and defaults.
- `src/config/types.rs` — embed `mounts: MountsConfig`.
- `src/config/env_vars.rs` — add `PODBOT_MOUNTS_*` entries.
- `src/config/validation.rs` — empty-allowlist and reserved-target rules.
- `src/config/tests/mounts_types_tests.rs` (new) — defaults and TOML
  round trips.
- `src/config/tests/hosting_layer_precedence_tests.rs` — extended
  precedence cases.
- `src/config/tests/semantic_validation_tests.rs` — extended host-mount
  cases.
- `src/api/mod.rs` — re-export the new host-mount surface.
- `src/api/host_mount.rs` (new) — domain types, planner, port trait, and
  unit tests.
- `src/engine/connection/host_mount/mod.rs` (new) — `DefaultHostPathProbe`
  adapter and the `bollard::models::Mount` conversion.
- `src/engine/connection/host_mount/tests.rs` (new) — adapter tests.
- `src/engine/connection/create_container/mod.rs` — accept and apply the
  optional `HostMountPlan` and populate `HostConfig.mounts`.
- `src/engine/connection/create_container/tests/*` — extended assertions.
- `src/error.rs` — new `FilesystemError` (and possibly `ConfigError`)
  variants.
- `tests/features/host_mounted_workspaces.feature` (new) — BDD scenarios.
- `tests/bdd_host_mounted_workspaces.rs` (new) and
  `tests/bdd_host_mounted_workspaces_helpers/` (new) — BDD scaffold.
- `tests/bdd_host_mounted_workspaces_e2e.rs` (new) — testcontainers
  scenario.
- `tests/testcontainers_support.rs` (new, optional) — shared
  socket-detection helper.
- `docs/podbot-design.md`, `docs/users-guide.md`,
  `docs/developers-guide.md`, `docs/podbot-roadmap.md`.

Concretely, in `src/api/host_mount.rs`, define:

```rust
pub struct AllowlistedRoot(Utf8PathBuf);

pub struct HostMountRequest<'a> {
    pub workspace: &'a WorkspaceConfig,
    pub mounts: &'a MountsConfig,
}

pub struct HostMountPlan {
    canonical_host_path: Utf8PathBuf,
    container_path: Utf8PathBuf,
    matched_root: AllowlistedRoot,
    read_only: bool,
}

pub trait HostPathProbe: Sync + Send {
    fn canonicalize(&self, path: &Utf8Path) -> PodbotResult<Utf8PathBuf>;
    fn is_directory(&self, path: &Utf8Path) -> PodbotResult<bool>;
    fn parent_is_symlink(&self, path: &Utf8Path) -> PodbotResult<bool>;
    fn probe_writable(&self, path: &Utf8Path) -> PodbotResult<()>;
}

pub fn plan_host_mount_workspace(
    config: &AppConfig,
    probe: &dyn HostPathProbe,
) -> PodbotResult<Option<HostMountPlan>>;
```

In `src/engine/connection/host_mount/mod.rs`, define:

```rust
pub struct DefaultHostPathProbe;

impl HostPathProbe for DefaultHostPathProbe { /* std::fs + cap_std */ }

impl HostMountPlan {
    pub(crate) fn into_bollard_mount(&self) -> bollard::models::Mount;
}
```

Dependencies: no new crates are expected. If the rootless write probe
needs `nix` for `geteuid`/`statvfs`, treat that as a tolerance event and
escalate before adding the dependency. The default adapter should attempt
to satisfy the probe using `cap_std::fs_utf8` and `std::fs` first.

References used while planning this step:

- `https://docs.rs/bollard/latest/bollard/models/struct.HostConfig.html`
- `https://docs.rs/cap-std/latest/cap_std/fs/struct.Dir.html`
- `https://docs.rs/cap-std/latest/cap_std/fs_utf8/struct.Dir.html`
- `https://github.com/opencontainers/runtime-spec/blob/main/config.md#mounts`
- `https://kubernetes.io/docs/concepts/security/pod-security-policy/#volumes-and-file-systems`
- `https://www.redhat.com/en/blog/rootless-podman-user-namespace-modes`
- `https://devcontainers.github.io/implementors/json_reference/`

## Revision note

Initial draft created 2026-05-29 from the roadmap, design documents, code
inspection, and parallel research into bollard bind-mount shapes,
cap-std versus std canonicalisation, rootless Podman userns behaviour, and
prior art in OCI / Kubernetes / devcontainers. This draft awaits user
approval before implementation begins.
