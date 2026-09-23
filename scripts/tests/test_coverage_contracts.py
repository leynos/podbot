"""The coverage steps: the watchdog that bounds them, and their cache reports.

The **watchdog** rule exists because the budget was inherited. An
inherited default is a value this repository never states and cannot
notice changing. The **cache report** rule exists because the repin that
made sccache work is only observable through its counters.
"""

from __future__ import annotations

import typing as typ

import pytest
from workflow_contracts import of_type
from workflow_contracts import parse as parse_workflow
from workflow_coverage import (
    CACHE_REPORT_COMMAND,
    COVERAGE_ACTION,
    WATCHDOG_VARIABLE,
    cache_reports,
)
from workflow_coverage import coverage_jobs as coverage_jobs_in

#: The budget this repository runs its coverage steps under, in seconds.
#: It is the action's current default, declared here so that a later
#: change to that default cannot move it silently. The worst coverage
#: step observed over six green runs is 487 s, so this is about 3.7
#: times the worst seen, and both coverage jobs pass
#: `use-cargo-nextest: 'false'`, which leaves the watchdog as the only
#: timer bounding cargo.
REQUIRED_WATCHDOG: typ.Final[str] = "1800"


def test_every_coverage_job_states_its_watchdog(
    workflow_texts: dict[str, str],
) -> None:
    """An inherited watchdog is a budget this repository never stated.

    The value is pinned as well as required, because the action's
    default is what it replaces: leaving the assertion at "some value is
    set" would let the budget drift from the one measured here without
    failing anything.
    """
    jobs = coverage_jobs_in(workflow_texts)

    assert jobs, (
        f"no job invoking {COVERAGE_ACTION} was found; the reader matches the "
        f"action path, so an empty result means the reader is broken"
    )
    for workflow, job, watchdog in jobs:
        assert watchdog is not None, (
            f"{workflow}:{job} runs the coverage action without declaring "
            f"{WATCHDOG_VARIABLE}, so it inherits the action's default"
        )
        assert str(watchdog) == REQUIRED_WATCHDOG, (
            f"{workflow}:{job} sets {WATCHDOG_VARIABLE} to {watchdog!r}, not "
            f"the {REQUIRED_WATCHDOG}s this repository measured and states"
        )


def test_a_longer_action_path_is_not_the_coverage_action() -> None:
    """The reader matches the path exactly, not as a substring.

    A substring also selects an action whose path extends this one, such as
    a `generate-coverage-disabled`. The discovery assertion next door only
    checks the result is non-empty, so a false-positive match would satisfy
    it and the watchdog assertion would then be about the wrong step.

    Driven over a constructed workflow because this repository contains no
    such action: parametrized over the real files the rule passes whether it
    compares paths or searches for text.
    """
    text = (
        "jobs:\n  test:\n    env:\n"
        f"      {WATCHDOG_VARIABLE}: '1800'\n"
        "    steps:\n"
        f"      - uses: {COVERAGE_ACTION}-disabled@abc\n"
        f"      - uses: {COVERAGE_ACTION}-v2@abc\n"
    )

    assert coverage_jobs_in({"ci.yml": text}) == ()


def test_the_coverage_action_itself_is_found_at_any_ref() -> None:
    """Narrow as well as sufficient: the real path must still match.

    A rule that refused everything would pass the test above and break the
    contract it serves, so the accepting case is asserted beside it.
    """
    text = (
        "jobs:\n  test:\n    env:\n"
        f"      {WATCHDOG_VARIABLE}: '1800'\n"
        "    steps:\n"
        f"      - uses: {COVERAGE_ACTION}@0123456789abcdef0123456789abcdef01234567\n"
    )

    assert coverage_jobs_in({"ci.yml": text}) == (("ci.yml", "test", "1800"),)


