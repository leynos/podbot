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

WORKFLOWS_DIRECTORY: typ.Final[pathlib.Path] = (
    pathlib.Path(__file__).resolve().parents[1] / ".github" / "workflows"
)


#: The suffixes GitHub runs a workflow under, compared case-folded.
WORKFLOW_SUFFIXES: typ.Final[tuple[str, ...]] = (".yml", ".yaml")


class _UniqueKeyLoader(yaml.SafeLoader):
    """`yaml.SafeLoader` refusing a mapping that declares a key twice.

    PyYAML keeps the last of two equal keys and says nothing, so a job
    declaring `runs-on` twice parses into a document holding only the
    second value, and the placement rule reads a half GitHub may not run.
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

    Both YAML extensions are read, compared case-folded: a lane in the
    other one, or spelt `CI.YML`, would otherwise escape every contract
    without failing anything.

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
    paths = (
        p
        for p in sorted(directory.iterdir())
        if p.suffix.casefold() in WORKFLOW_SUFFIXES
    )
    for path in paths:
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
        If the text is not a YAML mapping, or a mapping in it declares one
        key twice.

    Examples
    --------
    >>> parse("ci.yml", "jobs:\n  build:\n    runs-on: ubuntu-latest\n")
    {'jobs': {'build': {'runs-on': 'ubuntu-latest'}}}
    """
    try:
        document = yaml.load(text, Loader=_UniqueKeyLoader)  # noqa: S506 - a SafeLoader subclass
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
