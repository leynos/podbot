# Step 3.3.1: Token daemon runtime directory and atomic token writer

This ExecPlan (execution plan) is a living document. The sections `Constraints`,
`Tolerances`, `Risks`, `Progress`, `Surprises and discoveries`, `Decision log`,
and `Outcomes and retrospective` must be kept up to date as work proceeds.

Status: DRAFT

## Purpose and big picture

Podbot is preparing the host-side surface that will later let GitHub App
installation tokens rotate while a long-lived sandbox keeps reading a stable
in-container secret path. Step 3.2.1 made token acquisition real;
`InstallationAccessToken` already carries the token string and conservative
expiry metadata. The next host-side primitive is somewhere safe to put that
token on disk: a per-container runtime directory and an atomic file writer.

After this change, library code can ask a host-side `TokenWriter` to publish a
freshly acquired token for a given container identifier and receive back the
host path of the per-container *directory* that 3.4.1 will later bind-mount
read-only into the sandbox. The directory will exist at
`$XDG_RUNTIME_DIR/podbot/<container_id>/` with mode `0700`, the
`ghapp_token` file inside will be written via rename from a temporary sibling
at mode `0600`, and concurrent readers (today: an integration test fixture;
tomorrow: a `GIT_ASKPASS` helper inside the sandbox) will never observe a
missing or partially-written file. The port returns the directory rather than
the file path on purpose: bind-mounting a file on Linux pins a dentry at mount
time, so a future bind-mount of the file would leave containers reading stale
data after each rotation. Encoding "mount the directory" at the type level
makes the wrong choice impossible. This slice does not start a refresh loop,
does not bind-mount the directory into a container, and does not configure Git
askpass; those belong to roadmap items 3.3.2 and 3.4.1.

A reader gains the ability to call a `TokenWriter` with a validated container
identifier and an `InstallationAccessToken` and observe the resulting file with
the correct contents, mode, and ownership; if `$XDG_RUNTIME_DIR` is unset, the
adapter falls back to `/tmp/podbot-<uid>` with a logged warning rather than
failing.

## Constraints

This slice must respect the existing podbot architecture and policy.

- Roadmap and design fidelity. Follow roadmap item 3.3.1 in
  `docs/podbot-roadmap.md` and the token-management contract in
  `docs/podbot-design.md` §Token management. The runtime directory must be
  `$XDG_RUNTIME_DIR/podbot/<container_id>/` at mode `0700`; the token file must
  be named `ghapp_token` at mode `0600`; writes must be atomic from a reader's
  point of view via a rename from a temporary file in the same directory.
- Hexagonal boundary. The domain port (a `TokenWriter` trait, a `ContainerId`
  newtype, a `RuntimeDir` newtype, a `TokenMaterial` byte-slice wrapper)
  belongs in the `api` module so the dependency rule points inwards. The
  capability-oriented filesystem adapter belongs in a new internal
  `token_daemon` module that depends on the port, not the other way round.
  Higher-level orchestration must not learn about `cap_std::fs_utf8::Dir`,
  `std::os::unix::fs::PermissionsExt`, or the XDG fallback policy. The port
  must not import any type from `crate::github`; the adapter converts an
  `&InstallationAccessToken` into a transient `TokenMaterial` at the boundary
  so the port remains domain-pure.
- Capability-oriented filesystem access. All host-side filesystem work goes
  through `cap_std::fs_utf8::Dir` with `cap_std::ambient_authority()`. No
  ambient `std::fs` operations on user-supplied paths; no raw `std::fs::rename`
  across directories. The writer opens the runtime root `Dir` once during
  construction and reuses it for the writer's lifetime; per-write code does
  not re-open it.
- No token bytes in observability. The token must not appear in `Display` or
  `Debug` output, in `tracing` events, in metric labels, or in formatted error
  messages. `TokenMaterial` carries the bytes opaquely with a manual redacted
  `Debug`; the bytes leave it only through a single in-module `as_bytes`
  accessor used inside the file write path. A `clippy::disallowed_methods`
  configuration line bans
  `crate::github::installation_token::InstallationAccessToken::token`
  outside the `token_daemon` module so future contributors cannot accidentally
  interpolate token bytes into a tracing field elsewhere.
- File and module size. Every Rust source file remains under 400 lines, with
  submodule extraction at the first commit. The new `token_daemon` module is a
  directory module (`mod.rs`) with `runtime_dir.rs` and `atomic_writer.rs`
  siblings. Tests live in `*_tests.rs` files next to their subject, per the
  conventions in `docs/developers-guide.md` §13 and §15.
- Testing libraries. Use `rstest` fixtures for unit coverage, `rstest-bdd`
  v0.5.0 for behavioural coverage, `pretty_assertions`/`googletest` assertions
  where they make failure diagnostics clearer, and `insta` only if a stable
  serialised representation is added (none is planned). Use `mockall` for the
  `EnvSource` and any seam that abstracts a real environment dependency. Do
  not introduce property tests, Kani harnesses, or Verus proofs for this slice
  unless the design produces a real invariant over a generated input domain
  (the only candidate is `ContainerId::parse`, where a small `proptest`
  generator is justified if the parser grows beyond a single regex-shaped
  predicate; otherwise table-driven `rstest` cases suffice).
- Spelling, formatting, and gates. Use en-GB-oxendict in documentation and
  Rust comments. Run quality gates sequentially with `tee` log capture under
  `/tmp`: `make check-fmt`, `make lint`, `make test`, and for doc changes
  `make markdownlint`, `make fmt`, `make nixie`. Commit atomically per gate
  using a HEREDOC-driven `git commit -F` body. Run `coderabbit review --agent`
  after each major milestone and clear actionable findings before continuing.
- Public-API stability. Do not change the shape of any symbol in
  `podbot::api`, `podbot::config`, or `podbot::error` that is part of the
  stable boundary documented in `docs/podbot-design.md` §Public library API
  reference. Additive symbols may land behind `#[cfg(any(feature = "internal",
  test))]` if they are not yet ready to graduate.
- Platform scope. This slice targets Linux only. `XDG_RUNTIME_DIR`,
  `/run/secrets`, and the kernel guarantees the design relies on are Linux
  idioms. The adapter module is gated behind `#[cfg(target_os = "linux")]`;
  non-Linux builds must still compile, but the adapter is absent and the port
  remains usable for tests that supply their own implementation.

## Tolerances (exception triggers)

When any threshold below would be crossed, stop implementation and escalate.
Do not adjust the design quietly to fit under a tolerance.

- Scope. If the slice requires more than 14 files changed or more than 650
  net Rust lines added, stop and re-decompose with the user.
- Public API. If a stable public symbol in `podbot::api`, `podbot::config`,
  or `podbot::error` must change shape (not merely gain an additive method),
  stop and request approval.
