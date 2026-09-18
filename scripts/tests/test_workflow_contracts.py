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
import sys
import typing as typ
from pathlib import Path

import pytest

# The readers live in `scripts/`, which is not a package and is not on
# `sys.path` when pytest collects this file from the repository root.
# The bootstrap therefore has to run before the imports below, which is
# what E402 forbids and why each of them carries the suppression: the
# import order is not a preference here, it is the only order that
# resolves.
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from workflow_contracts import (  # noqa: E402 - must follow the sys.path bootstrap above
    CACHE_REPORT_COMMAND,
    COVERAGE_ACTION,
    WATCHDOG_VARIABLE,
    WRAPPER_EXPORT_COMMIT,
    WRAPPER_EXPORTING_PINS,
    WorkflowReadError,
    cache_reports,
    command_steps,
    load_workflow_documents,
    shared_actions_references,
)
from workflow_contracts import parse as parse_workflow  # noqa: E402 - must follow the sys.path bootstrap above
from workflow_contracts import coverage_jobs as coverage_jobs_in  # noqa: E402 - must follow the sys.path bootstrap above
from workflow_placement import (  # noqa: E402 - must follow the sys.path bootstrap above
    line_break_fault,
    runs_on_declarations,
)

#: A 40-hex commit, which is the only form a reference may take. A tag or
#: a branch is mutable, and a short SHA is ambiguous.
COMMIT_SHA = re.compile(r"\A[0-9a-f]{40}\Z")

#: The command CI must run to execute the contracts in this directory.
CONTRACT_COMMAND: typ.Final[str] = "make workflow-contracts"

#: The budget this repository runs its coverage steps under, in seconds.
#: It is the action's current default, declared here so that a later
#: change to that default cannot move it silently. The worst coverage
#: step observed over six green runs is 487 s, so this is about 3.7
#: times the worst seen, and both coverage jobs pass
#: `use-cargo-nextest: 'false'`, which leaves the watchdog as the only
#: timer bounding cargo.
REQUIRED_WATCHDOG: typ.Final[str] = "1800"


@pytest.fixture(name="workflow_texts")
def fixture_workflow_texts() -> dict[str, str]:
    """Return every workflow file's text.

    Returns
    -------
    dict[str, str]
        File name to file text.
    """
    return load_workflow_documents()


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


def test_every_coverage_job_states_its_watchdog(
    workflow_texts: dict[str, str],
) -> None:
    """An inherited watchdog is a budget this repository never stated.

    The value is pinned as well as required, because the action's
    default is what it replaces: leaving the assertion at "some value is
    set" would let the budget drift from the one measured here without
    failing anything.
    """
    jobs = coverage_jobs_in(workflow_texts)

    assert jobs, (
        f"no job invoking {COVERAGE_ACTION} was found; the reader matches the "
        f"action path, so an empty result means the reader is broken"
    )
    for workflow, job, watchdog in jobs:
        assert watchdog is not None, (
            f"{workflow}:{job} runs the coverage action without declaring "
            f"{WATCHDOG_VARIABLE}, so it inherits the action's default"
        )
        assert str(watchdog) == REQUIRED_WATCHDOG, (
            f"{workflow}:{job} sets {WATCHDOG_VARIABLE} to {watchdog!r}, not "
            f"the {REQUIRED_WATCHDOG}s this repository measured and states"
        )


def test_a_longer_action_path_is_not_the_coverage_action() -> None:
    """The reader matches the path exactly, not as a substring.

    A substring also selects an action whose path extends this one, such as
    a `generate-coverage-disabled`. The discovery assertion next door only
    checks the result is non-empty, so a false-positive match would satisfy
    it and the watchdog assertion would then be about the wrong step.

    Driven over a constructed workflow because this repository contains no
    such action: parametrized over the real files the rule passes whether it
    compares paths or searches for text.
    """
    text = (
        "jobs:\n  test:\n    env:\n"
        f"      {WATCHDOG_VARIABLE}: '1800'\n"
        "    steps:\n"
        f"      - uses: {COVERAGE_ACTION}-disabled@abc\n"
        f"      - uses: {COVERAGE_ACTION}-v2@abc\n"
    )

    assert coverage_jobs_in({"ci.yml": text}) == ()


def test_the_coverage_action_itself_is_found_at_any_ref() -> None:
    """Narrow as well as sufficient: the real path must still match.

    A rule that refused everything would pass the test above and break the
    contract it serves, so the accepting case is asserted beside it.
    """
    text = (
        "jobs:\n  test:\n    env:\n"
        f"      {WATCHDOG_VARIABLE}: '1800'\n"
        "    steps:\n"
        f"      - uses: {COVERAGE_ACTION}@0123456789abcdef0123456789abcdef01234567\n"
    )

    assert coverage_jobs_in({"ci.yml": text}) == (("ci.yml", "test", "1800"),)


