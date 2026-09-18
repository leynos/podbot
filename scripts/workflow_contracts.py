"""Readers over the workflow documents the contracts assert against.

The contracts next door say what must be true. This module is how the
workflow files are turned into something to say it about, and it is kept
separate for two reasons.

The first is that a reader can be wrong while no workflow is wrong, and a
reader exercised only against this repository's own files cannot show
that: parametrized over four correct documents it passes whether or not
it discriminates anything. Separating the reading lets the contracts
drive it with documents built in the test, including the ones this
repository does not contain and should never contain.

The second is that the raw text matters as much as the parsed value. A
folded scalar whose continuation is indented more deeply than its first
line keeps the line break, and the resulting `runs-on` carries a newline
inside an expression GitHub evaluates anyway. The parse tolerates it, so
a reader returning only the parsed value cannot refuse it, and a green
run is not evidence that it is absent.
"""

from __future__ import annotations

import pathlib
import collections.abc as cabc
import typing as typ

import yaml

#: Every reference to the shared-actions repository is matched by this
#: prefix rather than by an enumerated list of action paths. An
#: enumeration goes stale the moment a workflow adopts a new action from
#: the same repository, and the reference it misses is exactly the one
#: nobody thought to add.
SHARED_ACTIONS_PREFIX: typ.Final[str] = "leynos/shared-actions/"

#: Pins of the shared-actions repository whose `setup-rust` is known to
#: export `RUSTC_WRAPPER` and select a backend, so a reference on one of
#: them compiles through sccache.
#:
#: An allowlist, and the direction matters more than the contents. The
#: first draft of this contract named the pins known to be *wrapper-less*,
#: and a reviewer pointed out that the set cannot be complete: any commit
#: outside it passes every other rule here while restoring exactly the
#: no-cache state this file exists to prevent, and Dependabot chooses from
#: the whole history rather than from a list. Refusing an unknown pin
#: until someone checks it fails closed; accepting one until someone
#: blacklists it fails open, and the failure is silent because a cache
#: that is never consulted reports nothing.
#:
#: The export landed at `c6125f1` on 2026-09-04. A descendant of it on
#: shared-actions' default branch belongs here, once someone has verified
#: that descent; that verification is the cost of a repin, and it is the
#: work this contract is asking for rather than an obstacle to it.
WRAPPER_EXPORTING_PINS: typ.Final[frozenset[str]] = frozenset(
    {
        # 0e3c4d24, 2026-09-14. Verified descendant of `c6125f1`, and of
        # both pins this repository used before the repin that added this
        # file.
        "0e3c4d24e43aa48b511d94f3b902711eb02138df",
    }
)

#: The commit at which `setup-rust` began exporting the wrapper, named so
#: a diagnostic can tell a reader what to check rather than only that the
#: pin is unknown.
WRAPPER_EXPORT_COMMIT: typ.Final[str] = "c6125f1"

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

WORKFLOWS_DIRECTORY: typ.Final[pathlib.Path] = (
    pathlib.Path(__file__).resolve().parents[1] / ".github" / "workflows"
)


class WorkflowReadError(RuntimeError):
    """Raised when a workflow file cannot be read or parsed.

    The queries below are pure and take documents. Reading them is the
    one fallible step, so it reports its own failure rather than letting
    a parser's exception surface from what reads like a query.
    """


class SharedActionsReference(typ.NamedTuple):
    """One `uses:` naming the shared-actions repository.

    Attributes
    ----------
    workflow : str
        The workflow file's name.
    path : str
        The action or reusable workflow path, without the ref.
    ref : str
        Whatever follows the `@`, which a contract checks is a 40-hex
        commit rather than assuming it.
    """

    workflow: str
    path: str
    ref: str


