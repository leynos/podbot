"""Reading a GitHub Actions workflow, fallibly and visibly.

Generic: nothing here knows about CodeScene. The parsing, the trigger
grammar and the one filesystem call live together so that a contract
about any subject can use them.

Only ``read_workflows`` touches the filesystem, and it takes the
directory to read rather than finding one, so every reading above it can
be driven with supplied documents. That is what lets a contract ask what
a rule makes of a workflow this repository does not contain: the real
files use one spelling of everything and cannot tell a working reader
from a broken one.
"""

from __future__ import annotations

import fnmatch
import typing as typ

import yaml

if typ.TYPE_CHECKING:  # pragma: no cover - typing only
    import pathlib
    from collections.abc import Iterator

#: A parsed workflow document.
#:
#: The key type is ``str | bool`` rather than ``str``, and that is a
#: statement about YAML rather than defensiveness. YAML 1.1 resolves an
#: unquoted ``on:`` to a boolean, so a loader that resolves scalars keys
#: every workflow under ``True``. ``load_workflow`` keeps the string,
#: but a reader that declared ``dict[str, object]`` and then looked
#: under ``True`` would be claiming something the type says cannot
#: happen.
WorkflowDocument = dict[typ.Union[str, bool], object]

#: Triggers that mean a workflow serves pull requests. The second runs
#: with the base repository's secrets, which is the more dangerous of
#: the two to give a token to.
PULL_REQUEST_TRIGGERS: typ.Final[frozenset[str]] = frozenset(
    {"pull_request", "pull_request_target"}
)

#: The suffixes GitHub runs a workflow under, compared case-folded.
WORKFLOW_SUFFIXES: typ.Final[tuple[str, ...]] = (".yml", ".yaml")


class WorkflowReadingError(RuntimeError):
    """Raised when a reading here finds nothing it must have found.

    Every rule built on these readings is a refusal, and a refusal over
    an empty subject set is satisfied by any repository at all. So an
    empty reading is reported as a fault of the reader rather than
    returned, and it is a distinct type so that "this reader is broken"
    and "this repository complies" cannot be confused.
    """

    def __init__(self, message: str, *, reader: str, path: str | None = None) -> None:
        """Record the message, the reading that failed, and what it read."""
        super().__init__(message)
        self.reader = reader
        self.path = path


class _UniqueKeyLoader(yaml.BaseLoader):
    """``yaml.BaseLoader`` refusing a mapping that declares a key twice.

    PyYAML keeps the last of two equal keys and says nothing. A job
    declaring ``runs-on`` twice then parses into a document holding only
    the second value, so every contract reads the half GitHub may not.
    Refusing the document is the only reading that cannot be wrong about
    which half runs.
    """

    def construct_mapping(
        self, node: yaml.MappingNode, deep: bool = False
    ) -> dict[object, object]:
        """Construct one mapping, refusing a key already seen in it."""
        seen: set[object] = set()
        for key_node, _ in node.value:
            key = self.construct_object(key_node, deep=deep)
            if key in seen:
                raise yaml.constructor.ConstructorError(
                    "while constructing a mapping",
                    node.start_mark,
                    f"found duplicate key {key!r}",
                    key_node.start_mark,
                )
            seen.add(key)
        return super().construct_mapping(node, deep=deep)


def load_workflow(text: str) -> WorkflowDocument:
    r"""Parse a workflow while retaining every scalar as a string.

    Raises ``TypeError`` when the document is not a mapping, and
    ``yaml.YAMLError`` when it is not YAML or a mapping in it declares
    one key twice.

    >>> load_workflow("on:\n  push:\n")
    {'on': {'push': ''}}
    """
    parsed = yaml.load(text, Loader=_UniqueKeyLoader)  # noqa: S506 - BaseLoader is safe
    if not isinstance(parsed, dict):
        message = "a workflow must parse to a top-level mapping"
        raise TypeError(message)
    return typ.cast("WorkflowDocument", parsed)


