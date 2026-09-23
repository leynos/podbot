"""What the workflow files must say, and why each rule exists.

Three rules, each written from a specific failure rather than from a
general wish for tidiness.

The **hermetic pin** rule exists because every `leynos/shared-actions`
reference here sat, for two months, on a commit whose `setup-rust`
installs sccache, starts it, and exports no `RUSTC_WRAPPER`. Cargo
routed no compilation through it, so the cache was downloaded on every
Rust job and cached nothing, and the only visible symptom was a slow
lane. Dependabot's open group bump (#164) proposes moving those
references three weeks forward to a commit that still predates the
export, which reads exactly like pins being brought up to date. That is
why the class is refused by name rather than by review: recency is not
the property that matters.

The **watchdog** rule exists because the budget was inherited. An
inherited default is a value this repository never states and cannot
notice changing.

The **placement** rule exists because a folded scalar whose continuation
is indented more deeply than its first line keeps the line break, and the
resulting `runs-on` carries a newline inside the expression. GitHub
evaluates it anyway, so a green run is not evidence the defect is absent
and nothing but a contract will find it.

The last of those cannot be proved by this repository's own files: every
job here names a literal runner, so a check parametrized over them passes
whether or not it discriminates anything. The mechanism is driven
directly with constructed documents, in both directions, and the real
files are asserted separately.
"""

from __future__ import annotations

import re
from pathlib import Path

import pytest
from workflow_contracts import (
    WRAPPER_EXPORT_COMMIT,
    WRAPPER_EXPORTING_PINS,
    WorkflowReadError,
    load_workflow_documents,
    shared_actions_references,
)
from workflow_contracts import parse as parse_workflow
from workflow_coverage import COVERAGE_ACTION

#: A 40-hex commit, which is the only form a reference may take. A tag or
#: a branch is mutable, and a short SHA is ambiguous.
COMMIT_SHA = re.compile(r"\A[0-9a-f]{40}\Z")


def test_every_shared_actions_reference_is_a_commit(
    workflow_texts: dict[str, str],
) -> None:
    """A mutable ref is not a pin, and a short one is not unique."""
    references = shared_actions_references(workflow_texts)

    assert references, (
        "no shared-actions reference was found at all; the reader matches by "
        "repository prefix, so an empty result means the reader is broken "
        "rather than that the workflows stopped using the repository"
    )
    for reference in references:
        assert COMMIT_SHA.match(reference.ref), (
            f"{reference.workflow} pins {reference.path} at {reference.ref!r}, "
            f"which is not a 40-hex commit"
        )


def test_every_shared_actions_reference_moves_together(
    workflow_texts: dict[str, str],
) -> None:
    """One SHA across the repository, so a partial repin cannot land.

    The references were on two different commits before this contract
    existed, two months apart, and nothing said so. Actions from one
    repository are developed together and the interfaces between them
    move together; a pull request repinning some of them leaves a
    combination nobody has run.
    """
    references = shared_actions_references(workflow_texts)
    refs = {reference.ref for reference in references}

    assert len(refs) == 1, (
        "every leynos/shared-actions reference must name one commit; these "
        "do not: "
        + ", ".join(
            f"{reference.workflow}:{reference.path.rsplit('/', 1)[-1]}="
            f"{reference.ref[:8]}"
            for reference in references
        )
    )


def test_every_reference_names_a_pin_known_to_export_the_wrapper(
    workflow_texts: dict[str, str],
) -> None:
    """An allowlist, because a blacklist of bad pins cannot be complete.

    The first version of this contract named the pins known to lack the
    wrapper export. A reviewer pointed out the hole: any commit outside
    that set passes every other rule here while restoring exactly the
    no-cache state this file exists to prevent, and Dependabot chooses
    from the whole history rather than from a list. A newer pin short of
    the export reads as pins brought up to date, which is how this
    repository got into the state the repin fixed.

    Refusing an unknown pin fails closed and costs a verification. That
    verification is the work the contract is asking for: check the pin
    descends from the export commit on shared-actions' default branch,
    then add it here with that evidence in the comment.
    """
    for reference in shared_actions_references(workflow_texts):
        assert reference.ref in WRAPPER_EXPORTING_PINS, (
            f"{reference.workflow} pins {reference.path} at "
            f"{reference.ref[:8]}, which is not on the list of pins verified "
            f"to export RUSTC_WRAPPER. If sccache is installed and the "
            f"wrapper is not exported, it is started on every Rust job and "
            f"used by nothing, and the only symptom is a slow lane. The "
            f"export landed on shared-actions at {WRAPPER_EXPORT_COMMIT} on "
            f"2026-09-04: confirm this pin descends from it on the default "
            f"branch, then add it to WRAPPER_EXPORTING_PINS with that "
            f"evidence rather than widening the rule"
        )


