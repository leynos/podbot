# Step 3.2.1: Acquire installation tokens with an expiry buffer

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & discoveries`,
`Decision log`, and `Outcomes & retrospective` must be kept up to date as
work proceeds.

Status: DRAFT

## Purpose and big picture

Enable Podbot to acquire a GitHub App installation access token for
`workspace.source = "github_clone"` without exposing the GitHub App private key
to the sandbox container. After this change, host-side Podbot code can mint a
short-lived token through Octocrab, return the token string for Git operations,
and carry conservative expiry metadata that later token-daemon work can use to
schedule refreshes.

The observable result is that a caller can provide configured GitHub App
credentials and an installation ID, call the new token-acquisition API, and
receive an `InstallationAccessToken` value whose secret token can be written to
a host-side token file while its non-secret expiry metadata can be logged and
used for refresh scheduling. If acquisition fails, the caller receives a
semantic `GitHubError::TokenAcquisitionFailed` message with enough context to
act, and no error or log output contains the token value.

This plan is pre-implementation. Do not implement it until the user explicitly
approves the plan.

## Constraints

- Follow roadmap item 3.2.1 in `docs/podbot-roadmap.md`: use
  `installation_token_with_buffer`, return the token string for Git
  operations, handle acquisition failures semantically, and log expiry timing
  without logging token values.
- Preserve the security boundary in `docs/podbot-design.md`: the GitHub App
  private key remains host-side, and the sandbox sees only a short-lived
  installation token through later read-only token-file work.
- Use the `leta`, `rust-router`, and `hexagonal-architecture` skills when
  implementing. Route specific Rust questions through the smallest relevant
  Rust skill, likely `rust-errors` for semantic error shape and
  `rust-async-and-concurrency` only if the acquisition API introduces async
  ownership or runtime concerns beyond the existing pattern.
- Keep domain and policy decisions separate from infrastructure details. The
  GitHub module may adapt Octocrab, but orchestration and public API code must
  not grow direct Octocrab coupling.
- Do not expose `octocrab::Octocrab`,
  `octocrab::models::InstallationId`, or `secrecy::SecretString` through
  stable public modules. Keep Octocrab types in the internal GitHub adapter
  boundary.
- Use `rstest` for unit tests and `rstest-bdd` for behavioural tests. Use
  dependency injection and `mockall` rather than real GitHub network calls.
- Do not add property tests, Kani harnesses, or Verus proofs for this slice
  unless implementation introduces a genuine invariant over a range of states
  or a proof-worthy scheduling algorithm. The expected behaviour is
  client-adapter and error-path logic that fixed examples can validate.
- Keep every Rust source file under 400 lines. `src/github/mod.rs` is already
  close enough to the limit that substantial additions should move into a new
  submodule such as `src/github/installation_token.rs`.
- Use en-GB-oxendict spelling in documentation and Rust comments, except for
  external API names.
- Run quality gates sequentially, not in parallel. Use `tee` with files under
  `/tmp` for long command output.
- Use Makefile targets rather than direct Cargo commands: `make check-fmt`,
  `make lint`, and `make test`. For documentation edits, also run
  `make markdownlint`, `make fmt`, and `make nixie` as applicable.
- Run `coderabbit review --agent` after each major implementation milestone and
  address all actionable concerns before moving to the next milestone.
- Commit each completed, gated change as an atomic commit using a file-based
  commit message via `git commit -F`.

## Tolerances

- Scope: if implementation requires changes to more than 14 files or more
  than 450 net lines of Rust code, stop and escalate with a revised
  decomposition.
- Public interface: if a stable public API signature in `podbot::api`,
  `podbot::config`, or `podbot::error` must change, stop and ask for approval.
- Dependencies: adding `chrono = "0.4.43"` as a direct dependency is allowed
  only if the implementation cannot call `installation_token_with_buffer`
  without naming `chrono::Duration`. Any other new dependency requires
  escalation.
- Octocrab limitation: if exact GitHub `expires_at` from the REST response is
  required rather than conservative metadata derived from a freshly minted
  one-hour token, stop and present options. Octocrab 0.49.5's public
  `installation_token_with_buffer` method returns a `SecretString`, not the
  raw `InstallationToken` response.
- Repository scoping: if this task must restrict the token to a single
  repository through GitHub's `repositories`, `repository_ids`, or
  `permissions` request body fields, stop and escalate. Octocrab's required
  helper sends an empty request body and therefore relies on installation-level
  repository and permission scoping.
- Test iterations: if `make lint` or `make test` still fails after three fix
  passes, stop, record the failure evidence, and ask for direction.
- Ambiguity: if multiple valid behaviours affect token exposure, expiry
  metadata, or public API shape, stop and present the trade-offs.

## Risks

- Risk: Octocrab hides exact token expiry metadata behind its cache.
  Severity: medium. Likelihood: high. Mitigation: construct a fresh
  installation client for each acquisition so `installation_token_with_buffer`
  mints a fresh token, then record `acquired_at`,
  `expires_at = acquired_at + 3600 seconds`, and
  `refresh_after = expires_at - buffer` using GitHub's documented one-hour
  token lifetime. If future refresh work needs exact server-returned
  `expires_at`, introduce a separate REST adapter only after approval because
  it would no longer rely solely on the required helper.

- Risk: Adding a direct `chrono` dependency may be needed even though Chrono
  is already present transitively through Octocrab. Severity: low.
  Likelihood: high. Mitigation: first check whether the existing Octocrab
  version exposes a re-export or helper that avoids a direct dependency. If
  not, add
  `chrono = "0.4.43"` as a direct caret dependency and document that this names
  the type required by Octocrab's public API.

- Risk: Token values could leak through debug formatting, logs, assertions, or
  error messages. Severity: high. Likelihood: medium. Mitigation: define a
  token result type without derived `Debug` for the secret token field, provide
  explicit non-secret logging helpers, and test that failure messages and
  logged events omit the token fixture value.

- Risk: Extending the existing `GitHubAppClient` trait could mix credential
  validation and token issuance responsibilities. Severity: medium. Likelihood:
  medium. Mitigation: prefer a small token-specific trait or a carefully named
  method on a GitHub installation-token port, and keep the existing validation
  helper unchanged unless the implementation proves a shared trait is simpler
  without obscuring responsibilities.

- Risk: Current `run_token_daemon` surfaces are stubs, and implementing this
  acquisition helper does not complete the token daemon runtime directory,
  atomic writer, refresh loop, `GIT_ASKPASS`, or clone flow. Severity: medium.
  Likelihood: certain. Mitigation: keep this plan focused on 3.2.1 and document
  the remaining 3.3 and 3.4 work in the outcomes section after implementation.

## Progress

- [x] (2026-05-19T23:10:54Z) Create the `leta` workspace for this repository.
- [x] (2026-05-19T23:10:54Z) Load the `leta`,
  `hexagonal-architecture`, `rust-router`, `execplans`, `firecrawl`,
  `commit-message`, and `pr-creation` skill guidance relevant to planning.
- [x] (2026-05-19T23:10:54Z) Review `AGENTS.md`, roadmap item 3.2.1, the token
  management design, current GitHub code, error types, and BDD conventions.
- [x] (2026-05-19T23:10:54Z) Use Firecrawl to confirm GitHub App installation
  token prior art and Octocrab's `installation_token_with_buffer` behaviour.
- [x] (2026-05-19T23:10:54Z) Create `context_pack` pack `pk_dvtfmyk6` for the
  Wyvern planning team.
- [x] (2026-05-19T23:10:54Z) Receive Wyvern planning findings for architecture
  boundaries and validation strategy.
- [x] (2026-05-19T23:10:54Z) Rename the local branch to
  `3-2-1-installation-token-with-buffer`.
- [ ] Draft this ExecPlan and request approval.
- [ ] After approval, establish red tests for token acquisition, expiry
  metadata, semantic failures, and token redaction.
- [ ] Implement the GitHub installation-token acquisition adapter.
- [ ] Add behavioural coverage with `rstest-bdd`.
- [ ] Update design, user, developer, and roadmap documentation.
- [ ] Run gates, CodeRabbit review, commit, push, and open the implementation
  pull request.

## Surprises & discoveries

- Observation: `leta workspace add` succeeded, and `leta files` listed the
  repository, but `leta show GitHubAppClient` and related symbol lookups did
  not resolve Rust symbols in this workspace. Evidence: `leta show` returned
  "Symbol not found" for known Rust symbols in `src/github/mod.rs`. Impact:
  implementation should retry `leta` after any indexing delay, but the planning
  pass used targeted file reads and `rg` for non-symbol documentation searches.

- Observation: Firecrawl confirmed that GitHub's REST endpoint for an
  installation access token is
  `POST /app/installations/{installation_id}/access_tokens`, authenticated by a
  GitHub App JSON Web Token. The request may optionally narrow access with
  `repositories`, `repository_ids`, and `permissions`. Evidence: Firecrawl
  scrape of GitHub Docs for generating installation access tokens and the REST
  endpoint. Impact: this plan treats repository/permission narrowing as a
  future or escalated concern because the roadmap explicitly requires
  Octocrab's `installation_token_with_buffer`, whose current implementation
  uses an empty request body.

- Observation: Firecrawl and local crate source confirmed that Octocrab
  0.49.5's `installation_token_with_buffer` returns `SecretString` and uses a
  `chrono::Duration` buffer. It returns a cached token if the cached expiry is
  far enough in the future, otherwise it requests and caches a new token.
  Evidence: docs.rs for Octocrab and local source under
  `~/.cargo/registry/src/.../octocrab-0.49.5/src/lib.rs`. Impact: exact server
  expiry is not available from this helper, so the plan derives conservative
  metadata only when acquiring from a fresh installation client.

## Decision log

- Decision: Keep this ExecPlan in `Status: DRAFT` and require explicit user
  approval before implementation. Rationale: The user specifically said the
  plan must be approved before it is implemented, and the execplans skill
  requires that gate for initial drafts. Date/Author: 2026-05-19T23:10:54Z /
  Codex.

- Decision: Place implementation behind an internal GitHub adapter boundary
  rather than calling Octocrab from `src/api/mod.rs` or `src/main.rs`.
  Rationale: This preserves the hexagonal dependency rule: orchestration calls
  a small Podbot-owned port or helper, and the adapter owns Octocrab details.
  Date/Author: 2026-05-19T23:10:54Z / Codex, informed by Wyvern architecture
  review.

- Decision: Prefer a fresh installation client per acquisition to avoid
  scheduling against an unknown cached-token expiry. Rationale: Octocrab's
  helper may return cached tokens, but a newly created installation client has
  no cached installation token. With GitHub's one-hour lifetime, Podbot can
  safely derive metadata for refresh scheduling without exposing or depending
  on Octocrab internals. Date/Author: 2026-05-19T23:10:54Z / Codex.

- Decision: Do not plan property, Kani, or Verus coverage for this task.
  Rationale: The task introduces an async service-adapter call and simple
  duration arithmetic, not a broad state machine, unsafe code, or contractual
  lemma. Unit and behavioural tests provide proportionate rigour. Date/Author:
  2026-05-19T23:10:54Z / Codex, aligned with Wyvern validation review.

## Outcomes & retrospective

This section is intentionally empty while the plan is in draft. During
implementation, update it after each major milestone with what changed, what
was learned, and what remains for roadmap steps 3.3 and 3.4.

## Context and orientation

Podbot is a Rust 2024 workspace that provides a command-line interface (CLI)
and library for running coding agents in sandbox containers. The private
repository path is controlled by `workspace.source = "github_clone"`. In that
mode, Podbot must authenticate as a GitHub App on the host, mint an
installation access token, and later expose only that short-lived token to the
container through a read-only file.

Roadmap step 3.1 is complete. It added RSA private-key loading,
`build_app_client(app_id, private_key)`, credential validation against
`GET /app`, and classified GitHub credential errors. The current GitHub module
is `src/github/mod.rs`; it is internal and explicitly unstable. The existing
`GitHubAppClient` trait currently contains only
`validate_credentials(&self) -> BoxFuture<'_, Result<(), GitHubError>>`.