@pytest.mark.parametrize(
    ("scope", "expected"),
    [
        ("workflow", "1800"),
        ("job", "1800"),
        ("step", "1800"),
        ("none", None),
    ],
)
def test_the_watchdog_is_read_at_every_scope(scope: str, expected: object) -> None:
    """Step, then job, then workflow, innermost first.

    A step-level `env` overrides the job's, which overrides the workflow's,
    so a resolver reading only the outer two can report the stated budget
    while the action receives another. The real files declare it at one
    scope, so only constructed workflows show the precedence at all.
    """
    declaration = f"    env:\n      {WATCHDOG_VARIABLE}: '{REQUIRED_WATCHDOG}'\n"
    text = (
        (declaration.replace("    ", "", 1) if scope == "workflow" else "")
        + "jobs:\n  test:\n"
        + (declaration if scope == "job" else "")
        + "    steps:\n"
        + f"      - uses: {COVERAGE_ACTION}@abc\n"
        + (
            f"        env:\n          {WATCHDOG_VARIABLE}: '{REQUIRED_WATCHDOG}'\n"
            if scope == "step"
            else ""
        )
    )
    jobs = coverage_jobs_in({"ci.yml": text})

    assert len(jobs) == 1
    assert jobs[0][2] == expected


def test_an_inner_scope_wins_over_an_outer_one() -> None:
    """The precedence itself, not merely that each scope is read.

    Asserting each scope separately is satisfied by a resolver that reads
    them in the wrong order, because only one is ever set in those cases.
    """
    text = (
        f"env:\n  {WATCHDOG_VARIABLE}: '60'\n"
        "jobs:\n  test:\n"
        f"    env:\n      {WATCHDOG_VARIABLE}: '120'\n"
        "    steps:\n"
        f"      - uses: {COVERAGE_ACTION}@abc\n"
        f"        env:\n          {WATCHDOG_VARIABLE}: '1800'\n"
    )
    jobs = coverage_jobs_in({"ci.yml": text})

    assert jobs[0][2] == "1800"


def test_two_coverage_steps_in_one_job_report_separately() -> None:
    """One row per coverage step, so a disagreeing second step is visible.

    Reporting one watchdog per job would let a second invocation carry a
    different budget with nothing to say so.
    """
    text = (
        "jobs:\n  test:\n    steps:\n"
        f"      - uses: {COVERAGE_ACTION}@abc\n"
        f"        env:\n          {WATCHDOG_VARIABLE}: '1800'\n"
        f"      - uses: {COVERAGE_ACTION}@abc\n"
        f"        env:\n          {WATCHDOG_VARIABLE}: '60'\n"
    )
    jobs = coverage_jobs_in({"ci.yml": text})

    assert [job[2] for job in jobs] == ["1800", "60"]


def test_every_coverage_step_is_followed_by_a_cache_report(
    workflow_texts: dict[str, str],
) -> None:
    """The repin's only evidence is the counters, so the report must run.

    The repin this branch exists for is invisible in a green lane: on the
    pins used before, sccache was installed and started on every Rust job
    and Cargo routed nothing through it, and the only symptom was slowness.
    The report step is what turns that back into something a reader can
    check, so removing it, guarding it, or moving it before the compiling
    work would leave the change unevidenced while every other rule here
    still passed.
    """
    reports = cache_reports(workflow_texts)

    assert reports, (
        f"no step invoking {COVERAGE_ACTION} was found; the reader matches "
        f"the action path, so an empty result means the reader is broken"
    )
    for report in reports:
        assert report.follows_coverage, (
            f"{report.workflow}:{report.job} runs the coverage action at step "
            f"{report.coverage_index} with no {CACHE_REPORT_COMMAND!r} step "
            f"after it, so nothing reports whether anything reached the cache"
        )
        assert report.job_guard is None, (
            f"{report.workflow}:{report.job} guards the whole job with "
            f"{report.job_guard!r}, which can skip the cache report with it"
        )
        assert report.guard == "always()", (
            f"{report.workflow}:{report.job} guards its cache report with "
            f"{report.guard!r}. It must be always(): a red lane is exactly "
            f"when the counters are worth having, and a report that runs only "
            f"on success is absent whenever it would say most"
        )


