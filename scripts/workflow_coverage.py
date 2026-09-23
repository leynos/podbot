"""The coverage steps, the watchdog that bounds them, and their cache reports.

Split from `workflow_contracts`, which reads the documents, so that
neither module passes the 400-line limit `AGENTS.md` sets. Every reading
here is keyed on the coverage action and returns one row per coverage
step, so a job running the action twice cannot hide one of them.
"""

from __future__ import annotations

import collections.abc as cabc
import typing as typ

from workflow_contracts import of_type, parse

#: The action whose cargo invocation the watchdog bounds. Matched against
#: the part of `uses` before the `@`, not as a substring: a substring also
#: selects `generate-coverage-disabled@ref` and any other action whose path
#: extends this one, and the discovery assertion next door would then be
#: satisfied by the wrong step.
COVERAGE_ACTION: typ.Final[str] = (
    "leynos/shared-actions/.github/actions/generate-coverage"
)

#: The command that reports what reached the compiler cache. The repin
#: this contract guards is only observable through these counters, so a
#: report step that is absent, guarded, or placed before the compiling
#: work would leave the repin unevidenced while every other rule passed.
CACHE_REPORT_COMMAND: typ.Final[str] = "sccache --show-stats"

#: The variable naming that watchdog. It takes precedence over the
#: action's `cargo-wait-timeout` input, so a caller pinning the budget
#: sets it here.
WATCHDOG_VARIABLE: typ.Final[str] = "RUN_RUST_CARGO_WAIT_TIMEOUT"


def watchdog_of(
    document: dict[str, object],
    job: dict[str, object],
    step: dict[str, object] | None = None,
) -> object:
    """Return the watchdog budget in force for one coverage step.

    All three levels GitHub resolves are read, innermost first: the step's
    own `env`, then the job's, then the workflow's. A contract reading only
    the job reports the job's value while the action receives the step's,
    so it can assert that 1800 is in force when it is not. A contract
    reading only the job would equally report a workflow-level declaration
    as absent, which is the same error in the other direction.

    Parameters
    ----------
    document : dict[str, object]
        The whole workflow document.
    job : dict[str, object]
        The parsed job.
    step : dict[str, object] or None
        The coverage step, when the caller has matched one. Omitted only
        by a caller that has no step in hand.

    Returns
    -------
    object
        The declared value, or None when neither level declares one.
        The value is returned as written so a contract can refuse a
        blank declaration, which parses to "" or to None depending on
        its spelling and masks the outer scope either way.

    Examples
    --------
    >>> document = {"env": {"RUN_RUST_CARGO_WAIT_TIMEOUT": "600"}}
    >>> job = {"env": {"RUN_RUST_CARGO_WAIT_TIMEOUT": "1800"}}
    >>> watchdog_of(document, job)
    '1800'
    >>> watchdog_of(document, job, {"env": {"RUN_RUST_CARGO_WAIT_TIMEOUT": "60"}})
    '60'
    """
    scopes = (step, job, document) if step is not None else (job, document)
    for owner in scopes:
        env = of_type(owner.get("env"), dict)
        if WATCHDOG_VARIABLE in env:
            return env[WATCHDOG_VARIABLE]
    return None


def coverage_jobs(
    texts: cabc.Mapping[str, str],
) -> tuple[tuple[str, str, object], ...]:
    r"""Return every job invoking the coverage action, with its watchdog.

    Parameters
    ----------
    texts : cabc.Mapping[str, str]
        Workflow file name to file text.

    Returns
    -------
    tuple[tuple[str, str, object], ...]
        Workflow name, job name and declared watchdog value.

    Examples
    --------
    >>> text = (
    ...     "jobs:\n  test:\n    env:\n"
    ...     "      RUN_RUST_CARGO_WAIT_TIMEOUT: '1800'\n"
    ...     "    steps:\n      - uses: "
    ...     "leynos/shared-actions/.github/actions/generate-coverage@abc\n"
    ... )
    >>> coverage_jobs({"ci.yml": text})
    (('ci.yml', 'test', '1800'),)
    """
    found: list[tuple[str, str, object]] = []
    for workflow, text in texts.items():
        document = parse(workflow, text)
        for name, job in of_type(document.get("jobs"), dict).items():
            job_map = of_type(job, dict)
            steps = [
                of_type(step, dict) for step in of_type(job_map.get("steps"), list)
            ]
            coverage = [
                step
                for step in steps
                if str(step.get("uses", "")).partition("@")[0] == COVERAGE_ACTION
            ]
            if not coverage:
                continue
            # One entry per coverage step, not per job. A job invoking the
            # action twice gets two watchdogs, and reporting one would hide
            # whichever step disagreed.
            found.extend(
                (workflow, str(name), watchdog_of(document, job_map, step))
                for step in coverage
            )
    return tuple(found)


