"""What the single CodeScene publisher's upload step must look like.

Separated from ``codescene_coverage_test``, whose subject is which
workflow may upload at all. The subject here is the one that may: what
its upload step is guarded on, how it receives the secret, and what
stops two of its runs racing. The readings are in ``publisher_rules``.

Run via ``make test-workflow-contracts``.
"""

from __future__ import annotations

import pathlib
import typing as typ

import pytest
from codescene_coverage import CODESCENE_ACTION, invokes, publishers
from publisher_rules import (
    CREDENTIAL_PRESENT_CONJUNCT,
    MAIN_REF_CONJUNCT,
    binds_the_credential,
    cancelling_scopes,
    guard_conjuncts,
    stray_credential_sites,
)
from workflow_reading import read_workflows, workflow_jobs

if typ.TYPE_CHECKING:  # pragma: no cover - typing only
    from workflow_reading import WorkflowDocument

REPOSITORY_ROOT: typ.Final[pathlib.Path] = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS: typ.Final[pathlib.Path] = REPOSITORY_ROOT / ".github" / "workflows"


class Upload(typ.NamedTuple):
    """The publisher's upload step, with where it sits."""

    workflow: str
    document: WorkflowDocument
    path: str
    step: dict[str, object]


def _raw_steps(job: dict[str, object]) -> list[object]:
    """Return a job's steps list as written, so indices match the document's paths."""
    steps = job.get("steps")
    return steps if isinstance(steps, list) else []


@pytest.fixture(scope="module")
def upload() -> Upload:
    """Return the publisher's single CodeScene upload step."""
    ((name, document),) = publishers(read_workflows(WORKFLOWS)).items()
    found = [
        Upload(name, document, f"jobs.{job_name}.steps[{index}]", step)
        for job_name, job in workflow_jobs(document).items()
        for index, step in enumerate(_raw_steps(job))
        if isinstance(step, dict) and invokes(step, CODESCENE_ACTION)
    ]
    assert len(found) == 1, (
        f"{name} must invoke {CODESCENE_ACTION} once; it does {len(found)} times"
    )
    return found[0]


def test_the_publisher_uploads_rather_than_checks(upload: Upload) -> None:
    """The mode is named, not left to the action's default.

    ``mode`` decides whether the step uploads a report or gates against
    one, and naming it is how a reader knows which without reading the
    action.
    """
    inputs = upload.step.get("with")
    mode = inputs.get("mode") if isinstance(inputs, dict) else None
    assert mode == "upload", (
        f"the upload step must name `mode: upload`; it names {mode!r}"
    )


def test_the_publisher_uploads_only_from_main(upload: Upload) -> None:
    """The ref is one conjunct of the guard, and no ``||`` can bypass it.

    The publisher declares ``workflow_dispatch``, which can name any
    branch, so without this a feature branch's coverage could publish as
    the trunk's and every pull request would ratchet against it.
    """
    condition = str(upload.step.get("if", ""))
    conjuncts = guard_conjuncts(condition)
    assert conjuncts is not None, f"the upload guard contains `||`: {condition!r}"
    assert MAIN_REF_CONJUNCT in conjuncts, (
        f"the upload step must be guarded on {MAIN_REF_CONJUNCT} as one `&&` "
        f"term; it is guarded on {condition!r}"
    )
    assert CREDENTIAL_PRESENT_CONJUNCT in conjuncts, (
        f"the upload step must skip, not fail, when the secret is absent: {condition!r}"
    )


def test_the_publisher_binds_the_credential_it_tests(upload: Upload) -> None:
    """The guard's variable is bound on the step and handed to the action.

    ``env.CS_ACCESS_TOKEN != ''`` stays well formed with the binding
    deleted, because GitHub reads a missing property as ``''``; the
    upload then skips on every run and nothing fails.
    """
    assert binds_the_credential(upload.step), (
        "the upload step must bind CS_ACCESS_TOKEN to "
        "${{ secrets.CS_ACCESS_TOKEN }} and pass access-token: "
        f"${{{{ env.CS_ACCESS_TOKEN }}}}; it has {upload.step!r}"
    )


def test_the_credential_is_bound_nowhere_else(upload: Upload) -> None:
    """A wider scope puts the secret in reach of steps with no use for it."""
    strays = stray_credential_sites(upload.workflow, upload.document, upload.path)
    assert not strays, f"CS_ACCESS_TOKEN is read outside the upload step: {strays}"


def test_the_publisher_never_cancels_a_run(upload: Upload) -> None:
    """Publisher runs are grouped, and a run in progress is never cancelled.

    A cancelled run abandons both its upload and its ratchet baseline
    write. Without cancelling, GitHub keeps one pending run per group,
    so a newer push replaces a pending one and the newest baseline wins.
    """
    concurrency = upload.document.get("concurrency")
    assert isinstance(concurrency, dict), (
        f"{upload.workflow} must declare a concurrency group: {concurrency!r}"
    )
    assert concurrency.get("group"), (
        f"{upload.workflow} names no group: {concurrency!r}"
    )
    scopes = cancelling_scopes(upload.document)
    assert not scopes, f"{upload.workflow} may cancel a publisher run at {scopes}"
