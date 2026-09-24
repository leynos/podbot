"""How the CodeScene secret reaches the upload, and where else it may not.

The upload action is composite, and a composite action hands the calling
step's `env` to every step nested inside it, including artefact and cache
steps that have no use for the secret. So the secret is not bound on the
upload step at all. A step of its own runs one exact command, in which
GitHub evaluates `secrets.CS_ACCESS_TOKEN != ''` before the shell starts,
so the command writes only `available=true` or `available=false` to the
step's outputs and the token enters no process. The upload's guard reads
that output, and the action receives the secret through its
`access-token` input alone.

The positive shape is asserted, not only the absence of the old one.
GitHub reads a missing step output as an empty string, so a guard on
`steps.<id>.outputs.available == 'true'` stays well formed when the check
step is deleted, and the upload then skips on every run with nothing
failing.
"""

from __future__ import annotations

import re
import typing as typ

from codescene_reach import token_sites

if typ.TYPE_CHECKING:  # pragma: no cover - typing only
    from workflow_reading import WorkflowDocument

#: The secret, and the expression that reads it.
SECRET_NAME: typ.Final[str] = "CS_ACCESS_TOKEN"
SECRET_REFERENCE: typ.Final[str] = f"secrets.{SECRET_NAME}"

#: The one command the check step runs. The expression is evaluated before
#: the shell starts, so the shell sees `true` or `false`, never the secret.
TOKEN_CHECK_COMMAND: typ.Final[str] = (
    'echo "available=${{ secrets.CS_ACCESS_TOKEN != \'\' }}" >> "$GITHUB_OUTPUT"'
)

#: A whole-value `${{ }}` expression, with the whitespace GitHub allows.
_WRAPPER: typ.Final[re.Pattern[str]] = re.compile(
    r"^\$\{\{(?P<body>.*)\}\}$", re.DOTALL
)


def _expression(value: object) -> str | None:
    """Return a whole-value expression's body, whitespace-normalized, or `None`."""
    if not isinstance(value, str):
        return None
    match = _WRAPPER.match(value.strip())
    return " ".join(match.group("body").split()) if match else None


def _mapping(value: object) -> dict[str, object]:
    """Return a value when it is a mapping, and an empty one otherwise."""
    return value if isinstance(value, dict) else {}


def available_conjunct(step_id: str) -> str:
    """Return the guard conjunct that reads a check step's output.

    Parameters
    ----------
    step_id : str
        The check step's `id`.

    Returns
    -------
    str
        The conjunct, in the canonical spelling `guard_conjuncts` yields.

    Examples
    --------
    >>> available_conjunct("codescene-token")
    "steps.codescene-token.outputs.available == 'true'"
    """
    return f"steps.{step_id}.outputs.available == 'true'"


def is_token_check(step: dict[str, object]) -> bool:
    """Return whether a step is the token check, in its one allowed shape.

    It has an `id`, and its `run` is exactly the check command, so no
    wrapper such as `false && ...` or `echo ...` can contain the command
    without running it. It has no `if:` and no `continue-on-error`, either
    of which could stop it writing the output. It binds nothing: the token
    reaches the command only as the evaluated `true` or `false`, so no
    process, including a script checked out from a dispatched branch, ever
    holds it.

    Parameters
    ----------
    step : dict[str, object]
        One parsed workflow step.

    Returns
    -------
    bool
        True when the step has exactly that shape.

    Examples
    --------
    >>> is_token_check({"id": "t", "run": TOKEN_CHECK_COMMAND})
    True
    >>> is_token_check({"id": "t", "if": "always()", "run": TOKEN_CHECK_COMMAND})
    False
    """
    return (
        isinstance(step.get("id"), str)
        and "if" not in step
        and "continue-on-error" not in step
        and str(step.get("run", "")).strip() == TOKEN_CHECK_COMMAND
    )


def passes_the_secret_directly(step: dict[str, object]) -> bool:
    """Return whether an upload step takes the secret as its input only.

    Parameters
    ----------
    step : dict[str, object]
        The parsed upload step.

    Returns
    -------
    bool
        True when `access-token` is `${{ secrets.CS_ACCESS_TOKEN }}` and the
        step's `env` binds nothing of that name.

    Examples
    --------
    >>> passes_the_secret_directly({"with": {"access-token": "${{secrets.CS_ACCESS_TOKEN}}"}})
    True
    >>> passes_the_secret_directly({
    ...     "env": {"CS_ACCESS_TOKEN": "${{ secrets.CS_ACCESS_TOKEN }}"},
    ...     "with": {"access-token": "${{ env.CS_ACCESS_TOKEN }}"},
    ... })
    False
    """
    names = {str(name).casefold() for name in _mapping(step.get("env"))}
    passed = _expression(_mapping(step.get("with")).get("access-token"))
    return passed == SECRET_REFERENCE and SECRET_NAME.casefold() not in names


def stray_credential_sites(
    name: str, document: WorkflowDocument, check_path: str, upload_path: str
) -> list[str]:
    r"""Return every read of the secret other than the two allowed ones.

    The check step tests it in its `run` expression, and the upload step
    passes it as `access-token`. Anywhere else, any `env` on the job, the
    check step's and the upload step's own included, puts it in reach of a
    process.

    Parameters
    ----------
    name : str
        The workflow's file name, for the sites.
    document : WorkflowDocument
        The parsed workflow.
    check_path : str
        The check step's path, such as `jobs.a.steps[1]`.
    upload_path : str
        The upload step's path.

    Returns
    -------
    list[str]
        Every other site that reads or declares the secret.

    Examples
    --------
    >>> from workflow_reading import load_workflow
    >>> body = "env:\n  CS_ACCESS_TOKEN: x\njobs: {}\n"
    >>> stray_credential_sites("m.yml", load_workflow(body), "jobs.a.steps[0]", "jobs.a.steps[1]")
    ['m.yml: env.CS_ACCESS_TOKEN<key>']
    """
    allowed = {
        f"{name}: {check_path}.run",
        f"{name}: {upload_path}.with.access-token",
    }
    return [site for site in token_sites(name, document) if site not in allowed]
