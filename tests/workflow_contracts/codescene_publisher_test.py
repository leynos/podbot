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
from publisher_rules import MAIN_REF_CONJUNCT, cancelling_scopes, guard_conjuncts
from token_check import (
    TOKEN_CHECK_COMMAND,
    available_conjunct,
    is_token_check,
    passes_the_secret_directly,
    stray_credential_sites,
)
from workflow_reading import read_workflows, workflow_jobs

if typ.TYPE_CHECKING:  # pragma: no cover - typing only
    from workflow_reading import WorkflowDocument

REPOSITORY_ROOT: typ.Final[pathlib.Path] = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS: typ.Final[pathlib.Path] = REPOSITORY_ROOT / ".github" / "workflows"


class Upload(typ.NamedTuple):
    """The publisher's upload step, with where it sits and what precedes it."""

    workflow: str
    document: WorkflowDocument
    path: str
    step: dict[str, object]
    earlier: list[tuple[str, dict[str, object]]]


def _raw_steps(job: dict[str, object]) -> list[object]:
    """Return a job's steps list as written, so indices match the document's paths."""
    steps = job.get("steps")
    return steps if isinstance(steps, list) else []


@pytest.fixture(scope="module")
def upload() -> Upload:
    """Return the publisher's single CodeScene upload step."""
    ((name, document),) = publishers(read_workflows(WORKFLOWS)).items()
    found = [
        Upload(
            name,
            document,
            f"jobs.{job_name}.steps[{index}]",
            step,
            [
                (f"jobs.{job_name}.steps[{position}]", earlier)
                for position, earlier in enumerate(_raw_steps(job)[:index])
                if isinstance(earlier, dict)
            ],
        )
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


def _token_check(upload: Upload) -> tuple[str, dict[str, object]]:
    """Return the single token-check step that precedes the upload in its job."""
    checks = [(path, step) for path, step in upload.earlier if is_token_check(step)]
    assert len(checks) == 1, (
        f"exactly one step before the upload must have an id and run "
        f"{TOKEN_CHECK_COMMAND!r} as its sole, unguarded command; "
        f"found {[path for path, _ in checks]}"
    )
    return checks[0]


def test_the_token_is_checked_in_a_step_of_its_own(upload: Upload) -> None:
    """The check step exists, in its one allowed shape, before the upload.

    GitHub reads a missing step output as an empty string, so without this
    the guard below stays well formed with the check deleted, and the upload
    skips on every run with nothing failing.
    """
    _token_check(upload)


def test_the_publisher_uploads_only_from_main(upload: Upload) -> None:
    """The ref and the token check are conjuncts, and no ``||`` bypasses them.

    The publisher declares ``workflow_dispatch``, which can name any
    branch, so without the ref test a feature branch's coverage could
    publish as the trunk's and every pull request would ratchet against it.
    """
    _, check = _token_check(upload)
    condition = str(upload.step.get("if", ""))
    conjuncts = guard_conjuncts(condition)
    assert conjuncts is not None, f"the upload guard contains `||`: {condition!r}"
    assert MAIN_REF_CONJUNCT in conjuncts, (
        f"the upload step must be guarded on {MAIN_REF_CONJUNCT} as one `&&` "
        f"term; it is guarded on {condition!r}"
    )
    required = available_conjunct(str(check["id"]))
    assert required in conjuncts, (
        f"the upload step must skip, not fail, when the secret is absent, by "
        f"reading the check step's output as {required!r}: {condition!r}"
    )


def test_the_upload_takes_the_secret_only_as_its_input(upload: Upload) -> None:
    """The composite action would pass its step's env to every nested step."""
    assert passes_the_secret_directly(upload.step), (
        "the upload step must pass access-token: ${{ secrets.CS_ACCESS_TOKEN }} "
        f"and bind nothing named CS_ACCESS_TOKEN in its env; it has {upload.step!r}"
    )


def test_the_credential_is_bound_nowhere_else(upload: Upload) -> None:
    """A wider scope puts the secret in reach of steps with no use for it."""
    check_path, _ = _token_check(upload)
    strays = stray_credential_sites(
        upload.workflow, upload.document, check_path, upload.path
    )
    assert not strays, f"CS_ACCESS_TOKEN is read outside its two sites: {strays}"


def test_the_publisher_never_cancels_a_run(upload: Upload) -> None:
    """Publisher runs are grouped, and a run in progress is never cancelled.

    A cancelled run abandons both its upload and its ratchet baseline
    write. Without cancelling, GitHub keeps at most one pending run per
    group and a run queued later replaces it, so a burst of pushes
    publishes the last one queued.
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
