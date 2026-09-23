"""Which workflows a CV-005 rule applies to, as pure selections.

The parsing and the trigger grammar are in ``workflow_reading``, which
knows nothing about CodeScene. What is here is the choice of subjects:
the pull-request lane as a closure through reusable-workflow calls, the
one workflow allowed to publish, and the coverage steps whose inputs
must agree. What a selected workflow must not do is read in
``codescene_reach``.
"""

from __future__ import annotations

import re
import typing as typ

from workflow_reading import (
    WorkflowReadingError,
    pushes_to_main,
    serves_pull_requests,
    workflow_jobs,
    workflow_steps,
)

if typ.TYPE_CHECKING:  # pragma: no cover - typing only
    from workflow_reading import WorkflowDocument

#: The action that talks to CodeScene, matched on its path rather than
#: the word: the workflows discuss CodeScene in prose, and a comment is
#: not an invocation. A repin changes the SHA after the ``@``, so the
#: path is what stays true.
CODESCENE_ACTION: typ.Final[str] = (
    "leynos/shared-actions/.github/actions/upload-codescene-coverage"
)

#: The coverage generator, which every lane may run.
COVERAGE_ACTION: typ.Final[str] = (
    "leynos/shared-actions/.github/actions/generate-coverage"
)

#: A full-length commit pin. The value is deliberately not named:
#: Dependabot owns these bumps, and a test holding today's SHA turns
#: every routine bump into a manual edit.
PINNED_COMMIT: typ.Final[re.Pattern[str]] = re.compile(r"^[0-9a-f]{40}$")

#: The command no pull-request lane may run.
CLI_COMMAND: typ.Final[str] = "cs-coverage"

#: The secret no pull-request lane may put in reach of a process.
FORBIDDEN_VARIABLE: typ.Final[str] = "CS_ACCESS_TOKEN"

#: This repository, as a qualified ``uses:`` reference would name it.
SELF_REPOSITORY: typ.Final[str] = "leynos/podbot"

#: Where a same-repository reusable workflow lives. GitHub calls nothing
#: outside this directory, and nothing in a subdirectory of it.
WORKFLOW_DIRECTORY: typ.Final[str] = ".github/workflows/"

#: The two local spellings GitHub documents: ``./`` and ``$/``, the
#: second being the recommended one. Neither may carry an ``@ref``.
LOCAL_PREFIXES: typ.Final[tuple[str, ...]] = ("./", "$/")


def _strip_local_prefix(reference: str) -> str:
    """Return a reference without its leading ``./`` or ``$/``."""
    for prefix in LOCAL_PREFIXES:
        if reference.startswith(prefix):
            return reference.removeprefix(prefix)
    return reference


def is_refused_call(reference: object) -> bool:
    """Return whether a ``uses:`` value names this repository unreadably.

    Two spellings name a workflow here while running a version this tree
    does not hold: a local prefix with an ``@ref``, and a qualified
    self-call such as ``leynos/podbot/.github/workflows/x.yml@main``,
    which runs the file as it stands at that ref. A closure cannot read
    either, so following the local file would prove nothing about what
    runs; they are refused rather than followed.

    >>> is_refused_call("leynos/podbot/.github/workflows/x.yml@main")
    True
    >>> is_refused_call("$/.github/workflows/x.yml@main")
    True
    >>> is_refused_call("$/.github/workflows/x.yml")
    False
    >>> is_refused_call("other/repo/.github/workflows/x.yml@main")
    False
    """
    if not isinstance(reference, str):
        return False
    text = reference.strip()
    qualified = f"{SELF_REPOSITORY}/{WORKFLOW_DIRECTORY}"
    if text.casefold().startswith(qualified.casefold()):
        return True
    return text.startswith(LOCAL_PREFIXES) and "@" in text


def local_workflow(
    reference: object, documents: dict[str, WorkflowDocument]
) -> str | None:
    """Return the file a ``uses:`` value calls in this tree, if any.

    Matched by shape rather than by an enumerated prefix list: strip a
    leading ``./`` or ``$/`` and ask whether what remains is a file
    directly under the workflow directory, with no ``@ref``. A list of
    accepted spellings drops every spelling nobody thought to list,
    silently, while a pull request still runs the workflow it names.

    >>> local_workflow("$/.github/workflows/probe.yml", {"probe.yml": {}})
    'probe.yml'
    >>> local_workflow("./.github/workflows/nested/probe.yml", {"probe.yml": {}})
    """
    if not isinstance(reference, str) or "@" in reference:
        return None
    path = _strip_local_prefix(reference.strip())
    if not path.startswith(WORKFLOW_DIRECTORY):
        return None
    name = path.removeprefix(WORKFLOW_DIRECTORY)
    return name if name in documents and "/" not in name else None