The relevant existing files are:

- `src/github/mod.rs`: internal GitHub App authentication code, Octocrab
  client construction, the validation trait, and test-only factory helpers.
- `src/github/classify.rs`: HTTP status classification for GitHub API errors.
- `src/error.rs`: semantic errors, including
  `GitHubError::TokenAcquisitionFailed`, `GitHubError::TokenExpired`, and
  `GitHubError::TokenRefreshFailed`.
- `src/config/types.rs`: `GitHubConfig` with `app_id`, `installation_id`, and
  `private_key_path`.
- `src/api/mod.rs`: library-facing orchestration functions. The
  `run_token_daemon` function is still a stub; this task should not try to
  complete the daemon loop.
- `src/main.rs`: CLI adapter. The `token-daemon` subcommand currently calls
  the API stub.
- `tests/features/github_credential_validation.feature` and
  `tests/bdd_github_credential_validation_helpers/`: examples of the BDD
  structure used for GitHub App flows.
- `docs/podbot-design.md`: source of truth for token management and the
  GitHub clone execution flow.
- `docs/users-guide.md` and `docs/developers-guide.md`: user-facing and
  maintainer-facing documentation that must reflect any new behaviour.

The key term "installation access token" means a short-lived token minted by a
GitHub App for one installation of that App. GitHub issues these tokens through
`POST /app/installations/{installation_id}/access_tokens`; they are accepted by
Git over Hypertext Transfer Protocol (HTTP) and expire after about one hour.