- Dependencies. Adding any new crate beyond what `Cargo.toml` already lists
  requires approval. `tempfile` is already a dev-dependency and may be used in
  tests, but `tempfile`, `atomicwrites`, `cap-tempfile`, `nix`, and `rustix`
  are explicitly off-limits as production dependencies for this slice; the
  codebase already standardises on `cap_std::fs_utf8`.
- Concurrency primitives. If correctness requires a `flock`, a `loom`
  harness, an async runtime inside the writer, or shared mutable state, stop.
  The design assumes one writer per container and POSIX rename atomicity on
  the same filesystem; needing more says the design is wrong.
- Platform support. If a non-Linux target needs a real adapter (not a
  compile-time stub), stop. The roadmap explicitly scopes containers,
  `/dev/fuse`, and `/run/secrets/ghapp_token` to Linux.
- Test iterations. If `make lint` or `make test` still fails after three
  focused fix passes, stop, record the failing logs under `/tmp`, and ask for
  direction.
- Ambiguity. If two valid behaviours have materially different effects on
  token exposure, atomicity guarantees, fallback policy, or the public API
  shape, stop and present the trade-offs.

## Risks

These uncertainties are anticipated and have mitigations. Update as
implementation proceeds.

- Risk: token bytes leak through error `Display`, `Debug`, or `tracing`
  fields. Severity: high. Likelihood: medium. Mitigation: the adapter consumes
  `&InstallationAccessToken` (already redacted) and reads `.token()` exactly
  once into a local; errors carry only container identifiers and host paths;
  M3 includes an explicit redaction test that constructs a sentinel token and
  asserts its absence from every captured error and tracing field.
- Risk: umask masks the requested `0700`/`0600` mode bits silently. Severity:
  high. Likelihood: high. Mitigation: create the directory and file with the
  requested mode, then `set_permissions` to the same mode explicitly, and read
  back `metadata().permissions().mode() & 0o777` to assert the final mode.
  Mismatches surface as `FilesystemError::PermissionDenied` rather than silent
  corruption. Cover with `defeats_umask_022` and `defeats_umask_077` cases.
- Risk: `EXDEV` if the temp file and target file straddle filesystems.
  Severity: medium. Likelihood: medium. Mitigation: the temp file is opened
  inside the same `Dir` capability as the target via `Dir::open_with` with
  `O_CREAT | O_EXCL`, so the rename never crosses a mount point. A BDD
  scenario verifies the failure path when an operator supplies an override
  root that straddles filesystems.
- Risk: a concurrent reader observes an empty or partial file during a
  rotation. Severity: high. Likelihood: low. Mitigation: rename on the same
  filesystem is atomic by `rename(2)`; M3 includes a stress test with one
  writer thread and eight reader threads over 200 iterations asserting that
  every read returns either a complete previous-generation token or a complete
  new-generation token, never empty bytes or a partial prefix.
- Risk: `$XDG_RUNTIME_DIR` fallback to `/tmp` creates a parent directory other
  users can traverse. Severity: medium. Likelihood: medium. Mitigation: the
  fallback root `/tmp/podbot-<uid>` is created at mode `0700`; the adapter
  verifies the directory's owner equals the current `uid` before reuse, and
  warns loudly with `tracing::warn!` so operators see that XDG is unset. A BDD
  scenario covers fallback discovery and warning emission.
- Risk: scope creep into refresh, cleanup-on-stop, or bind-mount wiring.
  Severity: medium. Likelihood: high. Mitigation: the port exposes `write` and
  `cleanup` only; refresh loop work, askpass helper installation, and
  read-only bind-mount construction stay deferred to 3.3.2 and 3.4.1. The
  scope tolerance above stops drift.
- Risk: future readers misinterpret bind-mount semantics for the token file.
  Severity: high. Likelihood: medium. Mitigation: a host-side file bind mount
  captures a dentry at mount time, so rename swaps would leave a container
  with stale data. The port returns a `RuntimeDir` (the per-container host
  directory) rather than a `TokenFilePath`, so 3.4.1 cannot accidentally
  mount the file. `docs/podbot-design.md` §Token management and §Security
  model gain an explicit "mount the directory, never the file" rule, and the
  developers' guide records the dentry footgun.
- Risk: a developer adds a tracing event that interpolates token bytes via
  `InstallationAccessToken::token`. Severity: high. Likelihood: medium.
  Mitigation: an explicit `clippy::disallowed_methods` configuration line
  bans the accessor outside `src/token_daemon/`, and the M3 redaction test
  uses a sentinel byte sequence asserted absent from every captured tracing
  field. The port's `TokenMaterial` carries an opaque byte slice rather than
  a `&str` to discourage `format!("{:?}", token)` patterns.
- Risk: `FilesystemError::IoError`'s string `message` discards
  `std::io::ErrorKind`, so 3.3.2's retry loop cannot distinguish recoverable
  failures (e.g. `ErrorKind::Interrupted`, `ErrorKind::WouldBlock`) from
  terminal ones (`ErrorKind::PermissionDenied`,
  `ErrorKind::CrossesDevices`). Severity: medium. Likelihood: high.
  Mitigation: classify into the most specific existing variant
  (`FilesystemError::NotFound`, `PermissionDenied`, `IoError`) at the
  adapter boundary and propagate the source `io::Error` via the existing
  `FilesystemError::IoError { source }` `#[from]` chain so callers can
  downcast and inspect `ErrorKind` without parsing strings. If no `source`
  field exists today, add one as an additive change rather than packing the
  `ErrorKind` into the message string.
- Risk: stale `ghapp_token.tmp.<random>` files accumulate when a write fails
  after the temp file is created but before rename. Severity: low.
  Likelihood: medium. Mitigation: the writer removes any pre-existing
  `ghapp_token.tmp.*` siblings before each new write (best-effort, logged
  at debug), and also unlinks the current temp file on the error path
  inside `write` via an RAII guard.
- Risk: `/tmp/podbot-<uid>` fallback collides with `systemd-tmpfiles`
  policies that periodically clean paths under `/tmp`. Severity: medium.
  Likelihood: medium. Mitigation: the resolver defaults `allow_fallback` to
  `false`, requiring operators to opt into the fallback explicitly. When
  opt-in is set, the resolver prefers `$XDG_STATE_HOME/podbot/runtime/`
  (which honours the XDG state spec and is not subject to tmpfiles cleanup)
  and only falls back to `/tmp/podbot-<uid>` if `$XDG_STATE_HOME` is also
  unavailable.

## Context and orientation

Podbot is a Rust 2024 workspace; `src/lib.rs` declares the stable public
modules `api`, `config`, and `error`, and gates `engine` and `github` behind
`#[cfg(any(feature = "internal", test))]`. Quality gates are wrapped by the
`Makefile`. The repository uses `cap_std::fs_utf8::Dir` with
`cap_std::ambient_authority()` for all host-side filesystem work, `thiserror`
for semantic errors, `tracing` for observability, and `metrics` for counters
and histograms.

The relevant existing surfaces are:

- `src/lib.rs`: declares public modules and feature gating. The new
  `token_daemon` module will be added here under `#[cfg(any(feature =
  "internal", test))]`, alongside `engine` and `github`.
