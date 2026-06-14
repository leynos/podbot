# Address code-base audit findings

This ExecPlan (execution plan) is a living document. The sections `Constraints`,
`Tolerances`, `Risks`, `Progress`, `Surprises & discoveries`, `Decision log`,
and `Outcomes & retrospective` must be kept up to date as work proceeds.

Status: DRAFT

## Purpose / big picture

This plan addresses the maintainability and correctness concerns found during
the 2026-06-05 code-base audit. After the work lands, public APIs must not
claim success for work that has not happened, engine modules must not depend
upward on application programming interface (API) or configuration objects,
credential upload failures must be classified using structured data rather than
display strings, configuration loading must produce inspectable errors, and
test helpers must fail clearly when scenarios are under-specified.

Success is observable in three ways. First, embedders receive errors for
unsupported commands instead of false `CommandOutcome::Success` results,
without adding a new variant to the currently exhaustive public `PodbotError`
enum. Second, tests cover both happy and unhappy paths for the changed public
and internal contracts. Third, the canonical gates pass after each major
milestone:

```sh
set -o pipefail
make check-fmt 2>&1 | tee "/tmp/check-fmt-podbot-code-base-audit-2026-06-05.out"
make lint 2>&1 | tee "/tmp/lint-podbot-code-base-audit-2026-06-05.out"
make test 2>&1 | tee "/tmp/test-podbot-code-base-audit-2026-06-05.out"
```

After those deterministic gates pass for each milestone, run:

```sh
coderabbit review --agent
```

If the CodeRabbit rate limit is exceeded, sleep before retrying:

```sh
vsleep "$(shuf -i 15-30 -n 1)m"
```

The audit findings map to implementation milestones as follows:

| Finding | Milestone | Outcome |
| --- | --- | --- |
| Placeholder APIs report success | Milestone 1 | Compatible error result |
| Config file errors are misclassified | Milestone 1 | Targeted config errors |
| Engine imports API/config types | Milestone 2 | Engine-native requests |
| Credential errors use string matching | Milestone 3 | Structured source data |
| `ExecRequest` TTY builder docs are unclear | Milestone 4 | Rustdoc example |
| Config discovery docs are stale | Milestone 4 | Both candidates documented |
| Async tests can hang | Milestone 5 | Timeout-guarded tests |
| BDD helpers default missing state | Milestone 5 | Precondition errors |
| Repeated test helper stacks | Milestone 5 | Owned shared helpers |
| Oversized protocol module | Milestone 6 | Responsibility split |

## Constraints

The branch is `code-base-audit-2026-06-05`, tracking
`origin/code-base-audit-2026-06-05`. The plan file is
`docs/execplans/code-base-audit-2026-06-05.md`.

All repository instructions in `AGENTS.md` remain binding. In particular, use
Makefile targets rather than raw Cargo commands for standard gates, run test,
lint, and formatting gates sequentially, and route long command output through
`tee` into `/tmp`.

The stable library boundary described in
`docs/adr-001-define-the-stable-public-library-boundary.md` and
`docs/developers-guide.md` must remain intact. Public modules may expose typed
domain concepts, but must not expose engine, GitHub adapter, or command-line
interface (CLI) internals.

Every Rust module must keep or gain a module-level `//!` comment. Every public
API added or changed must have Rustdoc, including examples where useful.

Tests added for changed Rust behaviour should use `rstest` for table-driven
unit coverage and `rstest-bdd` for behaviour-driven scenarios where the change
is externally observable or already covered by Gherkin scenarios. Use
`googletest` assertions and `pretty_assertions` where their assertion style
makes failures clearer. Use `insta` snapshots only where output shape has
multiple variants and snapshot stability is relevant.

Do not add property tests, Kani harnesses, or Verus proofs unless a milestone
introduces a new invariant over broad input ranges, states, orderings, or
contractual business logic. This plan mainly makes existing contracts typed and
explicit; example-based and behaviour-driven tests are expected to be the
smallest sufficient rigour.

When introducing a reusable helper, abstraction, or port, first sweep the
repository for an existing equivalent. Document the new abstraction's intended
scope and reuse policy in `docs/developers-guide.md` if it is not already clear
there.