## Plan of work

Stage A is approval and red-test scaffolding. After approval, add focused unit
tests before implementation. Create `src/github/installation_token.rs` for the
new code instead of expanding `src/github/mod.rs`. Add
`src/github/installation_token_tests.rs` if needed so `src/github/tests.rs`
stays below the 400-line file limit. The first tests should define the shape of
an `InstallationAccessToken` result:

```rust
pub struct InstallationAccessToken {
    token: String,
    acquired_at: std::time::SystemTime,
    expires_at: std::time::SystemTime,
    refresh_after: std::time::SystemTime,
}
```

The exact field visibility may be private with accessors. The secret token must
be obtainable for Git credential delivery, while logs and `Debug` output must
expose only timing metadata.

Stage B is the GitHub adapter implementation. Define a small token acquisition
port in `src/github/installation_token.rs`. A likely shape is:

```rust
pub trait GitHubInstallationTokenClient: Send + Sync {
    fn acquire_installation_token(
        &self,
        installation_id: u64,
        expiry_buffer: std::time::Duration,
    ) -> BoxFuture<'_, Result<InstallationAccessToken, GitHubError>>;
}
```

Implement the trait for `OctocrabAppClient`. The implementation should call
`self.client.installation(InstallationId(installation_id))`, then call
`installation_token_with_buffer(chrono::Duration::from_std(expiry_buffer)?)`.
Map conversion failures and Octocrab errors to
`GitHubError::TokenAcquisitionFailed { message }`, reusing the existing
classification style where status codes are available. Do not include token
values in errors.

