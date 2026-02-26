# Step 3.1.1: Load the private key from the configured path

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as
work proceeds.

Status: IN PROGRESS

## Purpose and big picture

Enable podbot to read a PEM-encoded RSA private key from the filesystem path
specified in `GitHubConfig.private_key_path`. After this change, calling
`podbot::github::load_private_key(&path)` with a valid PEM file returns a
`jsonwebtoken::EncodingKey` suitable for passing to
`OctocrabBuilder::app(app_id, key)`. Invalid or missing files produce a
`GitHubError::PrivateKeyLoadFailed` with a clear diagnostic message.

Non-RSA keys (Ed25519, ECDSA) are rejected at load time with an actionable
error rather than deferring failure to JWT signing, because octocrab v0.49.5
hardcodes `Algorithm::RS256` and GitHub's API only supports RS256 for App
authentication.

Observable outcome: unit tests and BDD scenarios exercise happy and unhappy
paths. Running `make test` shows the new tests passing. The function is not
yet wired into the orchestration flow (that is Step 3.1, tasks 2-4).

## Constraints

- Do not modify existing engine, config, or error modules beyond registering
  the new `github` module in `src/lib.rs`.
- Do not add `unsafe` code.
- Do not use `unwrap` or `expect` outside test code.
- Maintain `missing_docs = "deny"` compliance.
- Keep all files under 400 lines.
- Use en-GB spelling in documentation.
- Use `cap_std::fs_utf8` and `camino` for filesystem operations in production
  code.
- Use `rstest` for unit tests and `rstest-bdd` v0.5.0 for behavioural tests.
- The `jsonwebtoken` dependency must match the version already in
  `Cargo.lock` (v10.2.0 via octocrab) to avoid duplicate compilation.
- Only RSA keys are accepted. Ed25519 and ECDSA PEM files must be rejected
  at load time with a clear error stating that GitHub App authentication
  requires an RSA private key. This prevents a deferred failure at JWT
  signing where octocrab hardcodes RS256.

## Tolerances (exception triggers)

- Scope: if implementation requires changes to more than 16 files or more
  than 600 net lines, stop and confirm scope.
- Interface: if the existing `GitHubError::PrivateKeyLoadFailed` variant
  needs field changes, stop and confirm.
- Dependencies: only `jsonwebtoken` may be added as a direct dependency. If
  another crate is needed, stop and confirm.
- Iterations: if `make lint` or `make test` still fails after three fix
  passes, stop and record blocker evidence.
- Ambiguity: if octocrab's `EncodingKey` expectations differ from
  `jsonwebtoken::EncodingKey::from_rsa_pem`, stop and investigate.

## Risks

- Risk: `jsonwebtoken` version drift between octocrab's transitive
  dependency and our direct dependency could cause duplicate compilation or
  type mismatches.
  Severity: medium. Likelihood: low.
  Mitigation: pin to exactly `10.2.0` with `features = ["use_pem"]`,
  matching the existing `Cargo.lock` entry.

- Risk: `EncodingKey` does not implement `Debug` or `Clone`, complicating
  storage or logging.
  Severity: low. Likelihood: medium.
  Mitigation: return by value; callers store directly. If needed later, wrap
  in a newtype with manual `Debug`.

- Risk: BDD integration tests in `tests/` cannot access `pub(crate)`
  modules.
  Severity: medium. Likelihood: certain.
  Mitigation: register `github` as `pub mod github;` in `lib.rs` with
  documentation noting it is internal and subject to change.

- Risk: test RSA key generation requires `openssl` to be available in the
  build environment.
  Severity: low. Likelihood: low.
  Mitigation: generate once, commit the fixture file. The key is test-only
  with no security value.

## Progress