Unsupported public placeholder commands must not add a new top-level
`PodbotError` variant in this branch. `podbot::error` is documented as
semver-stable and `PodbotError` is currently exhaustive, so adding a variant
would break embedders with exhaustive matches. Route the unsupported diagnostic
through an existing compatible error bucket, such as
`ContainerError::ExecFailed` with an operation identifier and clear
unsupported-work message. A new
top-level unsupported error requires a separate versioned migration that first
makes the public enum non-exhaustive or otherwise documents the breaking change.

## Tolerances

Stop and ask for direction if any single implementation milestone requires
touching more than twelve production files or if the remediation delta from
plan commit `c95dab9` grows beyond roughly 1,500 changed lines, excluding
generated snapshots and this plan.

Stop and ask for direction before changing a stable public type in a way that
would break existing callers. Do not add variants to currently exhaustive public
error enums, including `PodbotError`, unless the milestone explicitly includes a
versioned or non-exhaustive migration.

Stop and ask for direction if CodeRabbit reports a security, soundness, or
public API compatibility concern that cannot be resolved with a local patch in
the current milestone.

Stop and ask for direction if `make lint` or `make test` fails for reasons that
appear unrelated to this branch and cannot be reproduced in a focused command.

## Risks

Changing placeholder API results can break tests or callers that treated
successful no-op behaviour as the current contract. The affected functions are
behind the `experimental` feature, so the compatibility risk is limited to
experimental embedders and command-line tests that enable that feature. Mitigate
this by returning an existing compatible `PodbotError` bucket with a clear
unsupported diagnostic, rather than adding a new top-level variant.

Moving config-to-engine construction out of engine modules can create churn in
internal tests. Mitigate this by adding small composition helpers at the API or
orchestration layer rather than expanding call sites with repeated primitive
assembly.

Splitting `src/engine/connection/exec/protocol.rs` risks accidental visibility
or lifecycle changes. Mitigate this with a purely mechanical split after the
behavioural changes are already green, preserving function names and existing
tests where possible.

Consolidating test helpers risks over-generalizing BDD support. Mitigate this
by extracting only repeated state access, temporary file/directory, runtime,
and assertion helpers with narrow names and local ownership documented in the
developer's guide.

## Implementation plan

Start with the plan milestone. Commit this ExecPlan alone after Markdown
formatting and validation. Push the renamed branch and open a draft pull
request for plan review.

Milestone 1 changes false-success public command APIs and configuration error
ergonomics. Do not add a `PodbotError::Unsupported` variant. Instead, update
`run_agent`, `list_containers`, `stop_container`, and `run_token_daemon` so
unimplemented work returns an existing compatible error bucket, preferably
`PodbotError::Container(ContainerError::ExecFailed { .. })`, with the operation
name in `container_id` and a message that states the operation is unsupported
until implemented. Update CLI-facing tests that currently expect success.
`run_agent` must continue to return specific request, `ConfigError`, and
`GitHubError` validation failures before returning the unsupported diagnostic
for the still-unimplemented agent lifecycle. Reword its Rustdoc so validation
failures remain documented as real behaviour while the post-validation
lifecycle is explicitly unsupported. Remove the three
`#[expect(clippy::missing_const_for_fn)]` attributes from the pure stubs when
their bodies start constructing errors, because the lint will no longer fire and
unfulfilled expectations fail `make lint`. Update `src/config/loader.rs` so
malformed config paths and missing files produce targeted configuration errors.
Add `rstest` unit coverage for supported and unsupported command paths and for
config file read, parse, missing-file, and malformed-path cases. Add or update
BDD coverage only where the behaviour is observable through existing CLI/config
scenarios.

