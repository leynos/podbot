"""The runner-placement rule, and the reading it rests on.

Kept apart from the pin and watchdog contracts because it is the one
rule this repository's own files cannot prove. Every job here names a
literal runner, so a check parametrized over them passes whether or not
it discriminates anything; the rule is a function taking a parsed value
so the contracts can drive it directly, in both directions, with the
documents this repository does not contain.

The defect it refuses: a folded scalar whose continuation line is
indented more deeply than its first line keeps the line break, so the
value carries a newline inside the expression. GitHub evaluates it
regardless, so a green run is not evidence the defect is absent.
"""

from __future__ import annotations

import collections.abc as cabc
import typing as typ

import yaml

from workflow_contracts import of_type, parse


class RunsOn(typ.NamedTuple):
    """One job's runner placement, parsed value beside raw declaration.

    Attributes
    ----------
    workflow : str
        The workflow file's name.
    job : str
        The job's identifier.
    value : object
        The parsed value: a string, a list, or a mapping.
    raw : str
        The declaration as written, taken from the file rather than
        re-serialized, because the defect being refused is a line break
        the parse has already absorbed.
    """

    workflow: str
    job: str
    value: object
    raw: str


def _node_at(node: object, path: tuple[str, ...]) -> typ.Any | None:
    """Return the composed node at `path`, or None when it is absent.

    Parameters
    ----------
    node : object
        The node to walk from.
    path : tuple[str, ...]
        Mapping keys, outermost first.

    Returns
    -------
    typ.Any or None
        The node, or None when any key along the way is missing.
    """
    for key in path:
        pairs = dict(_node_pairs(node))
        if key not in pairs:
            return None
        node = pairs[key]
    return node


def _raw_runs_on(text: str, job: str) -> str:
    """Return the `runs-on` declaration for one job as written.

    Taken from the composed node's source marks rather than by scanning
    for indentation, so the continuation lines of a folded scalar come
    back whatever their indent, which is the case this exists to report.

    Parameters
    ----------
    text : str
        The workflow file's text.
    job : str
        The job's identifier.

    Returns
    -------
    str
        The declaration as written, or the empty string when the job
        declares none or the document cannot be composed.
    """
    try:
        root = yaml.compose(text)
    except yaml.YAMLError:
        return ""
    node = _node_at(root, ("jobs", job, "runs-on"))
    if node is None:
        return ""
    return text[node.start_mark.index : node.end_mark.index]


def _node_pairs(node: object) -> list[tuple[str, typ.Any]]:
    """Return a mapping node's key and value nodes, keys as strings.

    Parameters
    ----------
    node : object
        A composed node, or anything else.

    Returns
    -------
    list[tuple[str, typ.Any]]
        One pair per entry, empty when the node is not a mapping.
    """
    if not isinstance(node, yaml.MappingNode):
        return []
    return [
        (str(key.value), value)
        for key, value in node.value
        if isinstance(key, yaml.ScalarNode)
    ]


def runs_on_declarations(texts: cabc.Mapping[str, str]) -> tuple[RunsOn, ...]:
    r"""Return every job's runner placement, parsed beside raw.

    Parameters
    ----------
    texts : cabc.Mapping[str, str]
        Workflow file name to file text.

    Returns
    -------
    tuple[RunsOn, ...]
        One entry per job that declares `runs-on`.

    Examples
    --------
    >>> text = "jobs:\n  build:\n    runs-on: ubuntu-latest\n"
    >>> [(d.job, d.value) for d in runs_on_declarations({"ci.yml": text})]
    [('build', 'ubuntu-latest')]
    """
    found: list[RunsOn] = []
    for workflow, text in texts.items():
        document = parse(workflow, text)
        for name, job in of_type(document.get("jobs"), dict).items():
            value = of_type(job, dict).get("runs-on")
            if value is None:
                continue
            found.append(
                RunsOn(workflow, str(name), value, _raw_runs_on(text, str(name)))
            )
    return tuple(found)


def line_break_fault(value: object) -> str | None:
    """Return the first string in `value` carrying an internal line break.

    A `runs-on` may be a string, a list of labels or a `group`/`labels`
    mapping, and the defect can sit in any of them, so every string in
    the value is examined rather than only the top-level one. Refusing
    anything but a string would reject two shapes GitHub accepts, which
    is a different rule from the one being asserted.

    A trailing break is not a fault: a folded scalar written with `>`
    rather than `>-` keeps one, and GitHub strips it. What matters is a
    break *inside* the value, which is what an over-indented
    continuation produces.

    Parameters
    ----------
    value : object
        The parsed `runs-on` value.

    Returns
    -------
    str or None
        The offending string, or None when there is none.

    Examples
    --------
    >>> line_break_fault("ubuntu-latest") is None
    True
    >>> broken = "${{ a" + chr(10) + "  && 'b' || 'c' }}"
    >>> line_break_fault(broken) == broken
    True
    >>> line_break_fault(["ubuntu-latest", broken]) == broken
    True
    >>> line_break_fault("trailing break is not a fault" + chr(10)) is None
    True
    """
    if isinstance(value, str):
        return value if "\n" in value.rstrip("\n") else None
    if isinstance(value, list):
        faults = [line_break_fault(item) for item in value]
    elif isinstance(value, dict):
        faults = [line_break_fault(item) for item in value.values()]
    else:
        return None
    return next((fault for fault in faults if fault is not None), None)
