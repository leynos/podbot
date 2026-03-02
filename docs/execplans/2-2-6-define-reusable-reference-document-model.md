# Step 2.2.6: define reusable reference-document model

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & discoveries`,
`Decision log`, and `Outcomes & retrospective` must be kept up to date as work
proceeds.

Status: DRAFT

## Purpose and big picture

Deliver canonical content primitives that are independent from ingestion-job
lifecycle so host and guest profile documents can be modelled, versioned, and
bound consistently across series.

After implementation, teams can create and revise a `ReferenceDocument`, link
it to a series profile through `ReferenceBinding`, and query contracts through
repository ports and API endpoints without coupling to ingestion internals.

Success is visible through three artefacts and one behavioural proof set:

- an approved entity-relationship (ER) diagram for `ReferenceDocument`,
  `ReferenceDocumentRevision`, and `ReferenceBinding`;
- glossary entries that define all three entities and their invariants;
- documented repository and API contract acceptance criteria;
- passing unit tests (`pytest`) and behavioural tests (`pytest-bdd`) proving
  model invariants and end-user flows.

## Constraints

- Preserve hexagonal dependency direction: domain and application layers must
  not import Falcon, SQLAlchemy, or transport-specific types.
- Define ports (repository contracts) in the domain or application boundary;
  adapters implement ports.
- Keep the model explicitly independent from ingestion-job scope. No foreign
  keys, service calls, or naming that require ingestion-job entities.
- Include series-aligned host and guest profile document support in both schema
  and contracts.
- Keep existing externally published API behaviour backward compatible unless a
  documented, approved contract change is unavoidable.
- Ensure design decisions are recorded in the design document.
- Ensure end-user behavioural changes are documented in `docs/users-guide.md`.
- Ensure internal interface and operational guidance are documented in
  `docs/developers-guide.md` (create file if missing).
- Mark roadmap item `2.2.6` as done only after all acceptance checks pass.

## Tolerances (exception triggers)

- Scope: stop and escalate if implementation exceeds 16 files or 700 net lines
  before tests and docs updates.
- Interface: stop and escalate if a breaking public API contract change is
  required without an explicit migration path.
- Dependencies: stop and escalate before adding new runtime dependencies.
- Ambiguity: stop and escalate if `docs/roadmap.md` and repository roadmap
  location disagree and this affects completion tracking.
- Validation: stop and escalate if `make check-fmt`, `make typecheck`,
  `make lint`, or `make test` fails after two focused fix passes.

## Risks

- Risk: Referenced context docs may be missing or outdated.
  Severity: high. Likelihood: medium. Mitigation: verify each referenced file
  at Stage A; if absent, create a documented substitute and record the
  superseding decision in the design doc.

- Risk: Existing persistence schema may embed ingestion assumptions indirectly.
  Severity: high. Likelihood: medium. Mitigation: model review against current
  schema and service calls, with a migration note for any decoupling edits.

- Risk: Contract drift between repository ports and API payloads.
  Severity: medium. Likelihood: medium. Mitigation: define acceptance criteria
  as executable tests and keep a single glossary-backed vocabulary for field
  names.

- Risk: Requested Python validation commands may not map directly to the
  current repository toolchain.
  Severity: medium. Likelihood: high. Mitigation: in Stage A, document the
  authoritative command mapping (for example, `pytest`/`pytest-bdd` versus
  existing workspace gates) before implementation begins.

## Context and orientation

This task implements roadmap item `2.2.6` under canonical content foundation.
The implementation centres on three core entities:

- `ReferenceDocument`: canonical document identity and stable metadata.
- `ReferenceDocumentRevision`: immutable revision snapshots of document
  content.
- `ReferenceBinding`: relationship between a series profile context
  (host/guest) and a selected document revision.

Series alignment requirement:

- host profile and guest profile documents are first-class bindings in the same
  model, not separate ad-hoc tables.

Referenced documents from the task request:

- `docs/roadmap.md`
- `docs/episodic-podcast-generation-system-design.md`
- `docs/async-sqlalchemy-with-pg-and-falcon.md`
- `docs/testing-async-falcon-endpoints.md`
- `docs/testing-sqlalchemy-with-pytest-and-py-pglite.md`
- `docs/agentic-systems-with-langgraph-and-celery.md`

If any referenced document is missing, this plan treats that as an explicit
Stage A deliverable: create or supersede with documented rationale before
implementation proceeds.

## Interfaces and dependencies

Repository port contracts are defined at the boundary using Protocol interfaces
(or equivalent abstract contracts) and consumed by application services.
Adapters provide SQLAlchemy-backed implementations.

```python
from typing import Protocol, Sequence

