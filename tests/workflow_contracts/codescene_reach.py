"""What puts CodeScene in reach of a lane, read over whole documents.

Separated from ``codescene_coverage``, whose subject is which workflows
a rule applies to. The subject here is what a selected workflow does
that a pull-request lane must not: name the upload action, run the CLI,
put the secret in reach of a process, or name the service's host.

Every reading visits every key and scalar of the parsed document rather
than a list of the places a breach is expected. Enumerated scopes have
been a way round each of these rules in turn: a workflow's top-level
``env``, ``defaults.run.shell``, a reusable call's ``with``, and a
callee's ``workflow_call`` secret declaration each reached a process
while a scope-by-scope reading looked elsewhere. These are prohibitions,
so over-matching is the safe direction; a false match fails loudly. The
parser discards comments, so prose explaining the policy is not read as
a breach of it.
"""

from __future__ import annotations

import re
import typing as typ

from codescene_coverage import CLI_COMMAND, CODESCENE_ACTION, FORBIDDEN_VARIABLE
from workflow_reading import scalars

if typ.TYPE_CHECKING:  # pragma: no cover - typing only
    from workflow_reading import WorkflowDocument

#: The service itself, reached by any road other than the action.
CODESCENE_HOST: typ.Final[str] = "codescene.io"

#: The upload action's own name, whatever owner, ref or case precedes it.
ACTION_NAME: typ.Final[str] = CODESCENE_ACTION.rsplit("/", 1)[-1]

#: A ``${{ }}`` expression, which may span lines in a block scalar.
_EXPRESSION: typ.Final[re.Pattern[str]] = re.compile(r"\$\{\{.*?\}\}", re.DOTALL)


def _is_key(where: str) -> bool:
    """Return whether a path from ``scalars`` names a key rather than a value."""
    return where.endswith("<key>")


def _names(text: str, needle: str) -> bool:
    """Return whether a text contains a needle, compared case-folded."""
    return needle.casefold() in text.casefold()


def _expression_reads(text: str) -> bool:
    """Return whether an expression in a value names the secret.

    Secret names are case-insensitive to GitHub, and an expression can
    reach one as ``secrets.X``, ``secrets['X']``, ``env.X`` or
    ``vars.X``; every one of those names it inside ``${{ }}``.
    """
    return any(_names(match, FORBIDDEN_VARIABLE) for match in _EXPRESSION.findall(text))


def _inherits(where: str, text: str) -> bool:
    """Return whether a value is a reusable call's ``secrets: inherit``.

    It names nothing, so a reading looking for the variable finds no
    mention of it while the called workflow receives every secret the
    caller holds.
    """
    return where.endswith(".secrets") and text.strip() == "inherit"


def token_sites(name: str, document: WorkflowDocument) -> list[str]:
    r"""Return every place one workflow puts the secret in reach.

    Three shapes, anywhere in the document: a key naming the variable
    (an ``env`` binding, a forwarded secret, a ``workflow_call`` secret
    declaration), an expression reading it, and ``secrets: inherit``.

    >>> from workflow_reading import load_workflow
    >>> body = "env:\n  CS_ACCESS_TOKEN: x\njobs:\n  a:\n    secrets: inherit\n"
    >>> token_sites("ci.yml", load_workflow(body))
    ['ci.yml: env.CS_ACCESS_TOKEN<key>', 'ci.yml: jobs.a.secrets']
    >>> token_sites("ci.yml", load_workflow("name: says CS_ACCESS_TOKEN\n"))
    []
    """
    return [
        f"{name}: {where}"
        for where, text in scalars(document)
        if (text.strip().casefold() == FORBIDDEN_VARIABLE.casefold() and _is_key(where))
        or (not _is_key(where) and (_expression_reads(text) or _inherits(where, text)))
    ]


def _values_naming(name: str, document: WorkflowDocument, needle: str) -> list[str]:
    """Return every key or value in one workflow containing a needle, case-folded."""
    return [
        f"{name}: {where}" for where, text in scalars(document) if _names(text, needle)
    ]


def codescene_contacts(name: str, document: WorkflowDocument) -> list[str]:
    r"""Return every place in one workflow that names the CodeScene host.

    Case-folded, because DNS names are case-insensitive and
    ``API.CODESCENE.IO`` reaches the same service.

    >>> from workflow_reading import load_workflow
    >>> body = "defaults:\n  run:\n    shell: curl https://API.CodeScene.io; bash {0}\n"
    >>> codescene_contacts("ci.yml", load_workflow(body))
    ['ci.yml: defaults.run.shell']
    >>> codescene_contacts("ci.yml", load_workflow("# see codescene.io\nname: x\n"))
    []
    """
    return _values_naming(name, document, CODESCENE_HOST)


def action_sites(name: str, document: WorkflowDocument) -> list[str]:
    r"""Return every place in one workflow naming the upload action.

    >>> from workflow_reading import load_workflow
    >>> body = "jobs:\n  a:\n    steps:\n      - uses: x/Upload-CodeScene-Coverage@v1\n"
    >>> action_sites("ci.yml", load_workflow(body))
    ['ci.yml: jobs.a.steps[0].uses']
    """
    return _values_naming(name, document, ACTION_NAME)


def cli_sites(name: str, document: WorkflowDocument) -> list[str]:
    r"""Return every place in one workflow naming the CodeScene CLI.

    A ``run:`` step invoking ``cs-coverage`` directly is the same gate in
    different clothes, and so is a shell or an input that runs it.

    >>> from workflow_reading import load_workflow
    >>> body = "jobs:\n  a:\n    steps:\n      - run: cs-coverage check\n"
    >>> cli_sites("ci.yml", load_workflow(body))
    ['ci.yml: jobs.a.steps[0].run']
    """
    return _values_naming(name, document, CLI_COMMAND)
