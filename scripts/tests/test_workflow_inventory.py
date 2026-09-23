"""Exactly what the readers find in this repository's own workflows.

Every contract elsewhere asserts over what a reader returns, and checks
only that the result is not empty. A reader that dropped one workflow, one
job-level `uses:`, or one coverage step would still return something, and
every rule over its result would pass while that entry went unguarded.
So the inventory is asserted exactly here, entry by entry.

Changing a workflow in a way that adds or removes an entry fails this
module deliberately. Update the expected inventory in the same commit,
which is the point: the reader's coverage is then a reviewed fact rather
than an assumption.
"""

from __future__ import annotations

import typing as typ

from workflow_contracts import shared_actions_references
from workflow_coverage import cache_reports, coverage_jobs
from workflow_placement import line_break_fault, runs_on_declarations

#: Every shared-actions reference, as workflow and action or workflow name.
#: The last is a job-level reusable-workflow call, which a reader of steps
#: alone would miss.
SHARED_ACTIONS: typ.Final[list[tuple[str, str]]] = [
    ("audit.yml", "setup-rust"),
    ("ci.yml", "setup-rust"),
    ("ci.yml", "generate-coverage"),
    ("coverage-main.yml", "setup-rust"),
    ("coverage-main.yml", "generate-coverage"),
    ("coverage-main.yml", "upload-codescene-coverage"),
    ("dependabot-automerge.yml", "dependabot-automerge.yml"),
]

#: Every coverage step, as workflow and job.
COVERAGE_JOBS: typ.Final[list[tuple[str, str]]] = [
    ("ci.yml", "build-test"),
    ("coverage-main.yml", "coverage-upload"),
]

#: Every runner declaration, as workflow, job and value.
RUNNERS: typ.Final[list[tuple[str, str, object]]] = [
    ("audit.yml", "audit", "ubuntu-latest"),
    ("ci.yml", "build-test", "ubuntu-latest"),
    ("coverage-main.yml", "coverage-upload", "ubuntu-latest"),
]


def test_every_shared_actions_reference_is_found(
    workflow_texts: dict[str, str],
) -> None:
    """All seven, including the job-level reusable-workflow call."""
    found = [
        (reference.workflow, reference.path.rsplit("/", 1)[-1])
        for reference in shared_actions_references(workflow_texts)
    ]
    assert found == SHARED_ACTIONS


def test_a_job_level_reusable_workflow_call_is_found() -> None:
    """Driven over a constructed call, so the job scope is proved to be read.

    The real inventory above would also fail if the call were dropped, but
    only while this repository keeps one; this case keeps the reader honest
    after that.
    """
    text = (
        "jobs:\n  call:\n    uses: leynos/shared-actions/.github/workflows/x.yml@abc\n"
    )
    found = shared_actions_references({"auto.yml": text})
    assert [(r.workflow, r.path, r.ref) for r in found] == [
        ("auto.yml", "leynos/shared-actions/.github/workflows/x.yml", "abc")
    ]


def test_every_coverage_step_is_found_with_its_watchdog(
    workflow_texts: dict[str, str],
) -> None:
    """Both coverage jobs, each with the budget it states."""
    found = [(workflow, job) for workflow, job, _ in coverage_jobs(workflow_texts)]
    assert found == COVERAGE_JOBS


def test_every_coverage_step_has_its_cache_report_found(
    workflow_texts: dict[str, str],
) -> None:
    """One report row per coverage step, each unguarded at the job."""
    found = [
        (report.workflow, report.job, report.guard, report.job_guard)
        for report in cache_reports(workflow_texts)
    ]
    assert found == [
        (workflow, job, "always()", None) for workflow, job in COVERAGE_JOBS
    ]


def test_every_runner_declaration_is_found_raw_and_parsed(
    workflow_texts: dict[str, str],
) -> None:
    """Each declaration's source text is kept beside its parsed value.

    A literal label reads the same both ways, so the raw text must equal
    the value here; the folded-scalar case below is where they differ.
    """
    declarations = runs_on_declarations(workflow_texts)
    assert [(d.workflow, d.job, d.value) for d in declarations] == RUNNERS
    assert [d.raw for d in declarations] == [value for _, _, value in RUNNERS]


def test_the_raw_declaration_keeps_a_folded_scalar_as_written() -> None:
    """The raw text is the source, line break and indent included."""
    text = (
        "jobs:\n  build:\n    runs-on: >-\n"
        "      ${{ github.event.pull_request.head.repo.fork\n"
        "        && 'ubuntu-latest' || 'ubicloud' }}\n"
        "    steps: []\n"
    )
    (declaration,) = runs_on_declarations({"ci.yml": text})
    assert declaration.raw == (
        ">-\n      ${{ github.event.pull_request.head.repo.fork\n"
        "        && 'ubuntu-latest' || 'ubicloud' }}\n"
    )
    assert line_break_fault(declaration.value) is not None


def test_no_runner_placement_carries_a_line_break(
    workflow_texts: dict[str, str],
) -> None:
    """The real files, asserted against the rule driven below."""
    declarations = runs_on_declarations(workflow_texts)

    assert declarations, "no job declares runs-on; the reader is broken"
    for declaration in declarations:
        fault = line_break_fault(declaration.value)
        assert fault is None, (
            f"{declaration.workflow}:{declaration.job} has a runs-on carrying "
            f"a line break, which GitHub evaluates as written. The value "
            f"parsed as {fault!r}, from:\n{declaration.raw}"
        )