Milestone 2 removes upward dependencies from engine modules. Move
`CreateContainerRequest::from_app_config` out of
`src/engine/connection/create_container/mod.rs` into the orchestration or API
composition layer. Replace repository-clone engine request fields that depend on
`crate::api` value objects with engine-native primitives, such as a
prevalidated remote URL, branch string, workspace path string, and askpass path
string. Move `CredentialUploadRequest::from_app_config` out of
`src/engine/connection/upload_credentials/mod.rs` as part of the same boundary
cleanup, because that engine module also imports `AppConfig`. Keep API value
object parsing and config-to-engine composition at the API or orchestration
boundary. When touching `upload_credentials`, preserve an engine-native request
shape that Milestone 3 can extend with explicit credential source identifiers
without reintroducing `AppConfig`. Add tests that prove API/domain values
compose into engine requests while engine code no longer imports `crate::api` or
`AppConfig`.

Milestone 3 makes credential upload failure classification structured. Replace
string matching in `src/engine/connection/upload_credentials/error_mapping.rs`
with explicit source identifiers carried through the upload plan. Add unit
tests for Claude, Codex, unknown, and filesystem-error paths. Add BDD coverage
if existing credential injection scenarios expose the diagnostic difference to
callers.

Milestone 4 documents and hardens reusable API and test patterns. Update
`ExecRequest` Rustdoc to state that TTY allocation is normalized and only
preserved for `ExecMode::Attached`; add an example showing canonical builder
order. Update configuration discovery docs in `src/config/mod.rs`,
`src/main.rs`, `docs/users-guide.md`, and `docs/developers-guide.md` where
needed so both `~/.config/podbot/config.toml` and `.podbot.toml` are documented
with precedence. Add developer-guide guidance for public placeholder APIs,
layer direction, structured failure classification, async test timeout guards,
and shared test helper ownership if the guide does not already state those
rules clearly.

Milestone 5 hardens and consolidates tests. Add timeout guards around hang-prone
`PendingReader` protocol tests. Replace BDD `unwrap_or_default` state access
with explicit `StepResult` failures and add negative tests for missing required
Given state. Extract repeated temporary config-file, environment mock,
invalid-value assertion, cap-std temporary directory, and Tokio runtime helper
code only where the helper has at least two current callers and a clear owner.
Use `pretty_assertions` or `googletest` assertions where they improve failure
messages.

Milestone 6 splits `src/engine/connection/exec/protocol.rs` by responsibility.
Perform this after behaviour is stable. Move code into small sibling modules
for session orchestration, stdin forwarding, output routing, and runtime policy
adapter integration. Keep public and `pub(super)` surfaces as narrow as the
existing tests permit. Re-run the full gate suite and CodeRabbit before
considering the branch complete.

## Validation plan

For each milestone, run the following commands sequentially and keep the log
files in `/tmp`:

```sh
set -o pipefail
make check-fmt 2>&1 | tee "/tmp/check-fmt-podbot-code-base-audit-2026-06-05.out"
make lint 2>&1 | tee "/tmp/lint-podbot-code-base-audit-2026-06-05.out"
make test 2>&1 | tee "/tmp/test-podbot-code-base-audit-2026-06-05.out"
```

For documentation-only changes, also run:

```sh
set -o pipefail
make markdownlint 2>&1 \
  | tee "/tmp/markdownlint-podbot-code-base-audit-2026-06-05.out"
make nixie 2>&1 | tee "/tmp/nixie-podbot-code-base-audit-2026-06-05.out"
```

After deterministic gates pass, run:

```sh
coderabbit review --agent
```

Record the exact commands, pass/fail status, and any CodeRabbit follow-up in the
`Progress`, `Surprises & discoveries`, and `Outcomes & retrospective`
sections.

## Progress

- [x] 2026-06-05: Loaded `leta`, `rust-router`, `execplans`,
  `arch-crate-design`, `rust-errors`, `commit-message`, and `pr-creation`
  guidance relevant to this work.
- [x] 2026-06-05: Created leta workspace for this repository.
- [x] 2026-06-05: Confirmed the starting worktree was clean.
- [x] 2026-06-05: Renamed local branch from `feat/rustrouterletaaudit` to
  `code-base-audit-2026-06-05`.
- [x] 2026-06-05: Drafted this ExecPlan.
- [x] 2026-06-05: Ran `make fmt`; it completed successfully but produced
  unrelated Markdown formatter churn in pre-existing documents, which was
  reverted to keep the plan commit focused.