def _declared_triggers(document: WorkflowDocument) -> object:
    """Return the ``on`` value under the string key or the boolean one."""
    return document.get("on", document.get(True))


def triggers(document: WorkflowDocument) -> frozenset[str]:
    r"""Return the trigger names a workflow declares.

    ``on`` is read through both the string key and the boolean ``True``,
    because a loader resolving YAML 1.1 scalars keys an unquoted ``on:``
    under the boolean, and a reader looking only for the string reports
    every workflow as triggerless. The value is read as a mapping, a
    sequence or a bare string, since GitHub accepts all three.

    >>> sorted(triggers(load_workflow("on:\n  pull_request:\n  push:\n")))
    ['pull_request', 'push']
    >>> sorted(triggers(load_workflow("on: [push, pull_request]")))
    ['pull_request', 'push']
    >>> triggers(load_workflow("name: x\n"))
    frozenset()
    """
    declared = _declared_triggers(document)
    if isinstance(declared, dict | list):
        return frozenset(str(name) for name in declared)
    if isinstance(declared, str):
        return frozenset({declared})
    return frozenset()


def serves_pull_requests(document: WorkflowDocument) -> bool:
    r"""Return whether a workflow's own triggers run it for a pull request.

    >>> serves_pull_requests(load_workflow("on: pull_request_target\n"))
    True
    >>> serves_pull_requests(load_workflow("on:\n  push:\n"))
    False
    """
    return bool(triggers(document) & PULL_REQUEST_TRIGGERS)


def _patterns(value: object) -> list[str]:
    """Return a filter's entries, which GitHub spells as a list or a scalar."""
    if isinstance(value, list):
        return [str(entry) for entry in value]
    return [value] if isinstance(value, str) else []


def _glob_matches(pattern: str, branch: str) -> bool:
    """Return whether one GitHub branch glob matches a branch name.

    GitHub's ``*`` stops at ``/`` and ``**`` does not; for a branch with
    no ``/`` in its name, such as ``main``, both reduce to ``fnmatch``.
    """
    return fnmatch.fnmatchcase(branch, pattern.replace("**", "*"))


def _filter_selects(patterns: list[str], branch: str) -> bool:
    """Return whether a ``branches`` filter, with ``!`` negations, selects a branch.

    GitHub reads the list in order and the last matching pattern wins,
    so ``['**', '!main']`` excludes main and ``['!main', 'main']`` does not.
    """
    selected = False
    for pattern in patterns:
        negated = pattern.startswith("!")
        if _glob_matches(pattern.removeprefix("!"), branch):
            selected = not negated
    return selected


def pushes_to_main(document: WorkflowDocument) -> bool:
    r"""Return whether a push to the main branch runs a workflow.

    Branch filters are read as GitHub reads them, as globs with ``!``
    negation, so ``'**'`` counts as naming main. A filter this reading
    does not recognize answers ``False``: this reading decides which
    workflow may publish coverage, so a shape it cannot understand must
    not be granted that permission by default.

    >>> pushes_to_main(load_workflow("on:\n  push:\n    branches: [main]\n"))
    True
    >>> pushes_to_main(load_workflow("on:\n  push:\n    branches: ['**']\n"))
    True
    >>> pushes_to_main(load_workflow("on:\n  push:\n    tags: ['v*']\n"))
    False
    """
    if "push" not in triggers(document):
        return False
    declared = _declared_triggers(document)
    filters = declared.get("push") if isinstance(declared, dict) else None
    if not isinstance(filters, dict) or not filters:
        return True
    if "branches" in filters:
        return _filter_selects(_patterns(filters["branches"]), "main")
    if "branches-ignore" in filters:
        return not any(
            _glob_matches(pattern, "main")
            for pattern in _patterns(filters["branches-ignore"])
        )
    # A tag filter alone fires for tag pushes only, which are not branch
    # pushes. Every other filter, `paths` among them, narrows which
    # pushes run the workflow without excluding main.
    return not {"tags", "tags-ignore"} & set(filters)


