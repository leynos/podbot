"""The contracts must be run by CI, by a step nothing can skip.

A contract nothing runs is a comment. These cases pin down how a step
running a command is recognized, and which guards disqualify it.
"""

from __future__ import annotations

import typing as typ

import pytest
from workflow_commands import command_steps
from workflow_contracts import parse as parse_workflow

#: The command CI must run to execute the contracts in this directory.
CONTRACT_COMMAND: typ.Final[str] = "make workflow-contracts"


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
