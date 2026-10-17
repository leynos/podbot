"""Read workflow concurrency settings for pull-request cancellation contracts.

These suite-local queries retain the shared strict workflow reader; discovery
execution and parametrized assertions remain in concurrency_test.
"""

from __future__ import annotations

import re
from pathlib import Path

import yaml
from workflow_reading import WorkflowDocument, WorkflowReadingError, load_workflow


#: The repository's workflow directory. The module sits two levels below the
#: repository root, in `tests/workflow_contracts/`.
WORKFLOW_DIR = Path(__file__).resolve().parents[2] / ".github" / "workflows"

#: Extensions GitHub accepts for a workflow file. Scanning only `.yml` would
#: silently exempt a `.yaml` workflow from every contract here.
WORKFLOW_SUFFIXES: tuple[str, ...] = (".yml", ".yaml")

#: The exact `cancel-in-progress` expression every pull-request workflow
#: carries. Comparing against one string rather than searching for a substring
#: is what makes the literal `true` mutation fail: `true` is a YAML boolean and
#: never equals this.
CANCEL_EXPRESSION = "${{ github.event_name == 'pull_request' }}"

#: The trigger that puts a workflow in scope. `pull_request_target` is
#: deliberately absent; see the module docstring.
PULL_REQUEST = "pull_request"

#: Expressions that are unique to a single run. A group built from one of these
#: can never match another run, so it queues nothing and cancels nothing while
#: looking exactly like a concurrency control.
RUN_UNIQUE_EXPRESSIONS: tuple[str, ...] = (
    "github.run_id",
    "github.run_number",
    "github.run_attempt",
    "github.sha",
)

#: Expressions that differ between two pull requests. A group naming none of
#: them is shared by every branch, so one pull request's push would cancel
#: another's gates.
#: `github.head_ref` is deliberately absent: two pull requests from different
#: forks can share a source branch name, so a group keyed on it alone would
#: let one cancel the other. For a pull request, `github.ref` is
#: `refs/pull/<number>/merge`, which is unique.
PER_PULL_REQUEST_EXPRESSIONS: tuple[str, ...] = (
    "github.event.pull_request.number",
    "github.ref",
)

#: Workflows known to start on `pull_request`. Discovery below is dynamic so a
#: new workflow is covered the day it lands, but a dynamic list that silently
#: empties turns every parametrized test into a vacuous pass. This names the
#: floor discovery must still reach.
KNOWN_PULL_REQUEST_WORKFLOWS: frozenset[str] = frozenset(
    {
        "ci.yml",
    }
)


def _load(path: Path) -> WorkflowDocument:
    """Read and parse one workflow file through the shared strict reader.

    ``load_workflow`` keeps every scalar a string and refuses a mapping that
    declares a key twice, so a doubled ``concurrency`` or ``on`` cannot hide
    its first half. Every failure, unreadable, not YAML, or not a mapping, is
    raised as a ``WorkflowReadingError`` naming the file, so the fault is
    visible as the reader's rather than surfacing from inside a query.
    """
    try:
        return load_workflow(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, TypeError, yaml.YAMLError) as error:
        message = f"{path.name} could not be read as a workflow: {error}"
        raise WorkflowReadingError(
            message, reader="concurrency", path=str(path)
        ) from error


def _workflow_paths(directory: Path = WORKFLOW_DIR) -> list[Path]:
    """Return every workflow file in a directory, this repository's by default.

    Suffixes are compared case-folded, because GitHub runs `CI.YML` too.

    Returns
    -------
    list of Path
        Workflow paths sorted by name, so parametrized tests report in a
        stable order.
    """
    return sorted(
        path
        for path in directory.iterdir()
        if path.is_file() and path.suffix.casefold() in WORKFLOW_SUFFIXES
    )