- `src/api/mod.rs`: the stable orchestration surface. The existing
  `run_token_daemon(_container_id: &str)` function is a `feature =
  "experimental"` stub; it stays a stub for this slice. The new port goes
  into `src/api/token_writer.rs` with a gated re-export so library callers
  outside the crate cannot depend on the unstable surface yet.
- `src/github/installation_token.rs`: defines `InstallationAccessToken` with
  `.token()`, `.acquired_at()`, `.expires_at()`, `.refresh_after()`, and
  `.log_timing()`. The writer accepts `&InstallationAccessToken` and reads
  `.token()` only inside the file write path.
- `src/error.rs`: defines `PodbotError`, `ConfigError`, `ContainerError`,
  `GitHubError`, and `FilesystemError`. The slice reuses existing variants:
  `FilesystemError::NotFound`, `FilesystemError::PermissionDenied`,
  `FilesystemError::IoError`, and `GitHubError::TokenRefreshFailed` for the
  acquisition-to-write seam if needed. Avoid adding new variants unless a
  failure has no semantic home.
- `src/engine/connection/upload_credentials/`: the closest pattern in the
  codebase for capability-oriented file operations. Re-read this module for
  the idiomatic style before writing the adapter.
- `Cargo.toml`: `cap-std` 4.0.0 with `fs_utf8`, `camino` 1.2.2,
  `mockable` 0.1.4 with the `clock` feature, `tracing` 0.1.44, and `metrics`
  0.24.6 are already present. `tempfile` is a dev-dependency; `serial_test`
  is available for tests that touch shared host state.

A reader new to this work should also skim
`docs/execplans/3-2-1-installation-token-with-buffer.md` for the established
voice and `docs/rust-testing-with-rstest-fixtures.md` plus
`docs/rstest-bdd-users-guide.md` for the testing idiom this slice follows.

Two terms of art recur. "Runtime directory" means the per-container directory
under `$XDG_RUNTIME_DIR/podbot/<container_id>/` that holds rotating secrets.
"Atomic writer" means the host-side primitive that publishes a new token by
writing to a temporary file in the same directory and renaming it over the
existing `ghapp_token` so that a reader, performing a single `open(2)`, sees
either the old generation or the new generation in full.

## Skills, design references, and signposts

- Skills referenced during planning: `execplans`, `leta`, `rust-router`,
  `hexagonal-architecture`, `firecrawl`, `logisphere-design-review`.
- Skills to load during implementation, as needed:
  - `rust-router` to choose the smallest follow-on skill;
  - `rust-errors` for `FilesystemError` mapping and the no-secret error rule;
  - `rust-memory-and-state` for `cap_std::fs_utf8::Dir` ownership and
    capability handles;
  - `rust-async-and-concurrency` only if the writer needs to share state
    across tasks (it should not for this slice);
  - `proptest` and `rust-verification` for the `ContainerId` parser if a
    proptest generator is justified;
  - `rstest-bdd` (via `docs/rstest-bdd-users-guide.md`) for behavioural tests;
  - `commit-message` for atomic commits and `pr-creation` for the draft PR.
- Design and test documentation to consult:
  - `docs/podbot-design.md` §Token management, §Security model, §Module
    structure, §Public library API reference, §Error handling.
  - `docs/podbot-roadmap.md` items 3.2.1 (complete), 3.3.1 (this slice),
    3.3.2 and 3.4.1 (deferred).
  - `docs/developers-guide.md` §13 (testing conventions), §15 (file length
    and module-level documentation), §16 if present (recent installation
    token guidance).
  - `docs/rust-testing-with-rstest-fixtures.md`,
    `docs/rstest-bdd-users-guide.md`,
    `docs/rust-doctest-dry-guide.md`,
    `docs/reliable-testing-in-rust-via-dependency-injection.md`,
    `docs/complexity-antipatterns-and-refactoring-strategies.md`,
    `docs/ortho-config-users-guide.md`.

## Plan of work

The plan splits into five milestones, each with a green gate before the next
begins. The first three are structural; the fourth proves behaviour against
the port; the fifth lands documentation and the roadmap tick.

### Stage A: domain port and types (M1, red tests first)

Introduce the hexagonal port and the domain newtypes it consumes, in
`src/api/token_writer.rs`. Add a hidden re-export in `src/api/mod.rs` gated
by `#[cfg(any(feature = "internal", test))]`; do not yet expose the port on
the stable surface.

- Define `pub struct ContainerId(String)` with a `pub fn parse(value: impl
  AsRef<str>) -> Result<Self>` constructor that rejects empty input, path
  separators (`/`, `\\`), `..`, NUL, ASCII control characters, and any input
  whose trimmed length is outside `8..=64` bytes. The range accepts both the
  12-character short form and the 64-character full form of an OCI container
  ID. Provide `as_str(&self) -> &str` and `into_inner` for bridges. Derive
  `Clone`, `Eq`, and `PartialEq`. Implement a deliberate `fmt::Display` so
  the encoded string is the only thing observable; do not derive `Debug` to
  expose the inner string verbatim — the encoded ID is not secret, but a
  manual `Debug` keeps the representation aligned with `Display`. Validate
  behaviour via table-driven `rstest` cases; if the parser grows beyond a
  single predicate during implementation, add a `proptest` generator under
  `#[cfg(test)]` per `docs/podbot-roadmap.md` testing guidance.
- Define `pub struct RuntimeDir(Utf8PathBuf)` with `as_path(&self) ->
  &camino::Utf8Path` and `into_inner(self) -> Utf8PathBuf`. This is the
  port's success type; consumers compose `dir.as_path().join("ghapp_token")`
  when they need the file path. Returning the directory rather than the file
  encodes the bind-mount-the-directory rule at the type level: 3.4.1's mount
  wiring cannot mistakenly bind-mount the file because the writer never
  hands out its path.
- Define `pub struct TokenMaterial<'a>(&'a [u8])` with `pub fn as_bytes(&self)
  -> &[u8]` and a manual redacted `Debug` impl. The adapter constructs a
  `TokenMaterial` from `InstallationAccessToken::token().as_bytes()`
  exclusively inside `src/token_daemon/`; tests that need a stub writer
  build `TokenMaterial` directly without depending on `crate::github`. This
  keeps the port domain-pure and breaks the otherwise-circular dependency
  from `api` to `github`.
- Define the port:

  ```rust
  pub trait TokenWriter: Send + Sync {
      fn write(
          &self,
          container: &ContainerId,
          token: TokenMaterial<'_>,
      ) -> crate::error::Result<RuntimeDir>;
  }
  ```

  `cleanup` is deliberately omitted from the port. The roadmap defers
  container-stop cleanup to 3.3.2/4.x, and no caller in this slice needs to
  remove a per-container directory. The adapter exposes a concrete
  `pub(crate) fn cleanup(&self, container: &ContainerId) ->
  crate::error::Result<()>` for future drivers to call, but it does not
  appear on the port until a real caller justifies the surface.