Stage C is metadata and logging policy. The implementation should take a clock
sample immediately before or after the Octocrab call and derive
`expires_at = acquired_at + 3600 seconds` and
`refresh_after = expires_at - expiry_buffer`. Add a small non-secret logging
method or event helper that emits installation ID, `expires_at`,
`refresh_after`, and buffer seconds. Do not log the token string. If the code
needs a clock abstraction for deterministic tests, define a tiny internal clock
trait or pass `acquired_at` into a pure helper; avoid a new dependency.

Stage D is behavioural coverage. Add
`tests/features/github_installation_token.feature`,
`tests/bdd_github_installation_token.rs`, and
`tests/bdd_github_installation_token_helpers/` following the existing GitHub
credential-validation pattern. Cover:

- successful acquisition returns a token and expiry metadata;
- refresh metadata subtracts the configured buffer from expiry;
- acquisition failure becomes a semantic token-acquisition error;
- token values are not present in logs or error messages.

Stage E is documentation. Update `docs/podbot-design.md` to record the
installation-token acquisition contract and the conservative expiry metadata
decision. Update `docs/users-guide.md` with user-visible behaviour and failure
messages for GitHub clone token acquisition. Update `docs/developers-guide.md`
with internal adapter boundaries, mocking conventions, and the no-token-logs
rule. Mark roadmap item 3.2.1 done only after all gates pass and the
implementation is complete.