- [x] 2026-06-05: Ran `make check-fmt`; it passed.
- [x] 2026-06-05: Ran `make lint`; it passed.
- [x] 2026-06-05: Ran `make test`; it passed.
- [x] 2026-06-05: Ran `make markdownlint`; it passed.
- [x] 2026-06-05: Ran `make nixie`; it passed.
- [x] 2026-06-05: Ran `coderabbit review --agent`; it reported three
  sentence-case heading findings in this plan, which were fixed.
- [x] 2026-06-05: Ran follow-up `coderabbit review --agent`; it reported a
  markdown-wrapping concern, which was fixed by wrapping the long validation
  command.
- [x] 2026-06-05: Re-ran deterministic documentation gates after the
  CodeRabbit wrapping fix; `make check-fmt`, `make markdownlint`, and
  `make nixie` passed.
- [x] 2026-06-05: Attempted a final `coderabbit review --agent` twice after
  fixing the wrapping concern. Both attempts stalled after sandbox
  preparation; the stuck processes from this session were terminated.
- [x] Push branch `code-base-audit-2026-06-05` and set upstream to
  `origin/code-base-audit-2026-06-05`.
- [x] Run plan-milestone documentation gates.
- [x] Request CodeRabbit review for the plan milestone.
- [x] Open a draft pull request for ExecPlan review.
- [x] 2026-06-12: Addressed plan-review feedback by extending Milestone 2 to
  cover `CredentialUploadRequest::from_app_config`, initially specifying an
  unsupported-error target, calling out Clippy expectation removal, clarifying
  experimental compatibility risk, defining the changed-line baseline, and
  adding finding-to-milestone traceability.
- [x] 2026-06-12: Ran `make check-fmt`; it passed.
- [x] 2026-06-12: Ran `make lint`; it passed.
- [x] 2026-06-12: Ran `make test`; the first run failed because the user
  Podman socket was in `trigger-limit-hit`. After resetting and starting the
  socket, the focused e2e test and exact `make test` target passed.
- [x] 2026-06-12: Ran `make markdownlint`; it passed after fixing the
  traceability table separator style.
- [x] 2026-06-12: Ran `make nixie`; it passed.
- [x] 2026-06-12: Ran `coderabbit review --agent` for the updated plan; it
  stalled after sandbox preparation without producing findings, matching the
  earlier service stall. The podbot review process from this session was
  terminated without touching other agents' CodeRabbit processes.
- [x] 2026-06-14: Revised the plan to avoid adding a new exhaustive
  `PodbotError` variant. Milestone 1 now routes unsupported placeholder
  diagnostics through an existing compatible error bucket and defers any new
  top-level unsupported variant to a separate versioned or non-exhaustive
  migration.
- [x] 2026-06-14: Ran `make check-fmt`, `make lint`, `make test`,
  `make markdownlint`, and `make nixie`; all passed.
- [x] 2026-06-14: Ran `coderabbit review --agent`; it stalled after sandbox
  preparation without producing findings. The timeout-wrapped podbot review
  process from this session was terminated without touching other agents'
  CodeRabbit processes.
- [ ] Obtain explicit approval to implement the plan.
- [ ] Milestone 1: public unsupported APIs and config error ergonomics.
- [ ] Milestone 2: engine boundary direction cleanup.
- [ ] Milestone 3: structured credential upload failure classification.
- [ ] Milestone 4: API/docs/developer-guide hardening.
- [ ] Milestone 5: async and BDD test-helper hardening.
- [ ] Milestone 6: protocol module split.

## Surprises & discoveries

The developer's guide already documents several audited concerns, including the
stable library boundary, error handling boundary, repository cloning flow, and
some BDD conventions. It does not yet clearly state a general rule that public
placeholders must return typed unsupported errors rather than success, nor does
it clearly document shared test-helper ownership and structured classification
as reusable patterns.

Running `make fmt` on 2026-06-05 touched several pre-existing Markdown files
outside this plan. Those formatter changes were unrelated to the plan milestone
and were reverted before commit.

