"""The workflow readers, driven on documents this repository does not have.

Every CV-005 rule derives its subject from these readings, and each rule
is a refusal. A refusal over an empty subject set is satisfied by any
repository at all, so a reader that quietly finds nothing reports
compliance rather than an error. The real workflows use one spelling of
``on:``, one push filter and one job shape, so they agree with a broken
reader as readily as with a working one; every case here is constructed
for that reason.

Run via ``make test-workflow-contracts``.
"""

from __future__ import annotations

import typing as typ

import pytest
import yaml
from codescene_coverage import publishers, pull_request_workflows
from workflow_reading import (
    WorkflowReadingError,
    load_workflow,
    pushes_to_main,
    read_workflows,
    triggers,
    workflow_steps,
)

if typ.TYPE_CHECKING:  # pragma: no cover - typing only
    import pathlib

#: A minimal workflow body, so each case below varies one thing.
JOBS: typ.Final[str] = "jobs:\n  a:\n    steps: []\n"


@pytest.mark.parametrize(
    ("document", "expected"),
    [
        pytest.param("on:\n  pull_request:\n", {"pull_request"}, id="mapping"),
        pytest.param("'on':\n  pull_request:\n", {"pull_request"}, id="quoted-key"),
        pytest.param("on: [pull_request, push]", {"pull_request", "push"}, id="list"),
        pytest.param("on:\n  - pull_request\n", {"pull_request"}, id="block-list"),
        pytest.param("on: pull_request", {"pull_request"}, id="bare-string"),
        pytest.param("name: x\n", set(), id="no-triggers"),
    ],
)
def test_the_trigger_reader_handles_every_spelling(
    document: str, expected: set[str]
) -> None:
    """A list form read as a mapping stringifies into one key.

    ``on: [push, pull_request]`` read by a mapping-only reader becomes
    one trigger named ``"['push', 'pull_request']"``, and the workflow
    escapes every pull-request prohibition.
    """
    assert triggers(load_workflow(document)) == expected, document


def test_the_trigger_reader_survives_a_resolving_loader() -> None:
    """The ``on`` key can arrive as the boolean ``True``.

    YAML 1.1 resolves an unquoted ``on:`` to a boolean, so a loader that
    resolves scalars keys every workflow under ``True``. Written against
    ``yaml.safe_load`` because the hazard is a property of the loader,
    and swapping ``load_workflow`` to a resolving one is a one-word
    change that would otherwise empty every rule.
    """
    document = yaml.safe_load("on:\n  pull_request:\n  push:\n")
    assert True in document, "the premise: a resolving loader keys `on` as True"
    assert triggers(document) == {"pull_request", "push"}


@pytest.mark.parametrize(
    ("filters", "expected"),
    [
        pytest.param("", True, id="bare"),
        pytest.param("\n    branches: [main]", True, id="branches-main"),
        pytest.param("\n    branches: main", True, id="branches-scalar"),
        pytest.param("\n    branches: [release]", False, id="branches-other"),
        pytest.param("\n    branches: ['**']", True, id="branches-double-star"),
        pytest.param("\n    branches: ['ma*']", True, id="branches-glob"),
        pytest.param("\n    branches: ['**', '!main']", False, id="negated-last"),
        pytest.param("\n    branches: ['!main', 'main']", True, id="negation-undone"),
        pytest.param("\n    branches-ignore: [main]", False, id="ignore-main"),
        pytest.param("\n    branches-ignore: ['m*']", False, id="ignore-glob"),
        pytest.param("\n    branches-ignore: [wip]", True, id="ignore-other"),
        pytest.param("\n    tags: ['v*']", False, id="tags"),
        pytest.param("\n    tags-ignore: ['v*']", False, id="tags-ignore"),
        pytest.param("\n    paths: ['src/**']", True, id="paths"),
    ],
)
def test_the_push_reader_answers_every_filter_form(
    filters: str, *, expected: bool
) -> None:
    """Which workflow may publish turns on this reading.

    Filters are globs with ``!`` negation, the last match winning, so a
    literal-list reading lets ``'**'`` escape a rule that only main may
    write the baseline. A tag filter alone never fires for a branch.
    """
    document = load_workflow(f"on:\n  push:{filters}\n{JOBS}")
    assert pushes_to_main(document) is expected, f"push{filters!r}"