Stage F is validation, review, and commits. Run the relevant tests first, then
the full gates. Run CodeRabbit after the token acquisition implementation and
again after documentation if it raises concerns. Commit small units in this
order where possible: tests and adapter implementation, behavioural tests,
documentation and roadmap.

## Concrete steps

From repository root:

```plaintext
/home/leynos/.lody/repos/github---leynos---podbot/worktrees/93b6f504-413e-4374-8997-b9f529a4796a
```

confirm the branch:

```sh
git branch --show-current
```

Expected output:

```plaintext
3-2-1-installation-token-with-buffer
```

Before editing code, refresh navigation context:

```sh
leta workspace add "$PWD"
leta files src/github
```

If `leta show` can now resolve symbols, prefer it for Rust symbol navigation:

```sh
leta show GitHubAppClient
leta show OctocrabAppClient
leta refs GitHubError
```

If symbol lookup still fails, use targeted `rg` and `sed` for specific files
and record that in `Surprises & Discoveries`.

Run the existing GitHub-focused tests to establish a green baseline:

```sh
make test 2>&1 | tee /tmp/test-podbot-3-2-1-installation-token-with-buffer-baseline.out
```

Add red unit tests for the new token result and adapter seam. Then run:

```sh
make test 2>&1 | tee /tmp/test-podbot-3-2-1-installation-token-with-buffer-red.out
```

Expected result before implementation: the new tests fail because the token
acquisition API does not exist or returns the wrong behaviour.

Implement the adapter and run focused checks as available, then run the full
gates sequentially:

```sh
make check-fmt 2>&1 | tee /tmp/check-fmt-podbot-3-2-1-installation-token-with-buffer.out
make lint 2>&1 | tee /tmp/lint-podbot-3-2-1-installation-token-with-buffer.out
make test 2>&1 | tee /tmp/test-podbot-3-2-1-installation-token-with-buffer.out
```

After documentation updates, run:

```sh
make markdownlint 2>&1 | tee /tmp/markdownlint-podbot-3-2-1-installation-token-with-buffer.out
make fmt 2>&1 | tee /tmp/fmt-podbot-3-2-1-installation-token-with-buffer.out
make nixie 2>&1 | tee /tmp/nixie-podbot-3-2-1-installation-token-with-buffer.out
make check-fmt 2>&1 | tee /tmp/check-fmt-podbot-3-2-1-installation-token-with-buffer-final.out
make lint 2>&1 | tee /tmp/lint-podbot-3-2-1-installation-token-with-buffer-final.out
make test 2>&1 | tee /tmp/test-podbot-3-2-1-installation-token-with-buffer-final.out
```

Run CodeRabbit after each major milestone:

```sh
coderabbit review --agent
```

If CodeRabbit reports actionable issues, fix them, update this ExecPlan, rerun
the relevant gates, and only then continue.

## Validation and acceptance

Acceptance is behavioural, not merely structural:

- A unit test proves a successful mocked acquisition returns a token string,
  `acquired_at`, `expires_at`, and `refresh_after`.
- A unit test proves `refresh_after` is exactly `expires_at - buffer` for a
  representative five-minute buffer.
- A unit test proves acquisition errors are mapped to
  `GitHubError::TokenAcquisitionFailed` and preserve actionable context.
- A unit or behavioural test proves token fixture values are absent from
  formatted errors, debug output, and log output.
- A BDD scenario proves the happy path in user language: configured GitHub App
  credentials and an installation ID produce a token suitable for later Git
  operations.
- A BDD scenario proves an unhappy path: rejected installation token
  acquisition returns a clear semantic failure.