- [x] (2026-02-25 UTC) Drafted this ExecPlan.
- [ ] Add `jsonwebtoken` dependency to `Cargo.toml`.
- [ ] Generate test key fixtures (RSA, EC, Ed25519) in `tests/fixtures/`.
- [ ] Create `src/github.rs` with `load_private_key` and helpers.
- [ ] Register `pub mod github;` in `src/lib.rs` and update module docs.
- [ ] Add unit tests in `src/github.rs` (happy, unhappy, edge cases).
- [ ] Run code quality gates: `make check-fmt`, `make lint`, `make test`.
- [ ] Commit core implementation.
- [ ] Create BDD feature file `tests/features/github_private_key.feature`.
- [ ] Create BDD harness and helper modules.
- [ ] Run quality gates again.
- [ ] Commit BDD tests.
- [ ] Update `docs/podbot-design.md` with private key loading contract.
- [ ] Update `docs/users-guide.md` with key file requirements.
- [ ] Mark roadmap task as done in `docs/podbot-roadmap.md`.
- [ ] Run documentation gates: `make markdownlint`, `make fmt`,
  `make nixie`.
- [ ] Commit documentation updates.
- [ ] Finalise outcomes and retrospective.

## Surprises and discoveries

(To be populated during implementation.)

## Decision log