def test_a_workflow_serving_pull_requests_is_not_a_publisher() -> None:
    """Both halves of the publisher predicate, the second being the one dropped."""
    both = load_workflow(f"on: [pull_request, push]\n{JOBS}")
    assert pushes_to_main(both), "the premise: the fixture pushes to main"
    assert publishers({"ci.yml": both}) == {}


@pytest.mark.parametrize(
    ("body", "key"),
    [
        pytest.param(
            "jobs:\n  a:\n    runs-on: ubuntu-latest\n    runs-on: windows-latest\n",
            "runs-on",
            id="job-key",
        ),
        pytest.param("on:\n  push:\non:\n  pull_request:\n", "on", id="top-level"),
    ],
)
def test_a_duplicated_key_is_refused_rather_than_resolved(body: str, key: str) -> None:
    """PyYAML keeps the last of two equal keys and says nothing.

    A lane could carry a paid label in the discarded half of a doubled
    ``runs-on`` and read as hosted. Refusing is the one reading that
    cannot be wrong about which half GitHub runs.
    """
    with pytest.raises(yaml.YAMLError, match=f"duplicate key '{key}'"):
        load_workflow(body)


def test_distinct_keys_at_different_levels_are_not_duplicates() -> None:
    """The refusal is per mapping: every workflow repeats keys across jobs."""
    jobs = load_workflow("jobs:\n  a:\n    runs-on: x\n  b:\n    runs-on: y\n")["jobs"]
    assert isinstance(jobs, dict), jobs
    assert sorted(jobs) == ["a", "b"]


def test_a_workflow_with_an_upper_case_suffix_is_read(tmp_path: pathlib.Path) -> None:
    """GitHub runs ``CI.YML``, so a case-sensitive glob would skip a lane."""
    (tmp_path / "CI.YML").write_text(f"on: pull_request\n{JOBS}", encoding="utf-8")
    (tmp_path / "notes.txt").write_text("on: pull_request\n", encoding="utf-8")
    assert sorted(read_workflows(tmp_path)) == ["CI.YML"]


def test_an_empty_workflow_directory_is_a_reader_fault(tmp_path: pathlib.Path) -> None:
    """Finding no workflow at all is never an answer."""
    with pytest.raises(WorkflowReadingError) as raised:
        read_workflows(tmp_path)
    assert raised.value.reader == "read_workflows"
    assert raised.value.path == str(tmp_path)


@pytest.mark.parametrize(
    "body",
    [
        pytest.param("- not a mapping\n", id="not-a-mapping"),
        pytest.param("on: [\n", id="unclosed-flow-sequence"),
        pytest.param("on:\n  push:\n bad: indentation\n", id="bad-indentation"),
        pytest.param("a: 1\na: 2\n", id="duplicate-key"),
    ],
)
def test_a_file_that_is_not_a_workflow_names_its_file(
    tmp_path: pathlib.Path, body: str
) -> None:
    """A parser failure is reported at the boundary, with the file.

    ``yaml.YAMLError`` is neither a ``TypeError`` nor a ``ValueError``,
    so a boundary catching only those lets invalid YAML escape naming no
    file. ``- not a mapping`` parses, so it exercises only the shape
    branch; the others exercise the parser branch.
    """
    (tmp_path / "broken.yml").write_text(body, encoding="utf-8")
    with pytest.raises(WorkflowReadingError) as raised:
        read_workflows(tmp_path)
    assert "broken.yml" in (raised.value.path or ""), raised.value


def test_no_pull_request_workflow_is_a_reader_fault() -> None:
    """A repository whose pull-request lane reads as empty has a broken reader."""
    with pytest.raises(WorkflowReadingError) as raised:
        pull_request_workflows({"main.yml": load_workflow(f"on:\n  push:\n{JOBS}")})
    assert raised.value.reader == "pull_request_workflows"


@pytest.mark.parametrize(
    "body",
    [
        pytest.param("jobs: not-a-mapping\n", id="jobs-not-a-mapping"),
        pytest.param("jobs:\n  a: not-a-mapping\n", id="job-not-a-mapping"),
        pytest.param("jobs:\n  a:\n    steps: not-a-list\n", id="steps-not-a-list"),
        pytest.param("jobs:\n  a:\n    steps:\n      - a-string\n", id="step-a-string"),
    ],
)
def test_a_malformed_job_shape_yields_no_steps(body: str) -> None:
    """A fragment this reading cannot use contributes nothing, and raises nothing."""
    assert workflow_steps(load_workflow(f"on:\n  push:\n{body}")) == []