class ReferenceDocumentRepository(Protocol):
    async def create_document(self, command: CreateReferenceDocument) -> ReferenceDocument: ...
    async def get_document(self, document_id: ReferenceDocumentId) -> ReferenceDocument | None: ...
    async def add_revision(self, command: AddReferenceDocumentRevision) -> ReferenceDocumentRevision: ...
    async def list_revisions(
        self,
        document_id: ReferenceDocumentId,
    ) -> Sequence[ReferenceDocumentRevision]: ...

class ReferenceBindingRepository(Protocol):
    async def bind_revision(self, command: BindReferenceRevision) -> ReferenceBinding: ...
    async def get_binding(
        self,
        series_id: SeriesId,
        profile_kind: ProfileKind,
    ) -> ReferenceBinding | None: ...
```

API contract surface (Falcon) remains transport-only:

- `POST /reference-documents`
- `POST /reference-documents/{document_id}/revisions`
- `PUT /series/{series_id}/profiles/{profile_kind}/reference-binding`
- `GET /series/{series_id}/profiles/{profile_kind}/reference-binding`

`profile_kind` accepts `host` and `guest`.

## Plan of work

### Stage A: baseline, source verification, and architecture boundary lock

Verify referenced docs, roadmap path, and existing architecture seams. Document
any missing docs or path mismatches before implementation.

Deliverables:

- validated source list (or superseding docs list);
- explicit architecture boundary note for ports and adapters;
- approved file-level implementation map.

Go/no-go: proceed only when the source of truth for roadmap and design docs is
unambiguous.

### Stage B: domain model and ER design

Define domain entities and invariants independent of ingestion-job scope.
Produce the ER diagram and glossary entries.

Deliverables:

- entity definitions for `ReferenceDocument`,
  `ReferenceDocumentRevision`, and `ReferenceBinding`;
- Mermaid ER diagram in the design doc;
- glossary entries for all three entities and key fields;
- documented host/guest profile binding rules.

Go/no-go: proceed only when ER and glossary are approved by maintainers.

### Stage C: repository ports and persistence adapter

Define repository contracts in the boundary layer and implement SQLAlchemy
adapters. Ensure adapters conform to ports and remain infrastructure-isolated.

Deliverables:

- repository Protocol interfaces and DTOs/commands;
- SQLAlchemy models and mapping logic for three entities;
- migration scripts for new tables/indexes/constraints;
- acceptance criteria for repository contract behaviour.

Go/no-go: proceed only when port-level unit tests pass.

### Stage D: API contract and behavioural flows

Expose the model via Falcon endpoints for create, revise, bind, and read flows
with series host/guest profile support.

Deliverables:

- request and response schemas for the endpoints;
- API acceptance criteria documented in developers guide;
- behavioural scenarios for success, conflict, and not-found paths.

Go/no-go: proceed only when endpoint behavioural tests pass.

### Stage E: documentation, roadmap closure, and quality gates

Record decisions and operational guidance, then run required checks.

Deliverables:

- design decision updates in the design document;
- end-user behaviour updates in `docs/users-guide.md`;
- internal contract guidance in `docs/developers-guide.md`;
- roadmap item `2.2.6` marked done;
- passing `make check-fmt`, `make typecheck`, `make lint`, and `make test`.

## Concrete steps

Run from repository root with `set -o pipefail` and `tee` log capture.

```bash
set -o pipefail
LOG_BASE="/tmp/2-2-6-reference-document-model"

make check-fmt 2>&1 | tee "${LOG_BASE}-check-fmt.log"
make typecheck 2>&1 | tee "${LOG_BASE}-typecheck.log"
make lint 2>&1 | tee "${LOG_BASE}-lint.log"
make test 2>&1 | tee "${LOG_BASE}-test.log"