@pytest.mark.parametrize(
    ("steps", "follows", "guard"),
    [
        (
            "      - uses: {action}@abc\n      - if: always()\n        run: {report}\n",
            True,
            "always()",
        ),
        ("      - uses: {action}@abc\n", False, None),
        (
            "      - if: always()\n        run: {report}\n      - uses: {action}@abc\n",
            False,
            None,
        ),
        (
            "      - uses: {action}@abc\n"
            "      - if: success()\n        run: {report}\n",
            True,
            "success()",
        ),
        ("      - uses: {action}@abc\n      - run: {report}\n", True, None),
        (
            "      - uses: {action}@abc\n"
            "      - if: always()\n        run: echo {report}\n",
            False,
            None,
        ),
    ],
    ids=["after", "absent", "before", "wrongly-guarded", "unguarded", "echoed"],
)
def test_the_cache_report_is_read_by_position_and_guard(
    steps: str, follows: bool, guard: object
) -> None:
    """Driven over constructed workflows, because the real files are correct.

    Parametrized over `ci.yml` and `coverage-main.yml` the rule passes
    whether or not it reads position, guard or spelling at all. Two cases
    carry the weight. A report *before* the coverage step is present,
    spelled correctly, and reports on nothing. An echoed report satisfies a
    substring search while running nothing, which is the same defect the
    contract-command rule next door exists to refuse.
    """
    text = "jobs:\n  test:\n    steps:\n" + steps.format(
        action=COVERAGE_ACTION, report=CACHE_REPORT_COMMAND
    )
    found = cache_reports({"ci.yml": text})

    assert len(found) == 1
    assert found[0].follows_coverage is follows
    assert found[0].guard == guard


def test_a_job_with_no_coverage_step_contributes_nothing() -> None:
    """Narrow as well as sufficient.

    A job that never runs the coverage action needs no cache report, and a
    rule demanding one everywhere would fail every other job in the file.
    """
    text = "jobs:\n  lint:\n    steps:\n      - run: make check-fmt\n"

    assert cache_reports({"ci.yml": text}) == ()


@pytest.mark.parametrize(
    ("job_guard", "expected"),
    [
        ("", None),
        ("    if: false\n", False),
        ("    if: ${{ github.x }}\n", "${{ github.x }}"),
    ],
    ids=["unguarded-job", "job-false", "job-expression"],
)
def test_the_cache_report_carries_its_job_guard(
    job_guard: str, expected: object
) -> None:
    """A guard on the job stops the report as surely as one on the step.

    The real coverage jobs are unguarded, so only constructed jobs show
    the reader looking at the job scope at all.
    """
    text = (
        "jobs:\n  test:\n"
        f"{job_guard}"
        "    steps:\n"
        f"      - uses: {COVERAGE_ACTION}@abc\n"
        f"      - if: always()\n        run: {CACHE_REPORT_COMMAND}\n"
    )
    (found,) = cache_reports({"ci.yml": text})

    assert found.job_guard == expected


#: The two steps that turn the cache report into a verdict, in order.
HEALTH_STEPS: typ.Final[tuple[str, ...]] = (
    "sccache --show-stats --stats-format json > sccache-stats.json",
    "python3 scripts/check_sccache_health.py --expect-location ghac sccache-stats.json",
)


def test_every_cache_report_is_checked_for_health(
    workflow_texts: dict[str, str],
) -> None:
    """Printing the counters is not checking them.

    A lane whose sccache bound local disk, wrapped nothing, or failed every
    store still compiles and stays green, and the report above would say so
    only to someone reading it. After each report, the lane writes the
    statistics as JSON and runs the health check on them, each as its own
    step and in that order, and neither is guarded by anything but
    `always()`, so a red lane is still judged.
    """
    for report in cache_reports(workflow_texts):
        document = parse_workflow(report.workflow, workflow_texts[report.workflow])
        job = of_type(of_type(document.get("jobs"), dict).get(report.job), dict)
        later = [of_type(step, dict) for step in of_type(job.get("steps"), list)][
            report.report_index + 1 :
        ]
        runs = [str(step.get("run", "")).strip() for step in later]
        positions = [
            runs.index(command) if command in runs else -1 for command in HEALTH_STEPS
        ]
        assert -1 not in positions and positions == sorted(positions), (
            f"{report.workflow}:{report.job} must run, after its cache report "
            f"and in this order: {HEALTH_STEPS}; it runs {runs}"
        )
        guards = {later[index].get("if") for index in positions}
        assert guards == {"always()"}, (
            f"{report.workflow}:{report.job} guards its health steps with {guards}"
        )