- Decision: only accept RSA private keys; reject Ed25519 and ECDSA at load
  time with a clear error message.
  Rationale: octocrab v0.49.5 hardcodes `Algorithm::RS256` in `create_jwt`
  (`auth.rs:85`). GitHub's API only supports RS256 for App authentication.
  Loading a non-RSA key would succeed at read time but fail later at JWT
  signing with a cryptic `InvalidAlgorithm` error. Failing fast with an
  actionable message ("GitHub App authentication requires an RSA private
  key") is a better user experience.
  Date/Author: 2026-02-25 / DevBoxer.

- Decision: register `github` as `pub mod github;` in `lib.rs` despite the
  design document listing it as internal.
  Rationale: BDD integration tests in `tests/` can only access `pub` items
  from the crate. The module is documented as unstable and subject to
  change. This follows the same pattern as `engine` and `config`.
  Date/Author: 2026-02-25 / DevBoxer.

## Outcomes and retrospective

(To be populated on completion.)

## Context and orientation

Podbot is a Rust application (edition 2024, MSRV 1.88) that creates secure
containers for AI coding agents. The project is structured as a dual-delivery
library and CLI binary.

Key files for this task:

- `src/lib.rs` (24 lines): library entry point, currently exports `config`,
  `engine`, `error`. The new `github` module will be registered here.

- `src/config/types.rs` (241 lines): defines `GitHubConfig` with fields
  `app_id: Option<u64>`, `installation_id: Option<u64>`,
  `private_key_path: Option<Utf8PathBuf>`. Already has `validate()` and
  `is_configured()` methods.

- `src/error.rs` (467 lines): defines `GitHubError::PrivateKeyLoadFailed {
  path: PathBuf, message: String }`. This variant already exists and
  propagates through `PodbotError` via `#[from]`.

- `Cargo.toml`: lists `octocrab = "0.49.5"` which transitively depends on
  `jsonwebtoken = "10.2.0"` with `use_pem` feature. The `jsonwebtoken`
  crate is not yet a direct dependency.

- `src/engine/connection/upload_credentials/mod.rs` (302 lines):
  demonstrates the `cap_std::fs_utf8::Dir` + `ambient_authority()` pattern
  for capability-oriented filesystem access. The `open_host_home_dir`
  method (line 143) shows the idiomatic approach.

- `tests/bdd_error_handling.rs` and `tests/bdd_credential_injection*`:
  established patterns for BDD tests using `rstest-bdd` v0.5.0 with
  `ScenarioState`, `Slot<T>`, and the `#[scenario]` macro.

Octocrab's `OctocrabBuilder::app()` accepts
`(AppId, jsonwebtoken::EncodingKey)`. Critically, octocrab's `create_jwt`
function hardcodes `Algorithm::RS256` (`auth.rs:85`) and validates that the
key family matches the algorithm at encode time. This means only RSA keys
work; Ed25519 and ECDSA keys would trigger `ErrorKind::InvalidAlgorithm` at
JWT signing. `EncodingKey::from_rsa_pem(bytes)` parses PEM-encoded RSA keys
and validates the format. This is the function we will use, with an
additional PEM header check to detect and reject non-RSA key types with a
clear error before reaching `from_rsa_pem`.

## Agent team and ownership

Implementation uses a single integrator agent that:

- owns all new files (`src/github.rs`, test harnesses, fixtures);
- updates `src/lib.rs` and `Cargo.toml` for module registration and
  dependency additions;
- updates documentation files (`podbot-design.md`, `users-guide.md`,
  `podbot-roadmap.md`);
- runs quality gates and commits each logical slice.

## Plan of work

### Stage A: Dependency and fixture setup

Add `jsonwebtoken` as a direct dependency in `Cargo.toml` under
`[dependencies]`:

```toml
jsonwebtoken = { version = "10.2.0", default-features = false, features = ["use_pem"] }
```

Generate test key fixtures (committed to the repository; test-only, no
security value):

```bash
mkdir -p tests/fixtures
openssl genrsa 2048 > tests/fixtures/test_rsa_private_key.pem
openssl ecparam -genkey -name prime256v1 -noout \
    > tests/fixtures/test_ec_private_key.pem
openssl genpkey -algorithm ed25519 \
    > tests/fixtures/test_ed25519_private_key.pem
```

Validation: `cargo check` succeeds. `Cargo.lock` does not add a second
`jsonwebtoken` entry.

### Stage B: Core implementation (`src/github.rs`)

Create `src/github.rs` as a new module. Register it in `src/lib.rs` as
`pub mod github;` with a note that it is internal and subject to change.
Update the module-level doc comment in `lib.rs` to list it.

The module provides one public function:

```rust
pub fn load_private_key(
    key_path: &Utf8Path,
) -> Result<EncodingKey, GitHubError>
```

Internal structure uses four private helpers:

1. `open_key_directory(key_path) -> Result<(Dir, &str), GitHubError>`:
   splits the path into parent directory and filename, opens the parent as
   a `cap_std::fs_utf8::Dir` via `ambient_authority()`.

2. `read_key_file(dir, file_name, display_path) -> Result<String,
   GitHubError>`: reads the file to a string, returns an error if the file
   is empty.

3. `validate_rsa_pem(pem_contents, display_path) ->
   Result<(), GitHubError>`: inspects the PEM header to detect non-RSA key
   types. Checks for known non-RSA PEM tags (`EC PRIVATE KEY`,
   `OPENSSH PRIVATE KEY`) and returns a targeted error. RSA keys use
   either `RSA PRIVATE KEY` (PKCS#1) or `PRIVATE KEY` (PKCS#8). Since
   `PRIVATE KEY` is ambiguous, delegate to `from_rsa_pem` for the
   definitive check.

4. `parse_rsa_pem(pem_contents, display_path) ->
   Result<EncodingKey, GitHubError>`: calls `validate_rsa_pem` first, then
   `EncodingKey::from_rsa_pem(pem_contents.as_bytes())`, mapping
   `jsonwebtoken::errors::Error` to `GitHubError::PrivateKeyLoadFailed`.

A separate private function is used for testability:

```rust
fn load_private_key_from_dir(
    dir: &Dir,
    file_name: &str,
    display_path: &Utf8Path,
) -> Result<EncodingKey, GitHubError>
```

This allows unit tests to inject a `Dir` backed by a
`tempfile::TempDir`.

All errors use `GitHubError::PrivateKeyLoadFailed { path, message }`:

- Parent directory open failure: `"failed to open parent directory:
  {io_error}"`
- File read failure: `"failed to read file: {io_error}"`
- Empty file: `"file is empty"`
- EC key detected: `"GitHub App authentication requires an RSA private
  key; the file appears to contain an ECDSA key"`
- Ed25519/OpenSSH key detected: `"GitHub App authentication requires an
  RSA private key; the file appears to contain an Ed25519 key"`
- Invalid PEM / non-RSA PKCS#8: `"invalid RSA private key: {jwt_error}"`

Validation: `make check-fmt && make lint` pass.

### Stage C: Unit tests in `src/github.rs`

Add a `#[cfg(test)] mod tests` block using `rstest` fixtures and
`tempfile::TempDir` + `cap_std::fs_utf8::Dir` for filesystem isolation.

Test cases cover happy paths, unhappy paths, and edge cases including
non-RSA key rejection. Use parameterised `#[case]` for the non-RSA key
type and invalid-content variants.

Validation: `make test` passes with all new tests green.

### Stage D: BDD tests

Create `tests/features/github_private_key.feature` with six scenarios
covering valid RSA key loading, missing file, empty file, invalid PEM,
ECDSA rejection, and Ed25519 rejection.

Create `tests/bdd_github_private_key.rs` with `#[scenario]` macro
bindings and `tests/bdd_github_private_key_helpers/` directory with state,
steps, and assertions modules.

Validation: `make test` passes with BDD scenarios green.

### Stage E: Documentation updates

1. `docs/podbot-design.md`: add private key loading contract.
2. `docs/users-guide.md`: add private key file requirements section.
3. `docs/podbot-roadmap.md`: mark first task of Step 3.1 as done.

Validation: `make markdownlint`, `make fmt`, `make nixie` all pass.

### Stage F: Quality gates and commits

Commit in three logical slices:

1. Core implementation + unit tests (Stage A + B + C).
2. BDD tests (Stage D).
3. Documentation + roadmap (Stage E).

Each commit gated with:

```bash
set -o pipefail; make check-fmt 2>&1 | tee /tmp/check-fmt-3-1-1.out
set -o pipefail; make lint 2>&1 | tee /tmp/lint-3-1-1.out
set -o pipefail; make test 2>&1 | tee /tmp/test-3-1-1.out
```

Documentation commit additionally gated with:

```bash
set -o pipefail; make markdownlint 2>&1 | tee /tmp/markdownlint-3-1-1.out
set -o pipefail; make fmt 2>&1 | tee /tmp/fmt-3-1-1.out
set -o pipefail; make nixie 2>&1 | tee /tmp/nixie-3-1-1.out
```

## Interfaces and dependencies

New direct dependency: `jsonwebtoken = { version = "10.2.0",
default-features = false, features = ["use_pem"] }`.

Public interface added to `podbot::github`:

```rust
pub fn load_private_key(
    key_path: &camino::Utf8Path,
) -> Result<jsonwebtoken::EncodingKey, crate::error::GitHubError>
```

Reuses:

- `crate::error::GitHubError::PrivateKeyLoadFailed`
  (`src/error.rs:157-164`)
- `cap_std::ambient_authority` +
  `cap_std::fs_utf8::Dir::open_ambient_dir` (pattern from
  `src/engine/connection/upload_credentials/mod.rs:143-151`)

## Validation and acceptance

Running `make test` passes and the following new tests exist:

Unit tests in `src/github.rs`:

- `tests::load_valid_rsa_key_succeeds`
- `tests::load_missing_file_returns_error`
- `tests::load_empty_file_returns_error`
- `tests::load_invalid_pem_returns_error`
- `tests::load_ec_key_returns_clear_error`
- `tests::load_ed25519_key_returns_clear_error`
- `tests::error_includes_file_path`
- `tests::load_private_key_resolves_full_path`
- `tests::load_private_key_missing_parent_returns_error`

BDD scenarios in `tests/bdd_github_private_key.rs`:

- Valid RSA private key is loaded successfully
- Missing key file produces a clear error
- Empty key file produces a clear error
- Invalid PEM content produces a clear error
- ECDSA key is rejected with a clear error
- Ed25519 key is rejected with a clear error

Quality criteria:

- `make check-fmt` passes.
- `make lint` passes (clippy with warnings denied).
- `make test` passes (all existing + new tests).
- `make markdownlint` passes.
- `make nixie` passes.
- Roadmap task `3.1 / Load the private key` is marked `[x]`.

## Idempotence and recovery

All stages are re-runnable. `cargo check` and `make test` are idempotent.
The test key fixtures can be regenerated with `openssl` if needed (RSA, EC,
Ed25519). No destructive operations are involved.