- `docs/podbot-roadmap.md` marks 3.2.1 complete only after all code,
  behavioural tests, documentation updates, and gates are complete.
- `make check-fmt`, `make lint`, and `make test` pass.
- Documentation gates pass after doc changes: `make markdownlint`, `make fmt`,
  and `make nixie`.
- `coderabbit review --agent` has no unresolved actionable concerns.

## Idempotence and recovery

All implementation steps are additive and can be repeated. Tests may be rerun
without cleanup. Temporary command logs are written under `/tmp` and may be
removed after review.

If a gate fails, inspect the corresponding `/tmp/*podbot-3-2-1*` log, make a
minimal fix, update this ExecPlan's `Progress` or `Surprises & Discoveries`
when the failure reveals new information, and rerun only the failed gate before
continuing to the full final sequence.

If the direct `chrono` dependency is added and later proves unnecessary, remove
it in the same atomic change before committing. Do not leave dependency churn
in a separate commit.

If implementation discovers that exact `expires_at` is mandatory, do not patch
around Octocrab by calling undocumented internals. Stop, record the finding in
`Decision Log`, and ask whether to switch to an explicit REST adapter for
`POST /app/installations/{installation_id}/access_tokens`.

## Artifacts and notes

Firecrawl research used these sources:

- GitHub Docs:
  `https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-an-installation-access-token-for-a-github-app`
- GitHub REST Docs:
  `https://docs.github.com/en/rest/apps/apps#create-an-installation-access-token-for-an-app`
- Octocrab docs.rs:
  `https://docs.rs/octocrab/latest/octocrab/struct.Octocrab.html#method.installation_token_with_buffer`

Wyvern planning agents reported:

```plaintext
Architecture: keep Octocrab calls inside the GitHub adapter boundary; avoid
direct Octocrab use in API or CLI orchestration; add a token-specific seam and
metadata type.
```

```plaintext
Validation: add rstest unit coverage for happy path, error mapping, expiry
metadata, and redaction; add rstest-bdd scenarios for token acquisition
behaviour; property, Kani, and Verus coverage are not warranted for this slice.
```

## Interfaces and dependencies

The likely internal token result type in `src/github/installation_token.rs` is:

```rust
pub struct InstallationAccessToken {
    token: String,
    acquired_at: std::time::SystemTime,
    expires_at: std::time::SystemTime,
    refresh_after: std::time::SystemTime,
}
```

Provide accessor methods rather than public fields if that better protects
redaction:

```rust
impl InstallationAccessToken {
    pub fn token(&self) -> &str;
    pub const fn acquired_at(&self) -> std::time::SystemTime;
    pub const fn expires_at(&self) -> std::time::SystemTime;
    pub const fn refresh_after(&self) -> std::time::SystemTime;
}
```

Do not derive `Debug` if it would expose `token`. If a `Debug` implementation
is useful, write it manually and omit or redact the token field.

The likely internal acquisition trait is:

```rust
pub trait GitHubInstallationTokenClient: Send + Sync {
    fn acquire_installation_token(
        &self,
        installation_id: u64,
        expiry_buffer: std::time::Duration,
    ) -> BoxFuture<'_, Result<InstallationAccessToken, GitHubError>>;
}
```

Production implementation details:

- Convert `installation_id: u64` to `octocrab::models::InstallationId`
  internally.
- Convert `std::time::Duration` to `chrono::Duration` only at the Octocrab
  call boundary.
- Call `installation_token_with_buffer`.
- Convert the returned secret string into the Podbot token result without
  logging it.
- Map failures into `GitHubError::TokenAcquisitionFailed { message }`.

The only anticipated dependency change is:

```toml
chrono = "0.4.43"
```

Add it only if the implementation must name `chrono::Duration` directly to call
Octocrab's public method.

## Revision note

- 2026-05-19: Initial draft created for user approval. The plan records
  Firecrawl findings, Wyvern review input, expected implementation boundaries,
  validation strategy, and the Octocrab expiry-metadata trade-off. No feature
  implementation has begun.
