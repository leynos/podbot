"""CV-005: CodeScene coverage belongs to main, and to main alone.

Concordat rule ``main-owned-codescene-coverage``. One workflow uploads
coverage to CodeScene, the one running on pushes to main, and no
workflow a pull request can reach names the upload action, invokes
``cs-coverage``, puts ``CS_ACCESS_TOKEN`` in reach of a process, or
names the ``codescene.io`` host.

The rule is a policy rather than a gap. A pull request from a fork
cannot read ``CS_ACCESS_TOKEN``, so the changed-line check was a silent
skip for exactly the contributions least likely to have been measured.
On a branch it put a second tool on the critical path, and when the
CodeScene project stopped returning a gates configuration that tool
failed every pull request here over a defect in none of them. The
ratchet applies the same gate from this repository's own baseline and
needs no token.

Run via ``make test-workflow-contracts``.
"""

from __future__ import annotations

import pathlib
import typing as typ

import pytest
from codescene_coverage import (
    CODESCENE_ACTION,
    COVERAGE_ACTION,
    PINNED_COMMIT,
    coverage_steps,
    invokes,
    is_refused_call,
    publishers,
    pull_request_workflows,
)
from codescene_reach import action_sites, cli_sites, codescene_contacts, token_sites
from shell_commands import runs_unconditionally
from workflow_reading import job_steps, read_workflows, workflow_jobs, workflow_steps

if typ.TYPE_CHECKING:  # pragma: no cover - typing only
    from collections.abc import Callable

    from workflow_reading import WorkflowDocument

REPOSITORY_ROOT: typ.Final[pathlib.Path] = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS: typ.Final[pathlib.Path] = REPOSITORY_ROOT / ".github" / "workflows"

#: The command that runs this suite in CI.
CONTRACT_COMMAND: typ.Final[str] = "make test-workflow-contracts"

#: The inputs deciding what a coverage run measures, as opposed to what
#: happens to the report afterwards.
SELECTION: typ.Final[frozenset[str]] = frozenset(
    {"output-path", "format", "features", "with-default-features", "language"}
)


@pytest.fixture(scope="module")
def documents() -> dict[str, WorkflowDocument]:
    """Return this repository's workflows, parsed once."""
    return read_workflows(WORKFLOWS)


#: Each prohibition on the pull-request lane, by the reading that finds it.
PROHIBITIONS: typ.Final[dict[str, Callable[[str, WorkflowDocument], list[str]]]] = {
    "the upload action": action_sites,
    "the cs-coverage command": cli_sites,
    "the CS_ACCESS_TOKEN secret": token_sites,
    "the codescene.io host": codescene_contacts,
}


@pytest.mark.parametrize(
    "subject",
    [pytest.param(subject, id=subject.split()[1]) for subject in sorted(PROHIBITIONS)],
)
def test_no_pull_request_workflow_reaches_codescene(
    documents: dict[str, WorkflowDocument], subject: str
) -> None:
    """No workflow a pull request can reach names CodeScene by any road.

    One case per road, so a failure names the road. Each runs over the
    closure through reusable-workflow calls rather than a trigger list,
    so a ``workflow_call`` workflow called with ``secrets: inherit`` is
    held to the same rules as its caller.
    """
    reading = PROHIBITIONS[subject]
    offenders = sorted(
        site
        for name, document in pull_request_workflows(documents).items()
        for site in reading(name, document)
    )
    assert not offenders, (
        f"these pull-request lanes name {subject}; CV-005 keeps CodeScene "
        f"off the pull-request lane by any road: {offenders}"
    )


def test_no_pull_request_job_calls_this_repository_by_a_ref(
    documents: dict[str, WorkflowDocument],
) -> None:
    """A self-call at a ref runs a version the closure cannot read.

    ``leynos/podbot/.github/workflows/x.yml@main`` and
    ``$/.github/workflows/x.yml@main`` both name a workflow here, but
    run it as it stands at that ref. Following the checked-out file
    would prove nothing about what runs, so both are refused.
    """
    offenders = sorted(
        f"{name}: job {job_name} uses {job.get('uses')}"
        for name, document in pull_request_workflows(documents).items()
        for job_name, job in workflow_jobs(document).items()
        if is_refused_call(job.get("uses"))
    )
    assert not offenders, (
        f"call these workflows by `./` or `$/` without a ref: {offenders}"
    )


def test_exactly_one_workflow_publishes_coverage(
    documents: dict[str, WorkflowDocument],
) -> None:
    """One publisher, so the baseline and the upload have one writer.

    Two would race on the ratchet baseline. None would leave every pull
    request ratcheting against a baseline nobody writes, which passes
    silently and measures nothing.
    """
    found = sorted(publishers(documents))
    uploading = sorted(
        name
        for name, document in documents.items()
        if any(invokes(step, CODESCENE_ACTION) for step in workflow_steps(document))
    )
    assert len(found) == 1, f"expected exactly one publisher; found {found}"
    assert uploading == found, (
        f"the workflows invoking {CODESCENE_ACTION} must be exactly the "
        f"publisher {found}; they are {uploading}"
    )


