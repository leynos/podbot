"""Check the Markdown formatting wiring against the estate baseline.

The real Makefile and workflows must comply, and each clause must reject the
edit it exists to catch while accepting legitimate variants of the same
command, so that the rule is proved narrow as well as sufficient.
"""

from __future__ import annotations

import copy
from pathlib import Path

import pytest
from hypothesis import given
from hypothesis import strategies as st
from markdown_wiring_rules import (
    ESTATE_FLAGS,
    INSTALL_ACTION,
    LINT_ACTION,
    calls,
    check_fmt_steps,
    install_precedes_check_fmt,
    job_steps,
    lint_action_globs,
    lint_action_steps,
    runs_mdtablefix,
    runs_mdtablefix_check,
)
from workflow_reading import WorkflowDocument, read_workflows

ROOT = Path(__file__).resolve().parents[2]
WORKFLOWS = ROOT / ".github" / "workflows"
MAKEFILE = ROOT / "Makefile"
CHECK = "\t$(MDTABLEFIX) --check $(MDTABLEFIX_SELECT) $(MDTABLEFIX_RULES)"
REWRITE = "\t$(MDTABLEFIX) --in-place $(MDTABLEFIX_SELECT) $(MDTABLEFIX_RULES)"
VARIABLES = (
    "MDTABLEFIX ?= mdtablefix\n"
    "MDTABLEFIX_SELECT = --git --include-untracked\n"
    "MDTABLEFIX_RULES = --wrap --renumber --breaks --ellipsis --fences\n"
)
ALL_FLAGS = sorted(ESTATE_FLAGS)
LITERAL = "mdtablefix {mode} " + " ".join(ALL_FLAGS)


def fresh_documents() -> dict[str, WorkflowDocument]:
    """Read a private copy of the workflows for one test to mutate."""
    return copy.deepcopy(read_workflows(WORKFLOWS))


def _makefile(target: str, recipe_line: str, variables: str = VARIABLES) -> str:
    """Return a minimal Makefile whose ``target`` runs one line."""
    return f"{variables}\n{target}: ## A target\n{recipe_line}\n"


def _steps_matching(documents: dict[str, WorkflowDocument], action: str) -> list[dict]:
    """Return every step across ``documents`` that calls ``action``."""
    return [
        step
        for _, steps in job_steps(documents)
        for step in steps
        if calls(step, action)
    ]


def _move_installs_last(documents: dict[str, WorkflowDocument]) -> None:
    """Move each job's install step to the end of its steps, in place."""
    for _, steps in job_steps(documents):
        installs = [step for step in steps if calls(step, INSTALL_ACTION)]
        for step in installs:
            steps.remove(step)
            steps.append(step)


def test_the_repository_makefile_runs_the_check() -> None:
    """`make check-fmt` here runs `mdtablefix --check` with every estate flag."""
    assert runs_mdtablefix_check(MAKEFILE.read_text(encoding="utf-8"))


def test_the_repository_makefile_runs_the_rewrite() -> None:
    """`make fmt` here runs `mdtablefix --in-place` with every estate flag."""
    assert runs_mdtablefix(MAKEFILE.read_text(encoding="utf-8"), "fmt")


def test_the_repository_workflows_install_and_lint() -> None:
    """CI installs mdtablefix before check-fmt and lints every Markdown file."""
    documents = fresh_documents()
    assert check_fmt_steps(documents) > 0, "no job runs `make check-fmt`"
    assert install_precedes_check_fmt(documents) == []
    assert lint_action_globs(documents) == []
    assert lint_action_steps(documents) > 0


@pytest.mark.parametrize(
    ("target", "line"),
    [
        ("check-fmt", CHECK.replace(" --check", "")),
        ("check-fmt", CHECK.replace(" $(MDTABLEFIX_SELECT)", "")),
        ("check-fmt", CHECK.replace(" $(MDTABLEFIX_RULES)", "")),
        ("check-fmt", "\t-" + CHECK.lstrip("\t")),
        ("check-fmt", CHECK + " || true"),
        ("check-fmt", CHECK + "; true"),
        ("check-fmt", "\techo " + CHECK.lstrip("\t")),
        ("fmt", REWRITE.replace(" --in-place", "")),
        ("fmt", REWRITE.replace(" $(MDTABLEFIX_RULES)", "")),
        ("fmt", "\t-" + REWRITE.lstrip("\t")),
        ("fmt", REWRITE + " || true"),
    ],
    ids=[
        "check_no_mode",
        "check_no_select",
        "check_no_rules",
        "check_ignored",
        "check_or_true",
        "check_sequenced",
        "check_echoed",
        "fmt_no_mode",
        "fmt_no_rules",
        "fmt_ignored",
        "fmt_or_true",
    ],
)
def test_a_weakened_command_is_refused(target: str, line: str) -> None:
    """Each weakening of the check-fmt or fmt command is caught."""
    assert not runs_mdtablefix(_makefile(target, line), target)