def called_workflows(
    document: WorkflowDocument, documents: dict[str, WorkflowDocument]
) -> frozenset[str]:
    r"""Return the same-repository reusable workflows one document calls.

    A reference to another repository is not followed: its content is
    not in this tree, and claiming to have checked it would be worse
    than saying plainly that it is out of scope.

    >>> from workflow_reading import load_workflow
    >>> caller = load_workflow("jobs:\n  c:\n    uses: ./.github/workflows/p.yml\n")
    >>> sorted(called_workflows(caller, {"p.yml": {}}))
    ['p.yml']
    """
    names = (
        local_workflow(job.get("uses"), documents)
        for job in workflow_jobs(document).values()
    )
    return frozenset(name for name in names if name is not None)


def _reachable(
    seeds: list[str], documents: dict[str, WorkflowDocument]
) -> dict[str, WorkflowDocument]:
    """Return the seeds and every workflow they call, transitively."""
    found: dict[str, WorkflowDocument] = {}
    pending = list(seeds)
    while pending:
        name = pending.pop()
        if name not in found:
            found[name] = documents[name]
            pending.extend(called_workflows(documents[name], documents) - found.keys())
    return found


def pull_request_workflows(
    documents: dict[str, WorkflowDocument],
) -> dict[str, WorkflowDocument]:
    r"""Return every workflow a pull request can reach.

    A closure rather than a trigger list. A workflow declaring only
    ``workflow_call`` still runs on a pull request when a pull-request
    workflow calls it, and ``secrets: inherit`` hands it the token, so a
    reading enumerating triggers alone could not see it. Measured on
    episodic: such a workflow curling the CodeScene API with an
    inherited token passed every clause of a trigger-list contract.

    Raises ``WorkflowReadingError`` when no workflow serves a pull
    request, which cannot be true of a repository with a pull-request
    lane.

    >>> from workflow_reading import load_workflow
    >>> documents = {
    ...     "ci.yml": load_workflow("on:\n  pull_request:\n"),
    ...     "main.yml": load_workflow("on:\n  push:\n"),
    ... }
    >>> sorted(pull_request_workflows(documents))
    ['ci.yml']
    """
    found = _reachable(
        [
            name
            for name, document in documents.items()
            if serves_pull_requests(document)
        ],
        documents,
    )
    if not found:
        message = (
            "this reading found no workflow serving a pull request; the "
            "trigger reader is broken, not the workflows"
        )
        raise WorkflowReadingError(message, reader="pull_request_workflows")
    return found


def publishers(
    documents: dict[str, WorkflowDocument],
) -> dict[str, WorkflowDocument]:
    r"""Return the workflows allowed to upload coverage.

    A publisher pushes to main *and serves no pull request*. Both halves
    are needed: a workflow declaring ``pull_request`` and ``push``
    together would otherwise be required to upload and forbidden from
    uploading at once. Empty is a legitimate answer rather than a fault,
    because "no publisher" is one of the states the contract refuses.

    >>> from workflow_reading import load_workflow
    >>> main = load_workflow("on:\n  push:\n    branches: [main]\n")
    >>> sorted(publishers({"main.yml": main}))
    ['main.yml']
    >>> publishers({"ci.yml": load_workflow("on: [pull_request, push]\n")})
    {}
    """
    return {
        name: document
        for name, document in documents.items()
        if pushes_to_main(document) and not serves_pull_requests(document)
    }


def invokes(step: dict[str, object], action: str) -> bool:
    """Return whether a step calls one action, by path, at any ref.

    >>> invokes({"uses": f"{COVERAGE_ACTION}@" + "a" * 40}, COVERAGE_ACTION)
    True
    >>> invokes({"run": COVERAGE_ACTION}, COVERAGE_ACTION)
    False
    """
    return str(step.get("uses", "")).startswith(f"{action}@")


def coverage_steps(
    documents: dict[str, WorkflowDocument],
) -> dict[str, list[dict[str, object]]]:
    r"""Return each workflow's generate-coverage steps, keyed by file name.

    >>> from workflow_reading import load_workflow
    >>> body = "jobs:\n  a:\n    steps:\n      - uses: " + COVERAGE_ACTION + "@x\n"
    >>> {name: len(steps) for name, steps in
    ...  coverage_steps({"ci.yml": load_workflow(body)}).items()}
    {'ci.yml': 1}
    """
    found: dict[str, list[dict[str, object]]] = {}
    for name, document in documents.items():
        steps = [
            step for step in workflow_steps(document) if invokes(step, COVERAGE_ACTION)
        ]
        if steps:
            found[name] = steps
    return found