CodeRabbit review on 2026-06-05 enforced the documentation style guide's
sentence-case heading rule for this plan's mandatory living sections. The
headings were changed while preserving the required execplan section meanings.

The follow-up CodeRabbit review flagged markdown wrapping. The only overlong
line was a shell command in a validation code block, and it was wrapped to keep
the command readable while satisfying the style guide.

Two final CodeRabbit reruns after the wrapping fix stalled after sandbox
preparation and did not produce review findings. The deterministic gates were
clean after the fix, and the earlier CodeRabbit findings were resolved.

Plan review on 2026-06-12 found that Milestone 2 did not cover
`CredentialUploadRequest::from_app_config`, even though it imports `AppConfig`
from an engine module. The same review identified that changing the three pure
placeholder stubs will make their `clippy::missing_const_for_fn` expectations
unfulfilled unless those attributes are removed.

The first 2026-06-12 `make test` run failed in
`tests/bdd_repository_cloning_e2e.rs` because testcontainers could not connect
to the container engine. `systemctl --user status podman.socket` showed
`trigger-limit-hit`. After `systemctl --user reset-failed podman.socket` and
`systemctl --user start podman.socket`, `curl --unix-socket
/run/user/1000/podman/podman.sock http://localhost/_ping` returned `OK`, the
focused e2e target passed, and the exact `make test` target passed.

Plan review on 2026-06-14 found that adding
`PodbotError::Unsupported { .. }` would break embedders that exhaustively match
the currently public `PodbotError` enum. The plan now keeps Milestone 1
compatible by using an existing error bucket for unsupported placeholder
diagnostics.

## Decision log

2026-06-05: Use a typed unsupported/not-implemented error rather than removing
public placeholder functions. The functions are experimental, so the
compatibility risk is limited, but returning an inspectable domain error fixes
false-success semantics while preserving symbols for experimental callers.

2026-06-14: Do not add `PodbotError::Unsupported` in this remediation branch.
Although an explicit unsupported variant would be the clearest long-term shape,
`PodbotError` is currently public and exhaustive. Adding a variant would be a
breaking change for embedders with exhaustive matches. Use an existing
compatible bucket for this branch, and handle any new top-level unsupported
variant through a separate versioned or non-exhaustive migration.

2026-06-12: Include `CredentialUploadRequest::from_app_config` in Milestone 2.
The milestone's success criterion is that engine code no longer imports
`crate::api` or `AppConfig`, so all current engine importers must be covered.

2026-06-05: Defer the `protocol.rs` split until after behaviour-changing fixes.
The file is oversized, but a mechanical split is safest once tests already pin
the new public, config, credential, and BDD semantics.

2026-06-05: Treat property tests, Kani, and Verus as unnecessary unless a later
milestone introduces a new broad invariant. The known changes tighten existing
contracts and error classifications rather than adding new state machines or
proofs.

## Outcomes & retrospective

No implementation outcomes yet. This plan is ready for review before code
changes begin.

Plan-milestone validation on 2026-06-05:

- `make check-fmt`: passed.
- `make lint`: passed.
- `make test`: passed.
- `make markdownlint`: passed.
- `make nixie`: passed.
- `coderabbit review --agent`: stalled after sandbox preparation without
  producing findings.
- `coderabbit review --agent`: sentence-case and wrapping findings fixed; final
  reruns stalled after setup without producing findings.

Plan-review-update validation on 2026-06-12:

- `make check-fmt`: passed.
- `make lint`: passed.
- `cargo test --all-features --test bdd_repository_cloning_e2e` with
  `DOCKER_HOST=unix:///run/user/1000/podman/podman.sock`: passed after resetting
  the failed user Podman socket.
- `make test`: passed after resetting the failed user Podman socket.
- `make markdownlint`: passed after fixing the traceability table separator
  style.
- `make nixie`: passed.
- `coderabbit review --agent`: stalled after sandbox preparation without
  producing findings.

Plan-review-update validation on 2026-06-14:

- `make check-fmt`: passed.
- `make lint`: passed.
- `make test`: passed.
- `make markdownlint`: passed.
- `make nixie`: passed.