class CacheReport(typ.NamedTuple):
    """Where a cache report sits relative to the coverage step that precedes it.

    Attributes
    ----------
    workflow : str
        The workflow file's name.
    job : str
        The owning job's name.
    coverage_index : int
        The coverage step's position in the job's step list.
    report_index : int
        The report step's position, or -1 when the job has none.
    guard : object
        The report step's `if:`, or `None`.
    job_guard : object
        The owning job's `if:`, or `None`. A guard on the job stops the
        report as surely as one on the step, so it is carried too.
    """

    workflow: str
    job: str
    coverage_index: int
    report_index: int
    guard: object
    job_guard: object = None

    @property
    def follows_coverage(self) -> bool:
        """Report whether a report step exists after the coverage step.

        Returns
        -------
        bool
            True when a report step was found at a later index.

        Examples
        --------
        >>> CacheReport("ci.yml", "test", 3, 4, "always()").follows_coverage
        True
        >>> CacheReport("ci.yml", "test", 3, 1, "always()").follows_coverage
        False
        >>> CacheReport("ci.yml", "test", 3, -1, None).follows_coverage
        False
        """
        return self.report_index > self.coverage_index


def _report_indices(steps: list[dict[str, object]]) -> list[int]:
    """Return the positions of the steps running exactly the report command.

    Equality on the stripped value, not a search: `echo sccache --show-stats`
    is a step that reports nothing.

    Parameters
    ----------
    steps : list[dict[str, object]]
        One job's steps, in order.

    Returns
    -------
    list[int]
        Each matching step's index.

    Examples
    --------
    >>> _report_indices([{"run": "make"}, {"run": "sccache --show-stats"}])
    [1]
    >>> _report_indices([{"run": "echo sccache --show-stats"}])
    []
    """
    return [
        index
        for index, step in enumerate(steps)
        if str(step.get("run", "")).strip() == CACHE_REPORT_COMMAND
    ]


def _reports_in(
    workflow: str,
    job: str,
    steps: list[dict[str, object]],
    job_guard: object = None,
) -> list[CacheReport]:
    """Return one entry per coverage step in one job.

    Parameters
    ----------
    workflow : str
        The workflow file's name.
    job : str
        The job's name.
    steps : list[dict[str, object]]
        The job's steps, in order.
    job_guard : object
        The job's `if:`, or `None`.

    Returns
    -------
    list[CacheReport]
        One entry per coverage step, in order.

    Examples
    --------
    >>> steps = [
    ...     {"uses": COVERAGE_ACTION + "@abc"},
    ...     {"if": "always()", "run": CACHE_REPORT_COMMAND},
    ... ]
    >>> _reports_in("ci.yml", "test", steps)[0].follows_coverage
    True
    """
    reports = _report_indices(steps)
    found: list[CacheReport] = []
    for index, step in enumerate(steps):
        if str(step.get("uses", "")).partition("@")[0] != COVERAGE_ACTION:
            continue
        later = [position for position in reports if position > index]
        report_index = later[0] if later else -1
        found.append(
            CacheReport(
                workflow=workflow,
                job=job,
                coverage_index=index,
                report_index=report_index,
                guard=steps[report_index].get("if") if report_index >= 0 else None,
                job_guard=job_guard,
            )
        )
    return found


def cache_reports(texts: cabc.Mapping[str, str]) -> tuple[CacheReport, ...]:
    r"""Return one entry per coverage step, with the cache report that follows it.

    An entry is produced whether or not a report was found, so a missing
    report is a row with `report_index` of -1 rather than an absent row. A
    reader that simply omitted the job would make "no report anywhere" and
    "no coverage job at all" the same empty answer, and the contract could
    not tell which it was looking at.

    Parameters
    ----------
    texts : cabc.Mapping[str, str]
        Workflow file name to file text.

    Returns
    -------
    tuple[CacheReport, ...]
        One entry per coverage step.

    Examples
    --------
    >>> text = (
    ...     "jobs:\n  test:\n    steps:\n      - uses: "
    ...     "leynos/shared-actions/.github/actions/generate-coverage@abc\n"
    ...     "      - if: always()\n        run: sccache --show-stats\n"
    ... )
    >>> found = cache_reports({"ci.yml": text})
    >>> found[0].follows_coverage, found[0].guard
    (True, 'always()')
    """
    found: list[CacheReport] = []
    for workflow, text in texts.items():
        document = parse(workflow, text)
        for name, job in of_type(document.get("jobs"), dict).items():
            job_map = of_type(job, dict)
            steps = [
                of_type(step, dict) for step in of_type(job_map.get("steps"), list)
            ]
            found.extend(_reports_in(workflow, str(name), steps, job_map.get("if")))
    return tuple(found)