def load_workflow_documents(
    directory: pathlib.Path | None = None,
) -> dict[str, str]:
    r"""Return every workflow file's text, keyed by file name.

    Both YAML extensions are read: a lane in the other one would
    otherwise escape every contract without failing anything.

    Parameters
    ----------
    directory : pathlib.Path or None
        Where to read from. Defaults to this repository's workflows.

    Returns
    -------
    dict[str, str]
        File name to file text.

    Raises
    ------
    WorkflowReadError
        If a file cannot be read or decoded.

    Examples
    --------
    >>> import pathlib, tempfile
    >>> with tempfile.TemporaryDirectory() as directory:
    ...     path = pathlib.Path(directory, "ci.yml")
    ...     _ = path.write_text("jobs: {}\n", encoding="utf-8")
    ...     sorted(load_workflow_documents(pathlib.Path(directory)))
    ['ci.yml']
    """
    directory = WORKFLOWS_DIRECTORY if directory is None else directory
    texts: dict[str, str] = {}
    for path in sorted(directory.glob("*.y*ml")):
        try:
            texts[path.name] = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError) as error:
            message = f"{path.name} could not be read: {error}"
            raise WorkflowReadError(message) from error
    return texts


def parse(workflow: str, text: str) -> dict[str, object]:
    r"""Return one workflow's parsed document.

    Parameters
    ----------
    workflow : str
        The file's name, for the message.
    text : str
        The file's text.

    Returns
    -------
    dict[str, object]
        The parsed document.

    Raises
    ------
    WorkflowReadError
        If the text is not a YAML mapping.

    Examples
    --------
    >>> parse("ci.yml", "jobs:\n  build:\n    runs-on: ubuntu-latest\n")
    {'jobs': {'build': {'runs-on': 'ubuntu-latest'}}}
    """
    try:
        document = yaml.safe_load(text)
    except yaml.YAMLError as error:
        message = f"{workflow} could not be parsed: {error}"
        raise WorkflowReadError(message) from error
    if not isinstance(document, dict):
        message = f"{workflow} is not a mapping"
        raise WorkflowReadError(message)
    return document


def of_type[T](value: object, kind: type[T]) -> T:
    """Return `value` when it has the expected shape, else an empty one.

    One function rather than a pair of near-identical ones. A workflow
    document is a tree of `object`, and every step of a walk down it has
    to say what it expected and what to do when the file says something
    else; saying "an empty one of those" everywhere keeps the walks flat
    and keeps a malformed file from raising out of what reads as a
    query.

    Parameters
    ----------
    value : object
        The parsed value.
    kind : type[T]
        The shape expected, `dict` or `list`.

    Returns
    -------
    T
        The value, or an empty instance of `kind`.

    Examples
    --------
    >>> of_type({"a": 1}, dict)
    {'a': 1}
    >>> of_type("not a list", list)
    []
    """
    return value if isinstance(value, kind) else kind()


def _uses_values(job: dict[str, object]) -> list[object]:
    """Return every `uses:` in one job, its own and its steps'.

    Both spellings are read. A contract reading only steps would miss a
    reusable-workflow caller, and that is one of the references that has
    to move with the rest.

    Parameters
    ----------
    job : dict[str, object]
        The parsed job.

    Returns
    -------
    list[object]
        The declared values, in the order they appear.
    """
    steps = of_type(job.get("steps"), list)
    return [job.get("uses"), *(of_type(step, dict).get("uses") for step in steps)]


def _reference(workflow: str, value: object) -> SharedActionsReference | None:
    """Return the shared-actions reference `value` names, if it names one.

    Parameters
    ----------
    workflow : str
        The workflow file's name.
    value : object
        A `uses:` value, which may be anything the file said.

    Returns
    -------
    SharedActionsReference or None
        The reference, or None when the value names something else.
    """
    if not isinstance(value, str) or not value.startswith(SHARED_ACTIONS_PREFIX):
        return None
    path, _, ref = value.partition("@")
    return SharedActionsReference(workflow, path, ref)


def shared_actions_references(
    texts: cabc.Mapping[str, str],
) -> tuple[SharedActionsReference, ...]:
    r"""Return every `uses:` naming the shared-actions repository.

    Parameters
    ----------
    texts : cabc.Mapping[str, str]
        Workflow file name to file text.

    Returns
    -------
    tuple[SharedActionsReference, ...]
        One entry per reference, in file and job order.

    Examples
    --------
    >>> text = (
    ...     "jobs:\n  build:\n    steps:\n"
    ...     "      - uses: leynos/shared-actions/.github/actions/setup-rust@abc\n"
    ... )
    >>> [(r.path.rsplit("/", 1)[-1], r.ref) for r in shared_actions_references({"ci.yml": text})]
    [('setup-rust', 'abc')]
    """
    return tuple(
        reference
        for workflow, text in texts.items()
        for job in of_type(parse(workflow, text).get("jobs"), dict).values()
        for value in _uses_values(of_type(job, dict))
        if (reference := _reference(workflow, value)) is not None
    )


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


