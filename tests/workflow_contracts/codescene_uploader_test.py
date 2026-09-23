"""The CodeScene uploader runs at the approved pin, with no checksum input.

At shared-actions `a5765019` the `upload-codescene-coverage` action treats its
committed `cli-manifest.json` as the trust anchor for the cs-coverage archive
and rejects a non-empty `installer-checksum` outright. The repository
variable that fed that input, `CODESCENE_CLI_SHA256`, could only ever repeat
the manifest's digest, and the dispatch workflow that refreshed it,
`get-codescene-sha.yml`, has nothing left to do.

Every clause reads parsed documents through `read_workflows`, so a comment
naming the action or the input is not read as a use of it, and a directory
that cannot be listed or a file that cannot be parsed fails loudly as a
`WorkflowReadingError` rather than as an empty answer.

Run via ``make test-workflow-contracts``.
"""

from __future__ import annotations

import pathlib
import re
import typing as typ

import pytest
from codescene_coverage import CODESCENE_ACTION, invokes
from workflow_reading import read_workflows, scalars, workflow_steps

if typ.TYPE_CHECKING:  # pragma: no cover - typing only
    from workflow_reading import WorkflowDocument

REPOSITORY_ROOT: typ.Final[pathlib.Path] = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS: typ.Final[pathlib.Path] = REPOSITORY_ROOT / ".github" / "workflows"

#: The uploader pin approved for the committed-manifest trust model. Named
#: rather than shape-checked, because the pin is the decision: an earlier
#: commit accepts the deprecated input and resolves the CLI from `latest`.
APPROVED_UPLOADER_PIN: typ.Final[str] = "a5765019912a8ab6882b12db049c7cde635f3a85"

#: The input the uploader rejects at the approved pin.
DEPRECATED_INPUT: typ.Final[str] = "installer-checksum"

#: The repository variable whose only consumer was that input.
RETIRED_VARIABLE: typ.Final[str] = "CODESCENE_CLI_SHA256"

#: The dispatch workflow that refreshed the variable.
REFRESH_WORKFLOW: typ.Final[str] = "get-codescene-sha.yml"

#: An expression reading the retired variable under any context.
_READS_RETIRED: typ.Final[re.Pattern[str]] = re.compile(
    rf"\$\{{\{{[^}}]*\b(?:vars|env|secrets)\.{RETIRED_VARIABLE}\b", re.IGNORECASE
)


@pytest.fixture(scope="module")
def documents() -> dict[str, WorkflowDocument]:
    """Return this repository's workflows, parsed once."""
    return read_workflows(WORKFLOWS)


def _uploader_steps(
    documents: dict[str, WorkflowDocument],
) -> list[tuple[str, dict[str, object]]]:
    """Return every active step invoking the uploader, with its workflow."""
    return [
        (name, step)
        for name, document in documents.items()
        for step in workflow_steps(document)
        if invokes(step, CODESCENE_ACTION)
    ]


def test_every_uploader_reference_is_pinned_to_the_approved_sha(
    documents: dict[str, WorkflowDocument],
) -> None:
    """An uploader exists, and each one runs at the approved commit.

    The collection is asserted non-empty first: a contract over no uploaders
    is satisfied by deleting the upload, and a commented-out `uses:` line is
    not a step, so it cannot stand in for one.
    """
    steps = _uploader_steps(documents)
    assert steps, f"no workflow step invokes {CODESCENE_ACTION}"
    pins = sorted({(name, str(step["uses"]).partition("@")[2]) for name, step in steps})
    assert all(pin == APPROVED_UPLOADER_PIN for _, pin in pins), (
        f"every uploader must run at {APPROVED_UPLOADER_PIN}; found {pins}"
    )


def test_no_workflow_passes_the_deprecated_installer_checksum(
    documents: dict[str, WorkflowDocument],
) -> None:
    """The input fails the upload at the approved pin, whatever its value.

    Read as a key anywhere in the parsed document, so an empty value, a
    reusable-workflow `with:` or a comment-free spelling cannot slip past.
    """
    offenders = sorted(
        f"{name}: {where}"
        for name, document in documents.items()
        for where, text in scalars(document)
        if where.endswith("<key>") and text == DEPRECATED_INPUT
    )
    assert not offenders, f"these pass {DEPRECATED_INPUT}: {offenders}"


def test_no_workflow_reads_the_retired_variable(
    documents: dict[str, WorkflowDocument],
) -> None:
    """The variable has no consumer left; an expression reading it is one."""
    offenders = sorted(
        f"{name}: {where}"
        for name, document in documents.items()
        for where, text in scalars(document)
        if _READS_RETIRED.search(text)
    )
    assert not offenders, f"these read {RETIRED_VARIABLE}: {offenders}"


def test_the_checksum_refresh_workflow_is_absent(
    documents: dict[str, WorkflowDocument],
) -> None:
    """The refresh workflow has nothing left to refresh.

    Judged from the directory listing `read_workflows` made, which raises on
    a directory it cannot list, so an unreadable directory is never mistaken
    for an absent file. File names are compared case-folded, as GitHub runs
    either spelling.
    """
    present = {name.casefold() for name in documents}
    assert REFRESH_WORKFLOW not in present, f"{REFRESH_WORKFLOW} must not return"
