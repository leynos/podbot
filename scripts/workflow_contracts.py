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
import typing as typ

import yaml

#: Every reference to the shared-actions repository is matched by this
#: prefix rather than by an enumerated list of action paths. An
#: enumeration goes stale the moment a workflow adopts a new action from
#: the same repository, and the reference it misses is exactly the one
#: nobody thought to add.
SHARED_ACTIONS_PREFIX: typ.Final[str] = "leynos/shared-actions/"

#: Pins of the shared-actions repository whose `setup-rust` installs
#: sccache and exports no `RUSTC_WRAPPER`, so Cargo routes no
#: compilation through it. A reference on any of these leaves sccache
#: downloaded and started on every Rust job, caching nothing and
#: reporting nothing.
#:
#: The export landed on shared-actions at `c6125f1` on 2026-09-04, so
#: every commit before it belongs to this class. These six are the ones
#: reachable from this repository today: the last two are what every
#: reference here sat on until the repin that added this file, and
#: `57a33fa6` is what Dependabot's open group bump (#164) proposes. That
#: one moves the pins three weeks FORWARD and still stops short of the
#: export, so it reads as pins being brought up to date while leaving
#: the cache off. Being newer is not the property that matters; carrying
#: the export is.
WRAPPER_LESS_PINS: typ.Final[frozenset[str]] = frozenset(
    {
        "074f7d8ba75a6e5d18532b72cbe38fccbda4e9c6",
        "18bed1ca49a6de3d8882bd72635a32ae3f023d57",
        "32c8ea649ea44d40119f348ad48861212532061f",
        "57a33fa65e329db7edc81ece661f1a2e1d39868f",
        "1c1a46f0b4fde6dd78a9757fc475a5d06ee891c7",
        "d3cbe87e745e07b3ad53ddcb87deb19ffa95c9b8",
    }
)

#: The action whose cargo invocation the watchdog bounds.
COVERAGE_ACTION: typ.Final[str] = (
    "leynos/shared-actions/.github/actions/generate-coverage"
)

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
    """Return every workflow file's text, keyed by file name.

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
    """Return one workflow's parsed document.

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
    texts: typ.Mapping[str, str],
) -> tuple[SharedActionsReference, ...]:
    """Return every `uses:` naming the shared-actions repository.

    Parameters
    ----------
    texts : typ.Mapping[str, str]
        Workflow file name to file text.

    Returns
    -------
    tuple[SharedActionsReference, ...]
        One entry per reference, in file and job order.
    """
    return tuple(
        reference
        for workflow, text in texts.items()
        for job in of_type(parse(workflow, text).get("jobs"), dict).values()
        for value in _uses_values(of_type(job, dict))
        if (reference := _reference(workflow, value)) is not None
    )


def watchdog_of(document: dict[str, object], job: dict[str, object]) -> object:
    """Return the watchdog budget in force for one job.

    Both levels GitHub resolves are read, innermost first. A contract
    reading only the job would report a workflow-level declaration as
    absent, which is exactly backwards.

    Parameters
    ----------
    document : dict[str, object]
        The whole workflow document.
    job : dict[str, object]
        The parsed job.

    Returns
    -------
    object
        The declared value, or None when neither level declares one.
        The value is returned as written so a contract can refuse a
        blank declaration, which parses to "" or to None depending on
        its spelling and masks the outer scope either way.
    """
    for owner in (job, document):
        env = of_type(owner.get("env"), dict)
        if WATCHDOG_VARIABLE in env:
            return env[WATCHDOG_VARIABLE]
    return None


def coverage_jobs(
    texts: typ.Mapping[str, str],
) -> tuple[tuple[str, str, object], ...]:
    """Return every job invoking the coverage action, with its watchdog.

    Parameters
    ----------
    texts : typ.Mapping[str, str]
        Workflow file name to file text.

    Returns
    -------
    tuple[tuple[str, str, object], ...]
        Workflow name, job name and declared watchdog value.
    """
    found: list[tuple[str, str, object]] = []
    for workflow, text in texts.items():
        document = parse(workflow, text)
        for name, job in of_type(document.get("jobs"), dict).items():
            job_map = of_type(job, dict)
            steps = [
                of_type(step, dict) for step in of_type(job_map.get("steps"), list)
            ]
            if not any(COVERAGE_ACTION in str(step.get("uses", "")) for step in steps):
                continue
            found.append((workflow, str(name), watchdog_of(document, job_map)))
    return tuple(found)