- Decide synchronously vs asynchronously: the port is synchronous. The
  underlying `cap_std` filesystem operations are blocking syscalls; wrapping
  them in `async` would mislead callers into thinking the writer yields.
  Async drivers (such as 3.3.2's refresh loop) must use
  `tokio::task::spawn_blocking` when invoking `TokenWriter::write` from
  inside an async task. Record this requirement in the port's rustdoc.
- Add `src/api/token_writer_tests.rs` with `rstest` cases for every
  `ContainerId::parse` rejection rule plus an accepting case for both a
  12-character short ID and a 64-character lowercase hex full ID.
- Acceptance for M1: `make check-fmt`, `make lint`, and `make test` all
  green; the adapter does not yet exist; the new port and types are gated
  `#[cfg(any(feature = "internal", test))]` re-exports from `src/api/mod.rs`
  until the adapter wires through.

### Stage B: XDG runtime directory resolution (M2)

Create `src/token_daemon/mod.rs` (new directory module),
`src/token_daemon/runtime_dir.rs`, and `src/token_daemon/runtime_dir_tests.rs`.

- Define `pub(crate) struct RuntimeDirPolicy` with an optional
  `override_root: Option<camino::Utf8PathBuf>` and a Boolean `allow_fallback`
  whose default is **`false`**. Operators must opt in to fallback explicitly
  because `/tmp` is subject to `systemd-tmpfiles` cleanup policies that can
  race the writer. The policy carries no XDG resolution logic; it is the
  immutable input to the resolver.
- Reuse the existing `mockable` crate for environment injection rather than
  inventing a new trait. The codebase already pulls `mockable` for the
  `clock` feature; check whether the crate exposes an `env` feature (or a
  trait such as `mockable::env::Env`). If yes, use it; if no, define a
  minimal `pub(crate) trait EnvSource` with one method `fn var(&self, key:
  &str) -> Option<String>` and a `mockall` mock for tests, and record the
  reuse decision in `Decision log`.
- Implement `pub(crate) fn resolve_root(policy: &RuntimeDirPolicy, env: &dyn
  EnvSource, uid: u32) -> Result<Utf8PathBuf>` that:
  1. Returns `policy.override_root` if set, after verifying it exists, is a
     directory, is owned by `uid`, and has mode `0700`.
  2. Reads `XDG_RUNTIME_DIR` via the env source; rejects relative paths,
     missing paths, or paths not owned by `uid` and not at mode `0700`.
  3. Otherwise, if `allow_fallback` is true, prefers `$XDG_STATE_HOME` or
     `$HOME/.local/state` (per the XDG state spec) and returns
     `<state_home>/podbot/runtime/<container_id>'s parent>`. Creates the
     directory chain at mode `0700` if missing and verifies ownership.
     Falls back to `/tmp/podbot-<uid>` only when `$XDG_STATE_HOME` and
     `$HOME` are both unavailable. Emits `tracing::warn!` keyed by
     `xdg_runtime_dir_missing = true` and `fallback_root = <chosen path>`.
  4. Otherwise (XDG unset, fallback disabled), returns
     `FilesystemError::NotFound { path: PathBuf::from("$XDG_RUNTIME_DIR") }`
     with a message instructing the operator to set the variable or enable
     fallback explicitly.
- Test cases: XDG set and valid; XDG set but relative; XDG set but missing;
  XDG set but mode `0755`; XDG set but owned by another user; XDG unset
  with fallback disabled (default); XDG unset with fallback enabled and
  XDG_STATE_HOME present; XDG unset with fallback enabled and neither
  XDG_STATE_HOME nor HOME present; override root takes precedence. Use a
  captured tracing subscriber (`tracing-subscriber` with `with_test_writer`)
  to assert the warning is emitted exactly once when fallback kicks in.
- Acceptance for M2: all unit tests pass; no IO outside test-owned temp
  directories; `make check-fmt`, `make lint`, `make test` green.

### Stage C: atomic writer adapter (M3)

Implement the adapter as a directory module from the first commit:
`src/token_daemon/atomic_writer.rs` for the `TokenWriter` impl and
orchestration, `src/token_daemon/dir_ops.rs` for `Dir` open/create helpers
plus mode verification, and `src/token_daemon/mode.rs` for the
`enforce_mode` helper that does the post-create `set_permissions` plus
read-back. Splitting at the start avoids a 400-line cliff during M3.

- Define `pub(crate) struct CapStdTokenWriter` carrying the runtime root
  `Dir` (opened once at construction), the resolved root path (for return
  values and error context), and the resolved `uid`. Expose
  `pub(crate) fn new(policy: RuntimeDirPolicy) -> Result<Self>`; this is the
  composition-root constructor that production code calls. The root `Dir`
  is opened with `cap_std::fs_utf8::Dir::open_ambient_dir(root,
  ambient_authority())` exactly once, so per-write code never re-opens it.
- Implement `impl TokenWriter for CapStdTokenWriter` with the following
  algorithm in `write`:
  1. Reuse the per-writer root `Dir` cached at construction; do not re-open.
  2. Ensure the `podbot` subdirectory exists at mode `0700` via
     `dir_ops::ensure_dir_with_mode(&root, "podbot", 0o700)`, which calls
     `DirBuilder::new().mode(0o700).recursive(false).create(...)`,
     `set_permissions(Permissions::from_mode(0o700))`, and then reads back
     `metadata().permissions().mode() & 0o777` to assert. Mismatches return
     `FilesystemError::PermissionDenied { path }`.
  3. Ensure the per-container directory exists with the same discipline.
  4. Before writing the new temp file, sweep stale `ghapp_token.tmp.*`
     siblings via `Dir::entries` and unlink each (best-effort, logged at
     debug). This bounds the failure-mode growth.
  5. Within the per-container `Dir`, open a temporary file with a random
     suffix (`ghapp_token.tmp.<rand>`) using
     `OpenOptions::new().write(true).create_new(true).mode(0o600)`, install
     an RAII guard that unlinks the temp name on early return, and write
     `token.as_bytes()`. Call `sync_all()` on the file handle.
  6. Set permissions on the temp file to `0o600` explicitly, read back the
     mode, and abort on mismatch.
  7. Call `Dir::rename(temp_name, &per_container_dir, "ghapp_token")` to
     publish atomically. On success, disarm the RAII guard.
  8. Open the per-container `Dir` and `sync_all()` it to flush the rename
     metadata for in-session durability.
  9. Return `RuntimeDir::from(root_path.join("podbot").join(
     container.as_str()))`.
- Implement `pub(crate) fn cleanup(&self, container: &ContainerId) ->
  Result<()>` to remove `ghapp_token`, any `ghapp_token.tmp.*` siblings, and
  the per-container directory if present, ignoring missing entries
  idempotently.
- Map every `io::Error` to the most specific `FilesystemError` variant via a
  helper in `dir_ops::classify_io_error` that returns
  `FilesystemError::NotFound` for `ErrorKind::NotFound`,
  `FilesystemError::PermissionDenied` for `ErrorKind::PermissionDenied`,
  and `FilesystemError::IoError` otherwise. The helper preserves the
  original `io::Error` as a `source` so callers can downcast and inspect
  `raw_os_error() == Some(libc::EXDEV)` without parsing strings. If the
  current `FilesystemError::IoError` shape lacks a `source` field, treat the
  addition as an *additive* `#[from]`-only change; do not change existing
  field names.
- Emit `tracing::info!` on successful write keyed by `container_id`, `mode`,
  and the resolved per-container directory path; emit `tracing::warn!` only
  when XDG fallback or mode-mismatch healing occurs. Do not log token
  bytes; the `TokenMaterial` parameter is opaque and its `as_bytes`
  accessor is only called inside the `write` algorithm.
- Emit `metrics::counter!("podbot.token_daemon.write.total", "status" =>
  …)`, `metrics::histogram!("podbot.token_daemon.write.latency_seconds")`,
  `metrics::counter!("podbot.token_daemon.cleanup.total", "status" => …)`,
  and `metrics::counter!("podbot.token_daemon.cleanup.failures")` so
  operators can detect rotation regressions. Mirror the
  `podbot.github.installation_token.*` naming idiom from 3.2.1.
- Add a `clippy::disallowed_methods` entry banning
  `crate::github::installation_token::InstallationAccessToken::token`
  outside `src/token_daemon/`, so a future change to another module cannot
  smuggle a token string into a `tracing::info!` field.
- Unit tests cover: file created with mode `0600`; directory created with
  mode `0700`; `defeats_umask_022` and `defeats_umask_077`; rename replaces
  existing file with new contents; stale temp files are swept; cleanup is
  idempotent; redaction (a sentinel token byte sequence is absent from any
  captured tracing field or formatted error); a concurrency stress test
  with one writer thread and four reader threads iterating 200 times over a
  `tempfile::TempDir` proves no reader sees an empty or partial file.
- Acceptance for M3: stress test passes ten consecutive runs; lint warnings
  resolved in code, not via `#[allow(...)]`; every file under 400 lines;
  `coderabbit review --agent` reports no actionable findings.

### Stage D: behavioural coverage (M4)

Add `rstest-bdd` scenarios in `tests/features/token_daemon_runtime_dir.feature`,
`tests/bdd_token_daemon_runtime_dir.rs`, and a
`tests/bdd_token_daemon_runtime_dir_helpers/` directory mirroring the layout
used by the GitHub installation-token tests (see
`tests/bdd_github_installation_token.rs`). Compose only the M1 port and the M3
adapter; no production code changes should be needed in this stage.

Required scenarios:

- happy path: configured XDG runtime root, valid container ID, writer
  publishes `ghapp_token` with mode `0600` in a directory with mode `0700`;
- fallback: XDG unset, fallback enabled, writer emits a warning and uses
  `/tmp/podbot-<uid>`;
- permission denied: writer reports `FilesystemError::PermissionDenied` when
  the runtime root is owned by another user or has unsafe modes;
- invalid container ID: port rejects path-traversal and empty input before
  any filesystem work occurs;
- rename across filesystems: explicit `EXDEV` mapping when the override root
  is on a different mount, simulated via a controlled `OnceErrorWriter` test
  double or skipped with a documented reason when an EXDEV environment is
  unavailable;
- cleanup: cleanup removes a previously written directory and is idempotent.

Acceptance for M4: all scenarios green; `make markdownlint` clean for the new
feature file; `make test` green.

### Stage E: documentation and roadmap (M5)

Update the design document, developers' guide, users' guide, roadmap, and
this ExecPlan. Specifically:

- `docs/podbot-design.md` §Token management gains a one-paragraph note that
  3.3.1 introduces the host-side `TokenWriter` port (returning `RuntimeDir`,
  not a file path) and the `cap_std`-backed adapter at `src/token_daemon/`,
  retaining the existing pseudocode as the abstract algorithm. The same
  section gains an explicit, screen-reader-friendly rule that 3.4.1 must
  bind-mount the per-container *directory* read-only at
  `/run/secrets/`, never the file directly, because Linux file bind mounts
  pin a dentry at mount time and would leave the container with stale token
  bytes after each rotation. §Security model gains a cross-reference to
  this rule. Reference this ExecPlan. Capture this rule as ADR-NNN if the
  design doc maintainers consider it substantive enough to record alongside
  the existing ADRs; the developers' guide carries the engineering
  rationale either way.
- `docs/developers-guide.md` gains a §13.x subsection describing the
  `TokenWriter` port, the `token_daemon` module layout, the umask-defeating
  mode discipline, and the EXDEV gotcha. Document the test helper layout
  under `tests/bdd_token_daemon_runtime_dir_helpers/`.
- `docs/users-guide.md` gains a short note that operators will see a
  `xdg_runtime_dir_missing` warning when `$XDG_RUNTIME_DIR` is unset and the
  daemon falls back to `/tmp/podbot-<uid>`.
- `docs/podbot-roadmap.md` ticks item 3.3.1 with the existing checkbox style
  only after every gate passes.
- If a design decision in this slice is substantive enough to warrant an
  ADR (for example, the bind-mount-the-directory rule), capture it in
  `docs/adr-NNN-...` and reference it from the design document, per the
  documentation style guide.

Acceptance for M5: `make markdownlint`, `make nixie`, `make check-fmt`,
`make lint`, and `make test` all green; `coderabbit review --agent` reports
no actionable findings; the draft PR description references this ExecPlan and
the lody session.

## Concrete steps

Run all commands from the repository root inside this worktree:

```plaintext
/home/leynos/.lody/repos/github---leynos---podbot/worktrees/c8ebf87e-0570-4132-ace8-535bca052986
```

Confirm the branch before editing:

```sh
git branch --show-current
```

Expected output once renaming is complete:

```plaintext
3-3-1-token-daemon-runtime-directory-and-writer
```

Refresh navigation context:

```sh
leta workspace add "$PWD"
leta files src/
```

If symbol lookup resolves, prefer `leta show TokenWriter`, `leta refs
InstallationAccessToken`, and `leta calls --to write_atomic` over raw greps.
Record any indexing miss in `Surprises and discoveries`.

Establish a green baseline before red tests:

```sh
make test 2>&1 | tee /tmp/test-podbot-3-3-1-token-daemon-runtime-directory-and-writer-baseline.out
```

For each milestone, after edits:

```sh
make check-fmt 2>&1 | tee /tmp/check-fmt-podbot-3-3-1-token-daemon-runtime-directory-and-writer-MNN.out
make lint 2>&1 | tee /tmp/lint-podbot-3-3-1-token-daemon-runtime-directory-and-writer-MNN.out
make test 2>&1 | tee /tmp/test-podbot-3-3-1-token-daemon-runtime-directory-and-writer-MNN.out
```

After documentation edits:

```sh
make markdownlint 2>&1 | tee /tmp/markdownlint-podbot-3-3-1-token-daemon-runtime-directory-and-writer.out
make fmt 2>&1 | tee /tmp/fmt-podbot-3-3-1-token-daemon-runtime-directory-and-writer.out
make nixie 2>&1 | tee /tmp/nixie-podbot-3-3-1-token-daemon-runtime-directory-and-writer.out
```

Run CodeRabbit after each milestone:

```sh
coderabbit review --agent
```

Commit per milestone with a HEREDOC body via `git commit -F` and push to
`origin/3-3-1-token-daemon-runtime-directory-and-writer` so the draft PR
tracks progress.

## Validation and acceptance

Acceptance is behavioural and observable, not merely structural. The slice is
complete when every item below holds.

- A unit test proves `ContainerId::parse` rejects empty, path-traversal,
  separator, NUL, control-character, and over-length inputs, and accepts a
  representative 64-character lowercase hex OCI ID.
- A unit test proves the XDG resolver returns the configured root when XDG
  is valid, rejects relative or non-`0700`/non-owned XDG values, and falls
  back to `/tmp/podbot-<uid>` with exactly one `tracing::warn!` keyed by
  `xdg_runtime_dir_missing = true`.
- A unit test proves the writer creates the per-container directory at mode
  `0700` and the token file at mode `0600`, even under umask `0o022` and
  `0o077`.
- A unit test proves the writer replaces an existing token file via rename
  and that a concurrent reader thread observes either the previous or the
  new content in full, never empty or partial bytes, across at least 200
  iterations.
- A unit test proves the writer's errors and tracing fields do not contain a
  sentinel token value used to seed the test.
- A behavioural scenario proves the happy path end-to-end against the port.
- A behavioural scenario proves the XDG-unset fallback emits the warning and
  uses `/tmp/podbot-<uid>`.
- A behavioural scenario proves invalid container IDs are rejected before
  any filesystem mutation occurs.
- A behavioural scenario proves cleanup removes the per-container directory
  and is idempotent.
- `docs/podbot-design.md`, `docs/users-guide.md`, and
  `docs/developers-guide.md` record the new module layout, the umask
  discipline, and the bind-mount-the-directory rule for 3.4.1.
- `docs/podbot-roadmap.md` marks item 3.3.1 complete.
- `make check-fmt`, `make lint`, and `make test` all pass.
- `make markdownlint`, `make fmt`, and `make nixie` all pass on touched
  documentation. Pre-existing markdownlint failures in unrelated files are
  recorded in `Surprises and discoveries` rather than fixed in this slice.
- `coderabbit review --agent` reports no actionable findings on the final
  branch state.

Quality method:

- For each gate, capture output with `tee` to a `/tmp/*-3-3-1-*.out` log so
  reviewers can audit failures after the fact.
- Run gates sequentially, not in parallel, to honour the shared Cargo cache.

## Idempotence and recovery

All stages are additive. Tests can be re-run without cleanup; the writer's
own tests work against fresh `tempfile::TempDir` directories so the host
filesystem is not mutated. `cleanup` is idempotent by design.

If a gate fails, inspect the corresponding `/tmp/*podbot-3-3-1*` log, make a
minimal fix, append a line to `Progress` recording the failure and fix, and
rerun only the failed gate before continuing.

If `coderabbit review --agent` reports findings that conflict with this
ExecPlan (for example, a recommendation to drop `cleanup` from the port),
record the conflict in `Decision log` and ask for direction rather than
silently changing the port.

If `EXDEV` cannot be simulated in the test environment, skip the EXDEV BDD
scenario with `#[ignore]` and a `tracing::warn!` documenting the gap; do not
remove the production error mapping.

## Artifacts and notes

Research evidence collected during planning:

- XDG Base Directory Specification confirms `$XDG_RUNTIME_DIR` ownership and
  mode requirements, and the warn-and-fall-back policy when unset
  (`https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html`).
- `rename(2)` on Linux is atomic with respect to a reader of `newpath` when
  source and destination share a filesystem
  (`https://man7.org/linux/man-pages/man2/rename.2.html`).
- `cap_std::fs_utf8::Dir::rename` is the cap-std-aligned replacement for
  `std::fs::rename` and avoids parent-directory races
  (`https://docs.rs/cap-std/latest/cap_std/fs/struct.Dir.html`).
- The Docker bind-mount-of-a-file footgun is documented widely: bind mounting
  a file captures the dentry, so renames over the host path leave containers
  with stale data. The mitigation is to bind mount the *directory*
  (`https://unix.stackexchange.com/questions/537095/`,
  `https://stackoverflow.com/questions/53547973/`).
- Prior art for the temp-then-rename pattern: HashiCorp Vault Agent token
  sink, systemd credentials, Kubernetes projected SA tokens. Each enforces a
  `0700` directory and `0600`/`0400` files
  (`https://systemd.io/CREDENTIALS/`,
  `https://kubernetes.io/docs/concepts/storage/projected-volumes/`).

Architecture review notes (Plan agent, 2026-06-06):

```plaintext
Port lives in api::token_writer (internal-gated); adapter lives in
src/token_daemon/ with runtime_dir.rs and atomic_writer.rs; rename via
cap_std::fs_utf8::Dir; no new dependencies; cleanup is a method on the
trait; no Drop guard; Linux-only with a cfg gate.
```

## Interfaces and dependencies

New public-but-internal-gated symbols in `src/api/token_writer.rs`:

```rust
#[derive(Clone, Eq, PartialEq)]
pub struct ContainerId(String);

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RuntimeDir(camino::Utf8PathBuf);

/// Opaque token byte payload. Manual redacted `Debug`.
#[derive(Clone, Copy)]
pub struct TokenMaterial<'a>(&'a [u8]);

pub trait TokenWriter: Send + Sync {
    /// Publish `token` for `container` and return the per-container
    /// directory whose `ghapp_token` entry now holds the new bytes.
    ///
    /// The returned `RuntimeDir` is intended for bind-mount-by-directory
    /// only; consumers compose `.join("ghapp_token")` themselves.
    ///
    /// Blocking. Async callers must use `tokio::task::spawn_blocking`.
    fn write(
        &self,
        container: &ContainerId,
        token: TokenMaterial<'_>,
    ) -> crate::error::Result<RuntimeDir>;
}
```

`ContainerId::parse` rejects: empty, contains `/`, contains `\\`, equals
`..`, contains `\0`, contains an ASCII control character, or has trimmed
length outside `8..=64`. The range admits both 12-character short OCI
container IDs and 64-character full ones, matching the values bollard hands
back from container create and inspect operations. `TokenMaterial` has no
public constructor outside `src/token_daemon/`; the only producer in this
slice is the adapter, which converts an `&InstallationAccessToken` to a
`TokenMaterial` at the boundary. Tests that need a stub writer construct
`TokenMaterial` through a `#[cfg(test)]` helper.

New internal symbols in `src/token_daemon/`:

```rust
pub(crate) struct RuntimeDirPolicy {
    pub(crate) override_root: Option<camino::Utf8PathBuf>,
    pub(crate) allow_fallback: bool,
}

pub(crate) trait EnvSource: Send + Sync {
    fn var(&self, key: &str) -> Option<String>;
}

pub(crate) fn resolve_root(
    policy: &RuntimeDirPolicy,
    env: &dyn EnvSource,
    uid: u32,
) -> crate::error::Result<camino::Utf8PathBuf>;

pub(crate) struct CapStdTokenWriter { /* ... */ }

impl CapStdTokenWriter {
    pub(crate) fn new(policy: RuntimeDirPolicy) -> crate::error::Result<Self>;
}

impl crate::api::token_writer::TokenWriter for CapStdTokenWriter { /* ... */ }
```

No new production dependencies. Existing `cap-std`, `camino`, `tracing`, and
`metrics` cover capability filesystem access, path types, observability, and
counters. The token-bytes-never-leave-the-write-site rule is enforced at the
type level by accepting `&InstallationAccessToken` rather than a raw string.

Module wiring in `src/lib.rs`:

```rust
#[cfg(any(feature = "internal", test))]
#[doc(hidden)]
pub mod token_daemon;
#[cfg(not(any(feature = "internal", test)))]
mod token_daemon;
```

The `token_daemon` module is the composition root for the slice; it depends
on `api::token_writer` for the port and `github::installation_token` for the
token type.

## Progress

- [x] (2026-06-06T00:00:00Z) Loaded `leta`, `hexagonal-architecture`,
  `rust-router`, and `execplans` skills; created the leta workspace for the
  worktree.
- [x] (2026-06-06T00:00:00Z) Reviewed roadmap item 3.3.1, design §Token
  management, the prerequisite 3.2.1 and 2.2.5 ExecPlans, `src/lib.rs`,
  `src/api/mod.rs`, `src/github/installation_token.rs`, `src/error.rs`, and
  the cap-std patterns in `src/engine/connection/upload_credentials/`.
- [x] (2026-06-06T00:00:00Z) Ran Firecrawl research on XDG runtime dir,
  rename atomicity, cap-std API, and the file-bind-mount footgun.
- [x] (2026-06-06T00:00:00Z) Ran a Plan-agent architecture pass producing
  the M1-M5 milestone decomposition this document follows.
- [x] (2026-06-06T00:00:00Z) Drafted this ExecPlan and requested approval.
- [x] (2026-06-06T00:00:00Z) Ran a Logisphere community-of-experts design
  review; applied the resulting revisions: `RuntimeDir` return type,
  `TokenMaterial` domain type, `cleanup` off the port, sync port with
  spawn_blocking guidance, `allow_fallback = false` default, XDG_STATE_HOME
  preference, runtime root `Dir` cached at construction, `io::ErrorKind`
  preserved through error mapping, stale temp-file sweep, pre-emptive
  submodule split, `clippy::disallowed_methods` for token accessor,
  bind-mount-the-directory rule promoted to the design document.
- [ ] Receive explicit user approval to proceed with implementation.
- [ ] M1: red tests, port, and newtypes land with green gates.
- [ ] M2: XDG resolver and `EnvSource` land with green gates.
- [ ] M3: `CapStdTokenWriter` adapter, redaction tests, and stress test land
  with green gates and a clean CodeRabbit review.
- [ ] M4: rstest-bdd scenarios land with green gates and a clean CodeRabbit
  review.
- [ ] M5: design, developers' guide, users' guide, roadmap, and ADR (if
  needed) land; final gates and CodeRabbit pass.
- [ ] Branch renamed and tracking
  `origin/3-3-1-token-daemon-runtime-directory-and-writer`; draft PR open.

## Surprises and discoveries

- Observation: bind mounting the token *file* into the sandbox would defeat
  atomic rotation because Linux bind mounts capture a dentry at mount time;
  the new inode after rename would not be visible inside the container.
  Evidence: kernel bind-mount semantics, multiple Docker/Stack Overflow
  reports. Impact: the design document and developers' guide must instruct
  3.4.1 to bind mount the per-container *directory* read-only and resolve
  `ghapp_token` inside the container.
- Observation: `cap_std`'s file create still honours the process umask
  despite an `OpenOptions::mode(0o600)` request. Evidence: cap-std and
  POSIX documentation. Impact: the writer must explicitly `set_permissions`
  to `0o600` and verify the resulting mode, rather than relying on the
  create-time mode argument alone.
- Observation: pre-existing `make fmt` runs surface markdownlint failures in
  unrelated documentation across the repository (recorded in 3.2.1's
  surprises). Impact: keep `make fmt` confined to touched files where the
  Makefile allows it; otherwise record and restore unrelated formatter
  churn.

## Decision log

- Decision: keep this ExecPlan in `Status: DRAFT` and require explicit user
  approval before implementation. Rationale: the user instructed that the
  plan must be approved before implementation, matching the execplans
  approval gate. Date/Author: 2026-06-06T00:00:00Z / Claude.
- Decision: place the port in `src/api/token_writer.rs` under a gated
  re-export and the adapter in a new `src/token_daemon/` directory module.
  Rationale: keeps the hexagonal dependency rule honest, mirrors the
  prevailing layout (`engine`, `github`), and lets 3.3.2 wire the adapter
  through `run_token_daemon` without altering the port. Date/Author:
  2026-06-06T00:00:00Z / Claude, informed by the Plan-agent review.
- Decision: rely on `cap_std::fs_utf8::Dir::rename` plus per-write temp
  files with random suffixes for atomicity; reject adding `tempfile`,
  `atomicwrites`, `cap-tempfile`, or `nix` as a production dependency.
  Rationale: keeps the dependency surface tight, matches the rest of the
  codebase, and avoids the `std::fs::rename` race the alternatives carry.
  Date/Author: 2026-06-06T00:00:00Z / Claude.
- Decision: enforce mode bits with explicit `set_permissions` plus a
  read-back assertion, rather than trusting the create-time mode argument.
  Rationale: cap-std honours umask, so a naive create can leave the file at
  `0644`. Date/Author: 2026-06-06T00:00:00Z / Claude.
- Decision: fall back to `/tmp/podbot-<uid>` with a `tracing::warn!` when
  `$XDG_RUNTIME_DIR` is unset; do not hard-error. Rationale: the XDG spec
  recommends fallback with a warning, and operators who run podbot in
  minimal environments without a session bus should not be blocked.
  Date/Author: 2026-06-06T00:00:00Z / Claude.
- Decision: skip property tests, Kani harnesses, and Verus proofs for this
  slice unless the `ContainerId` parser grows beyond a single predicate.
  Rationale: rename atomicity is a kernel guarantee, not an in-process
  invariant; mode-bit policy is a small finite set; redaction is best
  expressed as a single sentinel test. Date/Author: 2026-06-06T00:00:00Z /
  Claude.
- Decision: defer cleanup-on-container-stop, refresh loop, and bind-mount
  wiring to 3.3.2 and 3.4.1. Rationale: the roadmap explicitly scopes 3.3.1
  to the runtime directory and atomic writer. Date/Author:
  2026-06-06T00:00:00Z / Claude.
- Decision: revise the port to return `RuntimeDir` (the per-container host
  directory) rather than `TokenFilePath`. Rationale: Linux file bind mounts
  pin a dentry at mount time, so a future 3.4.1 mount of the file would
  freeze readers on the pre-rotation inode. Returning the directory makes
  the only safe mount shape the only available one. Date/Author:
  2026-06-06T00:00:00Z / Claude, following Logisphere design review.
- Decision: remove `cleanup` from the port and keep it on the adapter as
  `pub(crate) fn cleanup`. Rationale: no caller in 3.3.1 needs to invoke
  cleanup through the port; placing it on the port now risks redesign in
  3.3.2 when refresh-loop teardown lands. Date/Author: 2026-06-06T00:00:00Z
  / Claude, following Logisphere design review.
- Decision: declare the port synchronous. Rationale: the underlying
  filesystem operations are blocking syscalls; an `async` signature would
  mislead callers into thinking the writer yields. Async drivers must wrap
  calls with `tokio::task::spawn_blocking`. Date/Author:
  2026-06-06T00:00:00Z / Claude, following Logisphere design review.
- Decision: introduce a domain-owned `TokenMaterial<'_>` byte-slice wrapper
  instead of accepting `&InstallationAccessToken` at the port. Rationale:
  the port lives in `api`, the token type lives in `github` (an adapter);
  depending on it from the port reverses the hex dependency rule. The
  adapter converts at the boundary, and the redacted `Debug` plus a
  `clippy::disallowed_methods` rule guard against token leakage.
  Date/Author: 2026-06-06T00:00:00Z / Claude, following Logisphere design
  review.
- Decision: default `RuntimeDirPolicy::allow_fallback` to `false`, and when
  enabled prefer `$XDG_STATE_HOME/podbot/runtime/` before `/tmp/podbot-<uid>`.
  Rationale: `systemd-tmpfiles` can wipe `/tmp/podbot-*` entries during a
  write, producing intermittent auth failures with no operator-visible
  cause. Defaulting to no fallback forces operators to make an explicit
  trade-off; XDG_STATE_HOME survives sessions and is not subject to
  tmpfiles policy. Date/Author: 2026-06-06T00:00:00Z / Claude, following
  Logisphere design review.
- Decision: keep `$XDG_RUNTIME_DIR` as the default runtime root rather than
  `$XDG_STATE_HOME`. Rationale: tokens are short-lived (about one hour); the
  tmpfs-backed runtime path is wiped on logout, which is desirable hygiene
  for transient secrets, and the design document already commits to
  `/run/secrets/ghapp_token` semantics inside the container. Date/Author:
  2026-06-06T00:00:00Z / Claude, following Logisphere alternatives review.
- Decision: cache the runtime root `Dir` inside `CapStdTokenWriter` for the
  writer's lifetime. Rationale: re-opening the root on every write costs a
  syscall and confuses lifecycle (the policy was applied once at
  construction; reapplying it implicitly per write is a footgun if env
  changes). Date/Author: 2026-06-06T00:00:00Z / Claude, following
  Logisphere scaling review.
- Decision: classify `io::Error` into `FilesystemError` variants and
  preserve the source `io::Error` chain (via an additive `#[from]` source
  field on `FilesystemError::IoError` if one does not exist today) rather
  than collapsing to a string `message`. Rationale: 3.3.2's retry loop
  needs to distinguish `ErrorKind::Interrupted` from
  `ErrorKind::PermissionDenied`; string parsing is brittle. Date/Author:
  2026-06-06T00:00:00Z / Claude, following Logisphere failure-mode review.
- Decision: sweep `ghapp_token.tmp.*` siblings before each write and unlink
  the active temp file on the error path via an RAII guard. Rationale:
  bounds disk usage in failure paths and prevents stale state from
  accumulating across crashes. Date/Author: 2026-06-06T00:00:00Z / Claude,
  following Logisphere failure-mode review.
- Decision: split the adapter into `atomic_writer.rs`, `dir_ops.rs`, and
  `mode.rs` from the first commit, rather than waiting for the 400-line
  ceiling to bite. Rationale: pre-emptive split matches the codebase's
  established pattern (engine submodules, github submodules) and makes
  reviewer attention easier to direct. Date/Author: 2026-06-06T00:00:00Z /
  Claude, following Logisphere viability review.
- Decision: enforce the no-token-leak rule with a
  `clippy::disallowed_methods` configuration banning
  `InstallationAccessToken::token` outside `src/token_daemon/`. Rationale:
  a redaction test catches regressions only on the paths it covers; the
  lint catches every future path. Date/Author: 2026-06-06T00:00:00Z /
  Claude, following Logisphere pre-mortem.

## Outcomes and retrospective

To be completed after implementation. At minimum, record: what shipped,
which tests gate the slice, which 3.3.2 and 3.4.1 hooks are now ready,
which CodeRabbit recommendations were applied or deferred, and any
unexpected complexity surfaced by the umask, EXDEV, or XDG paths.

## Revision note

- 2026-06-06: Initial draft created for user approval. The plan records
  Firecrawl research findings on XDG, rename atomicity, cap-std, and the
  bind-mount footgun; the Plan-agent architectural review proposing the
  port location, adapter module layout, mode discipline, and the cleanup
  seam; the testing strategy across rstest, rstest-bdd, and a concurrency
  stress test; and the five-milestone delivery plan with explicit gates.
  No implementation has begun.
- 2026-06-06: Logisphere community-of-experts design review applied.
  Changes: port now returns `RuntimeDir` (not `TokenFilePath`) so 3.4.1
  cannot accidentally bind-mount the file; introduced a domain-owned
  `TokenMaterial<'_>` byte wrapper so the port stops depending on
  `crate::github`; removed `cleanup` from the port (kept on the adapter);
  declared the port synchronous with a `spawn_blocking` rustdoc note;
  defaulted `RuntimeDirPolicy::allow_fallback` to `false` and ordered the
  fallback chain as `XDG_STATE_HOME` then `/tmp/podbot-<uid>`; cached the
  runtime root `Dir` for the writer's lifetime; preserved `io::ErrorKind`
  through the error-mapping helper; added a stale-temp-file sweep and an
  RAII guard for the active temp file; widened `ContainerId` length to
  `8..=64` to admit OCI short IDs; pre-emptively split the adapter into
  `atomic_writer.rs`, `dir_ops.rs`, and `mode.rs`; added a
  `clippy::disallowed_methods` rule for `InstallationAccessToken::token`;
  promoted the bind-mount-the-directory rule from the developers' guide
  to the design document; reused `mockable` for env injection where
  possible; declared explicit metric names mirroring the github adapter.
  No implementation has begun.