def workflow_jobs(document: WorkflowDocument) -> dict[str, dict[str, object]]:
    r"""Return a workflow's jobs that are mappings, by job name.

    A job that is not a mapping is dropped rather than raising: a
    malformed fragment is not this reading's subject, and refusing the
    whole document over one would let an unrelated workflow fail a
    contract about coverage.

    >>> sorted(workflow_jobs(load_workflow("jobs:\n  a:\n    steps: []\n  b: x\n")))
    ['a']
    """
    jobs = document.get("jobs")
    if not isinstance(jobs, dict):
        return {}
    return {str(name): job for name, job in jobs.items() if isinstance(job, dict)}


def job_steps(job: dict[str, object]) -> list[dict[str, object]]:
    r"""Return one job's steps that are mappings, in declaration order.

    >>> job_steps({"steps": [{"run": "true"}, "x"]})
    [{'run': 'true'}]
    >>> job_steps({"steps": "not-a-list"})
    []
    """
    steps = job.get("steps")
    if not isinstance(steps, list):
        return []
    return [step for step in steps if isinstance(step, dict)]


def workflow_steps(document: WorkflowDocument) -> list[dict[str, object]]:
    r"""Return every step of every job in one workflow.

    >>> body = "jobs:\n  a:\n    steps:\n      - run: echo one\n"
    >>> [step["run"] for step in workflow_steps(load_workflow(body))]
    ['echo one']
    """
    return [step for job in workflow_jobs(document).values() for step in job_steps(job)]


def scalars(value: object, where: str = "") -> Iterator[tuple[str, str]]:
    r"""Yield every key and scalar in a parsed document with the path reaching it.

    Keys are yielded as well as values, because a key is where a name
    is declared: an ``env`` binding, a forwarded secret and a
    ``workflow_call`` secret declaration all put the name in the key.
    A key is reported at the path it opens, with ``<key>`` appended.

    >>> list(scalars({"env": {"A": "b"}}))
    [('env<key>', 'env'), ('env.A<key>', 'A'), ('env.A', 'b')]
    """
    match value:
        case dict():
            for key, child in value.items():
                path = f"{where}.{key}" if where else str(key)
                yield f"{path}<key>", str(key)
                yield from scalars(child, path)
        case list():
            for index, child in enumerate(value):
                yield from scalars(child, f"{where}[{index}]")
        case _:
            yield where, str(value)


def _read_one(path: pathlib.Path) -> WorkflowDocument:
    """Return one workflow's parsed document, naming the file on failure."""
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as error:
        message = f"{path} could not be read: {error}"
        raise WorkflowReadingError(
            message, reader="read_workflows", path=str(path)
        ) from error
    try:
        return load_workflow(text)
    except (TypeError, ValueError, yaml.YAMLError) as error:
        # `yaml.YAMLError` is neither of the other two, so without it a
        # file that is not YAML escapes this boundary naming no file.
        message = f"{path} is not a workflow document: {error}"
        raise WorkflowReadingError(
            message, reader="read_workflows", path=str(path)
        ) from error


def read_workflows(directory: pathlib.Path) -> dict[str, WorkflowDocument]:
    """Return every workflow document under one directory, by file name.

    The only filesystem access here. The suffix is compared case-folded,
    because GitHub runs ``CI.YML`` as readily as ``ci.yml``, and a sweep
    that skipped it would report repository-wide coverage while ignoring
    a lane. Raises ``WorkflowReadingError`` when the directory holds no
    workflow, or one cannot be read or parsed.
    """
    found = {
        path.name: _read_one(path)
        for path in sorted(directory.iterdir())
        if path.is_file() and path.suffix.casefold() in WORKFLOW_SUFFIXES
    }
    if not found:
        message = (
            f"no workflow documents were read from {directory}; every "
            f"assertion built on this reading is satisfied by finding "
            f"nothing, so this is the reader failing rather than the "
            f"repository complying"
        )
        raise WorkflowReadingError(
            message, reader="read_workflows", path=str(directory)
        )
    return found