def test_every_coverage_lane_names_one_pinned_commit(
    documents: dict[str, WorkflowDocument],
) -> None:
    """One generator commit across the lanes, and a commit, not a branch.

    A pull request's coverage is compared with the baseline main wrote,
    so two lanes on different generator versions compare two tools, and
    a partial repin is invisible in a diff that moves the other lane.
    """
    pins = {
        f"{name}: {step['uses']}": str(step["uses"]).removeprefix(f"{COVERAGE_ACTION}@")
        for name, lane in coverage_steps(documents).items()
        for step in lane
    }
    assert len(pins) >= 2, f"expected a pull-request lane and a publisher: {pins}"
    unpinned = sorted(
        site for site, pin in pins.items() if not PINNED_COMMIT.match(pin)
    )
    assert not unpinned, f"pin these to a full forty-character commit: {unpinned}"
    assert len(set(pins.values())) == 1, (
        f"the coverage lanes name different commits: {pins}"
    )


def _pull_request_lanes(documents: dict[str, WorkflowDocument]) -> list[str]:
    """Return the pull-request workflows that generate coverage, failing on none.

    A lane is a workflow that generates coverage, so housekeeping
    workflows such as Dependabot automerge are rightly outside it; but
    finding no lane at all is the reader failing, not compliance.
    """
    steps = coverage_steps(documents)
    lanes = [name for name in pull_request_workflows(documents) if name in steps]
    assert lanes, "no pull-request workflow generates coverage; the reader is broken"
    return lanes


def _the_only_leg(steps: list[dict[str, object]], name: str) -> dict[str, object]:
    """Return a lane's single coverage step's inputs, failing if there are more."""
    assert len(steps) == 1, f"{name} runs {len(steps)} coverage legs; pair them first"
    inputs = steps[0].get("with")
    assert isinstance(inputs, dict), f"{name}'s coverage step has no `with:` mapping"
    return inputs


def test_the_ratcheting_lane_matches_its_baseline(
    documents: dict[str, WorkflowDocument],
) -> None:
    """A ratchet against a differently built baseline measures the builds.

    The inputs deciding what is compiled and reported must agree, or
    the ratchet compares two builds rather than two commits, and it
    reports a number either way.
    """
    steps = coverage_steps(documents)
    (publisher,) = publishers(documents)
    baseline = _the_only_leg(steps[publisher], publisher)
    ours = {key: value for key, value in baseline.items() if key in SELECTION}
    for name in _pull_request_lanes(documents):
        theirs = {
            key: value
            for key, value in _the_only_leg(steps.get(name, []), name).items()
            if key in SELECTION
        }
        assert theirs == ours, f"{name} builds {theirs}; {publisher} builds {ours}"


def test_every_pull_request_lane_ratchets_unconditionally(
    documents: dict[str, WorkflowDocument],
) -> None:
    """The ratchet is the gate CV-005 leaves, so it must stay switched on.

    Deleting ``with-ratchet``, or guarding the step with an ``if:``,
    leaves every other clause green while the lane stops gating. The
    publisher must ratchet too, since its run writes the baseline.
    """
    steps = coverage_steps(documents)
    (publisher,) = publishers(documents)
    for name in [publisher, *_pull_request_lanes(documents)]:
        (step,) = steps[name]
        inputs = step.get("with")
        ratchet = inputs.get("with-ratchet") if isinstance(inputs, dict) else None
        assert ratchet == "true", f"{name} must ratchet; with-ratchet is {ratchet!r}"
        assert "if" not in step, (
            f"{name}'s coverage step is guarded: {step.get('if')!r}"
        )


def test_the_pull_request_lane_runs_these_contracts(
    documents: dict[str, WorkflowDocument],
) -> None:
    """A contract nothing runs protects nothing.

    Required as a step's sole command, in an unguarded step of an
    unguarded job: ``echo make test-workflow-contracts`` contains the
    words, ``false && make test-workflow-contracts`` contains the
    command, and an ``if:`` that is never true skips it; none of them
    runs the suite.
    """
    running = [
        f"{name}: job {job_name}"
        for name, document in pull_request_workflows(documents).items()
        for job_name, job in workflow_jobs(document).items()
        if "if" not in job
        for step in job_steps(job)
        if "if" not in step
        and runs_unconditionally(str(step.get("run", "")), CONTRACT_COMMAND)
    ]
    assert running, (
        f"no pull-request job runs `{CONTRACT_COMMAND}` as an unguarded step"
    )