def test_the_contracts_are_run_by_ci(workflow_texts: dict[str, str]) -> None:
    """A contract nothing runs is a comment.

    The assertion is on the command rather than on a step named
    "Workflow contracts": a step can be renamed, and a step whose `run:`
    was changed to something else would keep the name and stop asserting
    anything. Equality on the stripped value, not a substring search, since
    a search is satisfied by `echo 'make workflow-contracts'`.

    Both guards are checked. A step with no `if:` inside a job with
    `if: false` is dead code, and a contract reading only the step's own
    attributes stays green while the command never runs.
    """
    document = parse_workflow("ci.yml", workflow_texts["ci.yml"])
    running = command_steps(document, CONTRACT_COMMAND)

    assert len(running) == 1, (
        f"exactly one step in ci.yml must run exactly {CONTRACT_COMMAND!r}; "
        f"{len(running)} do. A step whose run: merely contains that text, "
        f"such as an echo or a comment, does not count"
    )
    assert running[0].can_run, (
        f"the step running {CONTRACT_COMMAND!r} in job {running[0].job!r} is "
        f"guarded by {running[0].describe_guards()}, so it can be skipped "
        "without failing anything"
    )


@pytest.mark.parametrize(
    ("job_guard", "step_guard", "can_run"),
    [
        ("", "", True),
        ("    if: false\n", "", False),
        ("", "        if: false\n", False),
        ("    if: ${{ github.event_name == 'push' }}\n", "", False),
        ("", "        if: ${{ false }}\n", False),
    ],
    ids=["unguarded", "job-guard", "step-guard", "job-expression", "step-expression"],
)
def test_a_guard_at_either_scope_stops_the_command_running(
    job_guard: str, step_guard: str, can_run: bool
) -> None:
    """Driven over constructed workflows, because the real one is unguarded.

    The contract above is parametrized over a file that has no `if:`
    anywhere, so it passes whether or not the rule reads the job. Only a
    constructed guarded job shows the job scope being read at all.
    """
    text = (
        "jobs:\n  lint:\n"
        f"{job_guard}"
        "    steps:\n"
        f"      - run: {CONTRACT_COMMAND}\n"
        f"{step_guard}"
    )
    found = command_steps(parse_workflow("ci.yml", text), CONTRACT_COMMAND)

    assert len(found) == 1
    assert found[0].job == "lint"
    assert found[0].can_run is can_run


def test_a_mentioned_command_is_not_a_running_one() -> None:
    """Equality, shown to reject the spellings a search accepts."""
    text = (
        "jobs:\n  lint:\n    steps:\n"
        f"      - run: echo '{CONTRACT_COMMAND}'\n"
        f"      - run: '# {CONTRACT_COMMAND}'\n"
        f"      - run: {CONTRACT_COMMAND} --dry-run\n"
    )
    assert command_steps(parse_workflow("ci.yml", text), CONTRACT_COMMAND) == ()


def test_no_runner_placement_carries_a_line_break(
    workflow_texts: dict[str, str],
) -> None:
    """The real files, asserted against the rule driven below."""
    declarations = runs_on_declarations(workflow_texts)

    assert declarations, "no job declares runs-on; the reader is broken"
    for declaration in declarations:
        fault = line_break_fault(declaration.value)
        assert fault is None, (
            f"{declaration.workflow}:{declaration.job} has a runs-on carrying "
            f"a line break, which GitHub evaluates as written. The value "
            f"parsed as {fault!r}, from:\n{declaration.raw}"
        )


def test_every_coverage_step_is_followed_by_a_cache_report(
    workflow_texts: dict[str, str],
) -> None:
    """The repin's only evidence is the counters, so the report must run.

    The repin this branch exists for is invisible in a green lane: on the
    pins used before, sccache was installed and started on every Rust job
    and Cargo routed nothing through it, and the only symptom was slowness.
    The report step is what turns that back into something a reader can
    check, so removing it, guarding it, or moving it before the compiling
    work would leave the change unevidenced while every other rule here
    still passed.
    """
    reports = cache_reports(workflow_texts)

    assert reports, (
        f"no step invoking {COVERAGE_ACTION} was found; the reader matches "
        f"the action path, so an empty result means the reader is broken"
    )
    for report in reports:
        assert report.follows_coverage, (
            f"{report.workflow}:{report.job} runs the coverage action at step "
            f"{report.coverage_index} with no {CACHE_REPORT_COMMAND!r} step "
            f"after it, so nothing reports whether anything reached the cache"
        )
        assert report.guard == "always()", (
            f"{report.workflow}:{report.job} guards its cache report with "
            f"{report.guard!r}. It must be always(): a red lane is exactly "
            f"when the counters are worth having, and a report that runs only "
            f"on success is absent whenever it would say most"
        )