class CommandStep(typ.NamedTuple):
    """One step running a given command, with the guards around it.

    Attributes
    ----------
    job : str
        The owning job's name. Kept because a guard on the job disables the
        step as surely as a guard on the step, and a collection that drops
        the owner cannot see it.
    job_guard : object
        The job's `if:`, or `None`.
    step_guard : object
        The step's `if:`, or `None`.
    """

    job: str
    job_guard: object
    step_guard: object

    @property
    def can_run(self) -> bool:
        """Report whether nothing guards this step.

        Returns
        -------
        bool
            True when neither the step nor its job declares an `if:`.

        Examples
        --------
        >>> CommandStep("lint", None, None).can_run
        True
        >>> CommandStep("lint", False, None).can_run
        False
        """
        return self.job_guard is None and self.step_guard is None

    def describe_guards(self) -> str:
        """Return the guards found, for a diagnostic.

        Returns
        -------
        str
            A description naming each guard, or "nothing" when unguarded.

        Examples
        --------
        >>> CommandStep("lint", False, "${{ false }}").describe_guards()
        "job if: False; step if: '${{ false }}'"
        >>> CommandStep("lint", None, None).describe_guards()
        'nothing'
        """
        parts = [
            f"{scope} if: {guard!r}"
            for scope, guard in (
                ("job", self.job_guard),
                ("step", self.step_guard),
            )
            if guard is not None
        ]
        return "; ".join(parts) if parts else "nothing"


def command_steps(document: dict[str, object], command: str) -> tuple[CommandStep, ...]:
    r"""Return every step whose `run:` is exactly ``command``.

    The comparison is equality on the stripped value rather than a
    substring search. A search is satisfied by ``echo 'make
    workflow-contracts'``, by the text inside a shell comment, and by any
    command that merely mentions it, so a contract written that way asserts
    that a string appears rather than that anything runs.

    Each match carries its owning job, because a step with no `if:` inside a
    job with ``if: false`` is dead code that an assertion about the step
    alone cannot see.

    Parameters
    ----------
    document : dict[str, object]
        A parsed workflow document.
    command : str
        The command the step must run, exactly.

    Returns
    -------
    tuple[CommandStep, ...]
        One entry per matching step.

    Examples
    --------
    >>> text = (
    ...     "jobs:\n  lint:\n    if: false\n    steps:\n"
    ...     "      - run: make workflow-contracts\n"
    ... )
    >>> found = command_steps(parse("ci.yml", text), "make workflow-contracts")
    >>> found[0].can_run, found[0].describe_guards()
    (False, 'job if: False')

    >>> text = "jobs:\n  lint:\n    steps:\n      - run: echo 'make x'\n"
    >>> command_steps(parse("ci.yml", text), "make x")
    ()
    """
    found: list[CommandStep] = []
    for name, job in of_type(document.get("jobs"), dict).items():
        job_map = of_type(job, dict)
        for step in of_type(job_map.get("steps"), list):
            step_map = of_type(step, dict)
            if str(step_map.get("run", "")).strip() != command:
                continue
            found.append(
                CommandStep(
                    job=str(name),
                    job_guard=job_map.get("if"),
                    step_guard=step_map.get("if"),
                )
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
    """

    workflow: str
    job: str
    coverage_index: int
    report_index: int
    guard: object

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
            steps = [
                of_type(step, dict)
                for step in of_type(of_type(job, dict).get("steps"), list)
            ]
            reports = [
                index
                for index, step in enumerate(steps)
                if str(step.get("run", "")).strip() == CACHE_REPORT_COMMAND
            ]
            for coverage_index, step in enumerate(steps):
                if str(step.get("uses", "")).partition("@")[0] != COVERAGE_ACTION:
                    continue
                later = [index for index in reports if index > coverage_index]
                report_index = later[0] if later else -1
                found.append(
                    CacheReport(
                        workflow=workflow,
                        job=str(name),
                        coverage_index=coverage_index,
                        report_index=report_index,
                        guard=(
                            steps[report_index].get("if") if report_index >= 0 else None
                        ),
                    )
                )
    return tuple(found)