pytest tests/unit -k "reference_document or reference_binding" 2>&1 | tee "${LOG_BASE}-pytest-unit.log"
pytest tests/bdd -k "reference_document or reference_binding" 2>&1 | tee "${LOG_BASE}-pytest-bdd.log"
```

Expected success indicators:

```plaintext
make check-fmt   # exits 0
make typecheck   # exits 0
make lint        # exits 0
make test        # exits 0
pytest ...       # reports all selected tests passed
```

If a command fails, fix, rerun the failing command, then rerun full gates.

## Validation and acceptance

Finish line is reached only when all criteria below are true:

- ER diagram for the three entities is merged and explicitly approved.
- Glossary includes complete definitions for `ReferenceDocument`,
  `ReferenceDocumentRevision`, and `ReferenceBinding`.
- Repository contracts are documented and enforced by unit tests.
- API contracts are documented and enforced by behavioural tests.
- Host and guest series profile bindings are demonstrably supported.
- `make check-fmt`, `make typecheck`, `make lint`, and `make test` pass.
- Unit tests pass under `pytest`.
- Behavioural tests pass under `pytest-bdd`.
- Design decisions are recorded in the design document.
- End-user behaviour is documented in `docs/users-guide.md`.
- Internal guidance is documented in `docs/developers-guide.md`.
- Roadmap item `2.2.6` is marked done.

## Idempotence and recovery

All plan stages are additive and can be rerun safely.

If a stage partially applies:

- revert only incomplete hunks for that stage;
- rerun stage-local tests first;
- rerun full quality gates before progressing.

Keep command logs in `/tmp` with stage-specific names to preserve evidence
across retries.

## Artifacts and notes

Implementation evidence should include:

- ER diagram snippet and review approval reference;
- final glossary section excerpt;
- repository contract doc excerpt;
- API acceptance criteria excerpt;
- gate command summaries with exit codes.

## Progress

- [x] (2026-02-28 UTC) Collected skill guidance (`execplans`,
      `hexagonal-architecture`) and repository constraints.
- [x] (2026-02-28 UTC) Drafted ExecPlan for roadmap task `2.2.6` at
      `docs/execplans/2-2-6-define-reusable-reference-document-model.md`.
- [x] (2026-02-28 UTC) Added `make typecheck` gate support in `Makefile` and
      validated `make check-fmt`, `make typecheck`, `make lint`, and
      `make test` all pass.
- [ ] Verify roadmap source file location and ensure `2.2.6` exists.
- [ ] Implement domain model, ports, adapters, and API contracts.
- [ ] Add and pass unit (`pytest`) and behavioural (`pytest-bdd`) tests.
- [ ] Update design, user, and developer documentation.
- [ ] Run all quality gates and mark roadmap item done.

## Surprises & discoveries

- Observation: `docs/roadmap.md` is absent; the repository currently contains
  `docs/podbot-roadmap.md`.
  Evidence: `rg --files | rg 'roadmap\\.md$'` returned
  `docs/podbot-roadmap.md` only. Impact: Stage A must confirm whether to add
  `docs/roadmap.md` or map task `2.2.6` into the existing roadmap document.

- Observation: Referenced design and testing docs may be absent.
  Evidence: path validation is required before implementation. Impact:
  superseding docs may need to be created before contract design.

## Decision log

- Decision: Use hexagonal architecture as a hard boundary for this feature.
  Rationale: preserves domain independence and adapter testability while
  matching requested architectural scope. Date/Author: 2026-02-28 / Codex

- Decision: Treat missing referenced docs as explicit Stage A deliverables
  rather than silent substitutions. Rationale: prevents hidden assumptions and
  keeps approval criteria auditable. Date/Author: 2026-02-28 / Codex

- Decision: Define acceptance criteria as executable tests plus documentation
  artefacts. Rationale: the task finish line requires both behavioural proof
  and explicit contract documentation. Date/Author: 2026-02-28 / Codex

- Decision: Add a `typecheck` Makefile target to satisfy required gate
  execution in this repository.
  Rationale: the requested quality gates explicitly require `make typecheck`,
  and the target did not exist before planning work.
  Date/Author: 2026-02-28 / Codex

## Outcomes & retrospective

Pending implementation. This section will be completed when the feature reaches
Status: COMPLETE.

## Revision note

Initial draft created on 2026-02-28 to scope roadmap item `2.2.6` with
hexagonal boundaries, test strategy, and documentation obligations.
