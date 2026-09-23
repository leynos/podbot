"""The publisher's upload guard and concurrency, as readings.

``coverage-main.yml`` answers ``workflow_dispatch`` as well as a push to
``main``, and a dispatch can name any branch, so the trigger filter does
not confine the upload; the step's own condition does. That condition
is read as a conjunction: split on ``&&``, with an unquoted ``||``
anywhere refused rather than interpreted, because
``... && github.ref == 'refs/heads/main' || github.event_name == 'workflow_dispatch'``
contains the ref test as a substring and makes every conjunct optional.

How the secret reaches the upload is read in ``token_check``.
"""

from __future__ import annotations

import re
import typing as typ

if typ.TYPE_CHECKING:  # pragma: no cover - typing only
    from workflow_reading import WorkflowDocument

#: The conjunct confining a step to the trunk, in its canonical spelling.
MAIN_REF_CONJUNCT: typ.Final[str] = "github.ref == 'refs/heads/main'"

#: A single-quoted expression string, which may itself contain ``||``.
_QUOTED: typ.Final[re.Pattern[str]] = re.compile(r"'(?:[^']|'')*'")

#: A whole-value ``${{ }}`` expression, with the whitespace GitHub allows.
_WRAPPER: typ.Final[re.Pattern[str]] = re.compile(
    r"^\$\{\{(?P<body>.*)\}\}$", re.DOTALL
)


def _unwrapped(value: str) -> str:
    """Return an expression without its ``${{ }}`` wrapper, if it has one."""
    stripped = value.strip()
    wrapped = _WRAPPER.match(stripped)
    return (wrapped.group("body") if wrapped else stripped).strip()


def guard_conjuncts(condition: str) -> list[str] | None:
    """Return a condition's ``&&`` conjuncts, or ``None`` when it has a ``||``.

    Only operators outside quoted strings count, so a literal ``'||'``
    does not make a condition a disjunction.

    >>> guard_conjuncts("${{ env.A != '' && github.ref == 'refs/heads/main' }}")
    ["env.A != ''", "github.ref == 'refs/heads/main'"]
    >>> guard_conjuncts("github.ref == 'refs/heads/main' || env.A != ''") is None
    True
    """
    body = _unwrapped(condition)
    blanked = _QUOTED.sub(lambda match: "_" * len(match.group()), body)
    if "||" in blanked:
        return None
    bounds = [
        0,
        *(end for match in re.finditer("&&", blanked) for end in match.span()),
        len(body),
    ]
    return [
        " ".join(body[start:end].split())
        for start, end in zip(bounds[::2], bounds[1::2])
    ]


def requires(condition: object, conjunct: str) -> bool:
    """Return whether a step condition holds only when a conjunct holds.

    >>> requires("env.A != '' && github.ref  ==  'refs/heads/main'", MAIN_REF_CONJUNCT)
    True
    >>> requires(None, MAIN_REF_CONJUNCT)
    False
    """
    if not isinstance(condition, str):
        return False
    conjuncts = guard_conjuncts(condition)
    return conjuncts is not None and conjunct in conjuncts


def _mapping(value: object) -> dict[str, object]:
    """Return a value when it is a mapping, and an empty one otherwise."""
    return value if isinstance(value, dict) else {}


def _cancels(concurrency: object) -> bool:
    """Return whether a ``concurrency`` value may cancel a run in progress.

    Fails closed: an expression may evaluate to true, so only an explicit
    ``false``, or no setting at all, reads as not cancelling.
    """
    if not isinstance(concurrency, dict):
        return False
    return str(concurrency.get("cancel-in-progress", "false")).strip() != "false"


def cancelling_scopes(document: WorkflowDocument) -> list[str]:
    """Return every scope of a workflow whose concurrency may cancel a run.

    >>> cancelling_scopes({"concurrency": {"group": "g", "cancel-in-progress": "true"}})
    ['workflow']
    >>> cancelling_scopes({"jobs": {"a": {"concurrency": {"group": "g"}}}})
    []
    """
    scopes = ["workflow"] if _cancels(document.get("concurrency")) else []
    jobs = _mapping(document.get("jobs"))
    scopes += [
        f"job {job_name}"
        for job_name, job in jobs.items()
        if _cancels(_mapping(job).get("concurrency"))
    ]
    return scopes