@pytest.mark.parametrize(
    ("steps", "follows", "guard"),
    [
        (
            "      - uses: {action}@abc\n      - if: always()\n        run: {report}\n",
            True,
            "always()",
        ),
        ("      - uses: {action}@abc\n", False, None),
        (
            "      - if: always()\n        run: {report}\n      - uses: {action}@abc\n",
            False,
            None,
        ),
        (
            "      - uses: {action}@abc\n"
            "      - if: success()\n        run: {report}\n",
            True,
            "success()",
        ),
        ("      - uses: {action}@abc\n      - run: {report}\n", True, None),
        (
            "      - uses: {action}@abc\n"
            "      - if: always()\n        run: echo {report}\n",
            False,
            None,
        ),
    ],
    ids=["after", "absent", "before", "wrongly-guarded", "unguarded", "echoed"],
)
def test_the_cache_report_is_read_by_position_and_guard(
    steps: str, follows: bool, guard: object
) -> None:
    """Driven over constructed workflows, because the real files are correct.

    Parametrized over `ci.yml` and `coverage-main.yml` the rule passes
    whether or not it reads position, guard or spelling at all. Two cases
    carry the weight. A report *before* the coverage step is present,
    spelled correctly, and reports on nothing. An echoed report satisfies a
    substring search while running nothing, which is the same defect the
    contract-command rule next door exists to refuse.
    """
    text = "jobs:\n  test:\n    steps:\n" + steps.format(
        action=COVERAGE_ACTION, report=CACHE_REPORT_COMMAND
    )
    found = cache_reports({"ci.yml": text})

    assert len(found) == 1
    assert found[0].follows_coverage is follows
    assert found[0].guard == guard


def test_a_job_with_no_coverage_step_contributes_nothing() -> None:
    """Narrow as well as sufficient.

    A job that never runs the coverage action needs no cache report, and a
    rule demanding one everywhere would fail every other job in the file.
    """
    text = "jobs:\n  lint:\n    steps:\n      - run: make check-fmt\n"

    assert cache_reports({"ci.yml": text}) == ()


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


@pytest.mark.parametrize(
    ("scope", "expected"),
    [
        ("workflow", "1800"),
        ("job", "1800"),
        ("step", "1800"),
        ("none", None),
    ],
)
def test_the_watchdog_is_read_at_every_scope(scope: str, expected: object) -> None:
    """Step, then job, then workflow, innermost first.

    A step-level `env` overrides the job's, which overrides the workflow's,
    so a resolver reading only the outer two can report the stated budget
    while the action receives another. The real files declare it at one
    scope, so only constructed workflows show the precedence at all.
    """
    declaration = f"    env:\n      {WATCHDOG_VARIABLE}: '{REQUIRED_WATCHDOG}'\n"
    text = (
        (declaration.replace("    ", "", 1) if scope == "workflow" else "")
        + "jobs:\n  test:\n"
        + (declaration if scope == "job" else "")
        + "    steps:\n"
        + f"      - uses: {COVERAGE_ACTION}@abc\n"
        + (
            f"        env:\n          {WATCHDOG_VARIABLE}: '{REQUIRED_WATCHDOG}'\n"
            if scope == "step"
            else ""
        )
    )
    jobs = coverage_jobs_in({"ci.yml": text})

    assert len(jobs) == 1
    assert jobs[0][2] == expected


def test_an_inner_scope_wins_over_an_outer_one() -> None:
    """The precedence itself, not merely that each scope is read.

    Asserting each scope separately is satisfied by a resolver that reads
    them in the wrong order, because only one is ever set in those cases.
    """
    text = (
        f"env:\n  {WATCHDOG_VARIABLE}: '60'\n"
        "jobs:\n  test:\n"
        f"    env:\n      {WATCHDOG_VARIABLE}: '120'\n"
        "    steps:\n"
        f"      - uses: {COVERAGE_ACTION}@abc\n"
        f"        env:\n          {WATCHDOG_VARIABLE}: '1800'\n"
    )
    jobs = coverage_jobs_in({"ci.yml": text})

    assert jobs[0][2] == "1800"


def test_two_coverage_steps_in_one_job_report_separately() -> None:
    """One row per coverage step, so a disagreeing second step is visible.

    Reporting one watchdog per job would let a second invocation carry a
    different budget with nothing to say so.
    """
    text = (
        "jobs:\n  test:\n    steps:\n"
        f"      - uses: {COVERAGE_ACTION}@abc\n"
        f"        env:\n          {WATCHDOG_VARIABLE}: '1800'\n"
        f"      - uses: {COVERAGE_ACTION}@abc\n"
        f"        env:\n          {WATCHDOG_VARIABLE}: '60'\n"
    )
    jobs = coverage_jobs_in({"ci.yml": text})

    assert [job[2] for job in jobs] == ["1800", "60"]


def test_two_contract_command_steps_are_both_reported() -> None:
    """The contract asserts exactly one, so the reader must find both."""
    text = (
        "jobs:\n  lint:\n    steps:\n"
        f"      - run: {CONTRACT_COMMAND}\n"
        "  check:\n    steps:\n"
        f"      - run: {CONTRACT_COMMAND}\n"
    )
    found = command_steps(parse_workflow("ci.yml", text), CONTRACT_COMMAND)

    assert [step.job for step in found] == ["lint", "check"]