@pytest.mark.parametrize("dropped", ALL_FLAGS)
def test_dropping_any_single_estate_flag_is_refused(dropped: str) -> None:
    """Every flag is required on its own, not only the set as a whole."""
    flags = " ".join(flag for flag in ALL_FLAGS if flag != dropped)
    line = "\t" + LITERAL.format(mode="--check").replace(" ".join(ALL_FLAGS), flags)
    assert not runs_mdtablefix_check(_makefile("check-fmt", line, ""))


@pytest.mark.parametrize(
    ("target", "line", "variables"),
    [
        ("check-fmt", CHECK, VARIABLES),
        ("check-fmt", "\t" + LITERAL.format(mode="--check"), ""),
        ("check-fmt", "\t@" + CHECK.lstrip("\t"), VARIABLES),
        ("check-fmt", "\tcargo fmt --check && " + LITERAL.format(mode="--check"), ""),
        ("check-fmt", "\t/usr/local/bin/" + LITERAL.format(mode="--check"), ""),
        ("fmt", REWRITE, VARIABLES),
        ("fmt", "\t" + LITERAL.format(mode="--in-place"), ""),
    ],
    ids=[
        "check_reference",
        "check_literal",
        "check_silenced",
        "check_chained",
        "check_qualified",
        "fmt_reference",
        "fmt_literal",
    ],
)
def test_a_legitimate_command_is_accepted(
    target: str, line: str, variables: str
) -> None:
    """A literal command, a silenced one, an `&&` chain or a path still comply."""
    assert runs_mdtablefix(_makefile(target, line, variables), target)


@given(st.permutations(ALL_FLAGS))
def test_flag_order_never_matters(order: list[str]) -> None:
    """Any ordering of the required flags is compliant, for both targets."""
    words = " ".join(order)
    check = _makefile("check-fmt", f"\tmdtablefix --check {words}", "")
    rewrite = _makefile("fmt", f"\tmdtablefix {words} --in-place", "")
    assert runs_mdtablefix_check(check)
    assert runs_mdtablefix(rewrite, "fmt")


@given(
    st.sampled_from(ALL_FLAGS),
    st.sampled_from([" || true", "; true", " | cat", " || :"]),
)
def test_a_dropped_flag_or_a_masked_status_is_never_compliant(
    dropped: str, mask: str
) -> None:
    """Missing any flag, or handing the status on, fails whatever the other is."""
    words = " ".join(flag for flag in ALL_FLAGS if flag != dropped)
    missing = _makefile("check-fmt", f"\tmdtablefix --check {words}", "")
    masked = _makefile("check-fmt", "\t" + LITERAL.format(mode="--check") + mask, "")
    assert not runs_mdtablefix_check(missing)
    assert not runs_mdtablefix_check(masked)


def test_an_install_after_check_fmt_is_refused() -> None:
    """Moving the install step below check-fmt is caught."""
    documents = fresh_documents()
    _move_installs_last(documents)
    assert install_precedes_check_fmt(documents) != []


def test_a_conditional_install_is_refused() -> None:
    """An install with an `if:` may be skipped, so it does not count."""
    documents = fresh_documents()
    for step in _steps_matching(documents, INSTALL_ACTION):
        step["if"] = "false"
    assert install_precedes_check_fmt(documents) != []


def test_removing_every_check_fmt_step_is_visible() -> None:
    """With no `make check-fmt` step the install rule is vacuous, and says so."""
    documents = fresh_documents()
    for _, steps in job_steps(documents):
        steps[:] = [step for step in steps if "check-fmt" not in str(step.get("run"))]
    assert install_precedes_check_fmt(documents) == []
    assert check_fmt_steps(documents) == 0


def test_narrowed_lint_globs_are_refused() -> None:
    """A markdownlint-cli2-action step linting less than `**/*.md` is caught."""
    documents = fresh_documents()
    for step in _steps_matching(documents, LINT_ACTION):
        step["with"] = {"globs": "docs/**/*.md"}
    assert lint_action_globs(documents) != []


def test_every_lint_step_is_counted() -> None:
    """The step count is what stops an empty list from passing as compliance."""
    documents = fresh_documents()
    total = lint_action_steps(documents)
    assert total == len(_steps_matching(documents, LINT_ACTION))