@pytest.mark.parametrize(
    ("ref", "accepted"),
    [
        ("0e3c4d24e43aa48b511d94f3b902711eb02138df", True),
        ("v2", False),
        ("main", False),
        ("0e3c4d2", False),
        ("1111111111111111111111111111111111111111", False),
    ],
    ids=["known-pin", "tag", "branch", "short-sha", "unknown-full-sha"],
)
def test_a_pin_outside_the_allowlist_is_rejected(ref: str, accepted: bool) -> None:
    """The allowlist, driven over constructed references.

    Every reference in this repository is already a known pin, so the
    contract over the real files passes whether the rule is an allowlist, a
    blacklist, or a bare length check. The unknown full SHA is the case that
    separates them: it satisfies the commit-shape rule and a blacklist, and
    only an allowlist refuses it.
    """
    text = f"jobs:\n  test:\n    steps:\n      - uses: {COVERAGE_ACTION}@{ref}\n"
    references = shared_actions_references({"ci.yml": text})

    assert len(references) == 1
    assert (references[0].ref in WRAPPER_EXPORTING_PINS) is accepted


def test_a_file_that_is_not_utf8_is_refused_by_name(tmp_path: Path) -> None:
    """The refusal names the file, because the directory holds several."""
    (tmp_path / "ci.yml").write_bytes(b"jobs:\n  lint:\n    runs-on: \xff\n")

    with pytest.raises(WorkflowReadError, match=re.escape("ci.yml could not be read")):
        load_workflow_documents(tmp_path)


def test_yaml_that_does_not_parse_is_refused() -> None:
    """A parse failure is this reader's own error, not PyYAML's.

    The distinction matters to a caller: `WorkflowReadError` is the one
    exception these contracts expect, and a `yaml.YAMLError` escaping from
    something that reads like a query is an unhandled crash.
    """
    with pytest.raises(
        WorkflowReadError, match=re.escape("ci.yml could not be parsed")
    ):
        parse_workflow("ci.yml", "jobs: [unclosed\n")


@pytest.mark.parametrize(
    "text",
    ["- lint\n- test\n", "just a string\n", "\n"],
    ids=["list", "scalar", "empty"],
)
def test_a_document_that_is_not_a_mapping_is_refused(text: str) -> None:
    """Each of these parses cleanly and is still not a workflow.

    The empty case is the one worth naming: an empty file yields `None`,
    which would otherwise reach every `of_type` walk below as a document.
    """
    with pytest.raises(WorkflowReadError, match=re.escape("ci.yml is not a mapping")):
        parse_workflow("ci.yml", text)


def test_a_duplicated_key_is_refused_rather_than_resolved() -> None:
    """PyYAML keeps the last of two equal keys and says nothing.

    A job declaring `runs-on` twice would parse into a document holding
    only the second label, so the placement rule would read a half GitHub
    may not run. Refusing is the one reading that cannot be wrong.
    """
    text = "jobs:\n  a:\n    runs-on: ubuntu-latest\n    runs-on: windows-latest\n"

    with pytest.raises(WorkflowReadError, match="duplicate key 'runs-on'"):
        parse_workflow("ci.yml", text)


def test_the_same_key_in_two_jobs_is_not_a_duplicate() -> None:
    """Narrow as well as sufficient: the refusal is per mapping."""
    text = "jobs:\n  a:\n    runs-on: x\n  b:\n    runs-on: y\n"

    assert sorted(parse_workflow("ci.yml", text)["jobs"]) == ["a", "b"]


def test_a_workflow_with_an_upper_case_suffix_is_read(tmp_path: Path) -> None:
    """GitHub runs `CI.YML`, so a case-sensitive glob would skip a lane."""
    (tmp_path / "CI.YML").write_text("jobs: {}\n", encoding="utf-8")
    (tmp_path / "notes.txt").write_text("jobs: {}\n", encoding="utf-8")

    assert sorted(load_workflow_documents(tmp_path)) == ["CI.YML"]