def _trigger_names(document: dict[str, object]) -> frozenset[str] | None:
    """Return the event names a workflow declares under `on:`.

    GitHub accepts three shapes: a mapping of event to configuration, a list
    of event names, and a bare event name. All three are read here. A reader
    that modelled only the mapping would drop a workflow written either other
    way out of discovery, and a contract over a filtered list reports nothing
    at all about a workflow it never sees.

    PyYAML also resolves an unquoted `on:` key to the boolean ``True``, so
    both spellings of the key are read.

    Parameters
    ----------
    document
        A parsed workflow document.

    Returns
    -------
    frozenset of str, or None
        The declared event names, empty when the workflow declares no `on:`
        at all, in which case nothing can start it. ``None`` when `on:` is
        present in a shape this reader does not model, an explicit empty
        value included; the caller decides
        what to do about that, and
        `test_every_workflow_declares_a_trigger_set_this_reader_models`
        reports it by name rather than letting discovery drop it in silence.
    """
    if "on" not in document and True not in document:
        return frozenset()
    match document.get("on", document.get(True)):
        case dict() as events:
            return frozenset(key for key in events if isinstance(key, str))
        case list() as events:
            return frozenset(event for event in events if isinstance(event, str))
        case str() as event if event.strip():
            return frozenset({event})
        case _:
            # An explicit empty `on:` loads as "" or None. It is present but
            # names no event, which is a shape this reader does not model,
            # not an absent key.
            return None


def _pull_request_workflows() -> list[Path]:
    """Return every workflow a pull request can start.

    Returns
    -------
    list of Path
        Workflow paths declaring a `pull_request` trigger, in name order.
    """
    return [
        path
        for path in _workflow_paths()
        if PULL_REQUEST in (_trigger_names(_load(path)) or frozenset())
    ]


#: A `${{ }}` expression, and a single-quoted string literal inside one.
_EXPRESSION = re.compile(r"\$\{\{(?P<body>.*?)\}\}", re.DOTALL)
_QUOTED = re.compile(r"'(?:[^']|'')*'")


def _evaluated_text(value: str) -> str:
    """Return the parts of a value GitHub evaluates as context references.

    Only the bodies of `${{ }}` expressions are evaluated, and a quoted
    string inside one is a literal. So `github.ref` written bare, or
    `${{ 'github.event.pull_request.number' }}`, names the context without
    reading it, and yields a constant group.

    Examples
    --------
    >>> _evaluated_text("ci-${{ github.ref }}").strip()
    'github.ref'
    >>> "github.ref" in _evaluated_text("ci-github.ref-${{ 'github.ref' }}")
    False
    """
    bodies = (match.group("body") for match in _EXPRESSION.finditer(value))
    return " ".join(_QUOTED.sub("''", body) for body in bodies)


def _concurrency(path: Path) -> dict[str, object]:
    """Return a workflow's top-level concurrency mapping.

    Parameters
    ----------
    path
        Workflow file to read.

    Returns
    -------
    dict
        The concurrency mapping, empty when the workflow declares none or
        declares the shorthand string form, which cannot carry
        `cancel-in-progress` at all.
    """
    declared = _load(path).get("concurrency")
    return declared if isinstance(declared, dict) else {}


#: The one place a run-unique value may appear in a group: as the fallback a
#: non-pull-request event selects. A pull request always has a number, so it
#: never reaches the fallback, and every other event gets a group of its own.
_NON_PULL_REQUEST_FALLBACK = re.compile(
    r"github\.event\.pull_request\.number\s*\|\|\s*github\.run_id\b"
)


def _run_unique_names(group: str) -> list[str]:
    """Return the run-unique names a pull request's group would evaluate.

    Examples
    --------
    >>> _run_unique_names("${{ github.event.pull_request.number || github.run_id }}")
    []
    >>> _run_unique_names("${{ github.run_id || github.event.pull_request.number }}")
    ['github.run_id']
    """
    evaluated = _NON_PULL_REQUEST_FALLBACK.sub("", _evaluated_text(group))
    return [name for name in RUN_UNIQUE_EXPRESSIONS if name in evaluated]
