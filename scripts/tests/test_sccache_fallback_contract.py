"""Every `sccache --show-stats` reader stands down for a declared sccache fallback.

`setup-rust` gives sccache a startup timeout and, if the server still will not
start, clears the compiler wrapper, raises a `sccache-fallback` annotation and
sets its `sccache-status` output to `fallback`. Three steps read the cache's
statistics: the cache report, the JSON write and the health check. With no
wrapper and no server they would print a table of zeros for a job that was
never cached, and the health check would turn the job red and hide a fallback
the action already announced. The three live in one local action,
`.github/actions/sccache-readers`, which every lane calls with `setup-rust`'s
status, and each skips on `fallback`; a companion step keeps the skip visible.
Anything else (`started`, empty on an older pin) still runs all three, so a
genuinely broken integration is still refused.

The contracts here hold the call in both workflows, the step id it reads and
the guard on every reader. They also evaluate each reader's real `if:` for a
`fallback` status and for normal ones with a small evaluator that models only
the operators the guards use and refuses the rest, and reject synthetic guards
that would run under a fallback. `sccache-readers-e2e.yml` then has GitHub
evaluate the same guards on a runner, which this module cannot do because
`setup-rust` has no input that forces its fallback.
"""

from __future__ import annotations

import re
import typing as typ

import pytest
from workflow_condition import STATUS_PATH, UnmodelledExpressionError, evaluate
from workflow_contracts import of_type, parse
from workflow_coverage import READERS_ACTION, READERS_ACTION_FILE

#: Every workflow that calls the readers, with the `setup-rust` step id.
WORKFLOWS: typ.Final = ("ci.yml", "coverage-main.yml")
SETUP_ID: typ.Final = "setup-rust"
#: What a caller hands the action: the status output of the setup step.
CALLER_STATUS: typ.Final = "${{ steps.setup-rust.outputs.sccache-status }}"
RUN_GUARD: typ.Final = f"always() && {STATUS_PATH} != 'fallback'"
NOTE_GUARD: typ.Final = f"always() && {STATUS_PATH} == 'fallback'"


def build_steps(text: str, workflow: str) -> list[dict[str, object]]:
    """Return the steps of a workflow's only job that calls the readers.

    Parameters
    ----------
    text : str
        The workflow file's text.
    workflow : str
        Its file name, used in the parse error.

    Returns
    -------
    list[dict[str, object]]
        The job's steps, in order.
    """
    jobs = of_type(parse(workflow, text).get("jobs"), dict)
    (steps,) = [
        of_type(of_type(job, dict).get("steps"), list)
        for job in jobs.values()
        if READERS_ACTION in str(job)
    ]
    return [of_type(step, dict) for step in steps]


def action_steps() -> list[dict[str, object]]:
    """Return the readers action's steps, in order.

    Returns
    -------
    list[dict[str, object]]
        The composite action's steps.
    """
    document = parse(
        READERS_ACTION_FILE.name, READERS_ACTION_FILE.read_text(encoding="utf-8")
    )
    steps = of_type(of_type(document.get("runs"), dict).get("steps"), list)
    return [of_type(step, dict) for step in steps]


@pytest.mark.parametrize("workflow", WORKFLOWS)
def test_each_lane_hands_the_readers_the_setup_status(
    workflow: str, workflow_texts: dict[str, str]
) -> None:
    """Call the readers once, after `setup-rust`, under `always()`, with its status.

    Parameters
    ----------
    workflow : str
        The workflow file to read.
    workflow_texts : dict[str, str]
        Every workflow's text, from the shared fixture.
    """
    steps = build_steps(workflow_texts[workflow], workflow)
    setup = [s for s in steps if "setup-rust@" in str(s.get("uses"))]
    assert len(setup) == 1, f"{workflow}: expected one setup-rust step"
    assert setup[0].get("id") == SETUP_ID, (
        f"{workflow}: the setup-rust step needs id {SETUP_ID!r} so its output can be read"
    )
    calls = [s for s in steps if s.get("uses") == READERS_ACTION]
    assert len(calls) == 1, f"{workflow}: expected one call of {READERS_ACTION}"
    (call,) = calls
    assert call.get("if") == "always()", (
        f"{workflow}: the readers must run under always(), found {call.get('if')!r}"
    )
    given = of_type(call.get("with"), dict).get("sccache-status")
    assert given == CALLER_STATUS, (
        f"{workflow}: the readers must receive {CALLER_STATUS!r}, found {given!r}"
    )
    assert steps.index(setup[0]) < steps.index(call), (
        f"{workflow}: the readers must come after the step that sets the status"
    )


@pytest.mark.parametrize("workflow", WORKFLOWS)
def test_no_lane_reads_sccache_statistics_outside_the_readers_action(
    workflow: str, workflow_texts: dict[str, str]
) -> None:
    """Keep every statistics reader inside the guarded action.

    A lane that re-added its own `sccache --show-stats` or health-check step
    would bypass the fallback guard the action carries, so no step of either
    workflow may run one itself.

    Parameters
    ----------
    workflow : str
        The workflow file to read.
    workflow_texts : dict[str, str]
        Every workflow's text, from the shared fixture.
    """
    for step in build_steps(workflow_texts[workflow], workflow):
        command = str(step.get("run", ""))
        assert not re.search(r"sccache\s+--show-stats", command), (
            f"{workflow}: {step.get('name')!r} reads sccache outside {READERS_ACTION}"
        )
        assert "check_sccache_health.py" not in command, (
            f"{workflow}: {step.get('name')!r} checks cache health outside "
            f"{READERS_ACTION}"
        )


def test_the_readers_skip_only_on_a_declared_fallback_with_a_visible_note() -> None:
    """Guard each reader on `fallback`, and keep the skip visible after the check."""
    steps = action_steps()
    readers = [s for s in steps if "sccache --show-stats" in str(s.get("run"))]
    readers += [s for s in steps if "check_sccache_health.py" in str(s.get("run"))]
    assert len(readers) == 3, "expected the report, the JSON write and the check"
    for step in readers:
        assert step.get("if") == RUN_GUARD, (
            f"{step.get('name')!r} must run unless the status is 'fallback', "
            f"found {step.get('if')!r}"
        )
    check = next(s for s in steps if "check_sccache_health.py" in str(s.get("run")))
    note = [s for s in steps if s.get("if") == NOTE_GUARD]
    assert len(note) == 1, "expected one note step for a fallback"
    assert "::notice" in str(note[0].get("run")), "the note must be a notice annotation"
    assert steps.index(note[0]) == steps.index(check) + 1, (
        "the note must follow the check it stands in for"
    )
    assert all(s.get("shell") == "bash" for s in steps), (
        "every composite run step needs a shell"
    )


def runs(step: dict[str, object], status: str) -> bool:
    """Return whether a step runs for a status, an omitted `if` meaning it does."""
    condition = step.get("if")
    return True if condition is None else evaluate(str(condition), status)


def test_each_stats_reader_is_skipped_under_a_fallback_and_runs_otherwise() -> None:
    """Execute each reader's real `if:` for a fallback and for normal statuses.

    The expression is evaluated, not compared as text, so an equivalent guard
    passes and a guard that merely resembles the right one does not. What this
    cannot do is let GitHub evaluate it; the end-to-end workflow does that.
    """
    steps = action_steps()
    readers = [s for s in steps if "sccache --show-stats" in str(s.get("run"))]
    readers += [s for s in steps if "check_sccache_health.py" in str(s.get("run"))]
    assert len(readers) == 3, "expected the report, the JSON write and the check"
    for step in readers:
        assert not runs(step, "fallback"), f"{step.get('name')!r} runs under a fallback"
        for status in ("started", "active", ""):
            assert runs(step, status), (
                f"{step.get('name')!r} is skipped for status {status!r}"
            )


@pytest.mark.parametrize(
    "condition",
    [
        pytest.param(None, id="guard-omitted"),
        pytest.param("always()", id="guard-dropped"),
        pytest.param(
            f"always() && {STATUS_PATH} == 'fallback'", id="comparison-inverted"
        ),
        pytest.param(f"always() && !({STATUS_PATH} != 'fallback')", id="negated"),
        pytest.param(
            f"always() && {STATUS_PATH} != 'fallback' || always()", id="or-composition"
        ),
    ],
)
def test_a_guard_that_runs_under_a_fallback_is_rejected(condition: str | None) -> None:
    """Treat each broken guard as one that runs under a fallback.

    Parameters
    ----------
    condition : str | None
        A synthetic `if:` that fails to stand down on `fallback`.
    """
    step: dict[str, object] = {} if condition is None else {"if": condition}
    assert runs(step, "fallback"), f"{condition!r} would have been accepted"


@pytest.mark.parametrize(
    "condition",
    [
        "success()",
        "contains(steps.setup-rust.outputs.sccache-status, 'fall')",
        "steps.other.outputs.sccache-status != 'fallback'",
        "always() && steps.setup-rust.outputs.sccache-status > 'a'",
        f"always() && ({STATUS_PATH} != 'fallback'",
        "always() == 'true'",
        f"!({STATUS_PATH} == 'fallback') != 'false'",
    ],
)
def test_the_evaluator_refuses_what_it_does_not_model(condition: str) -> None:
    """Raise on syntax or contexts outside the model instead of guessing.

    Parameters
    ----------
    condition : str
        An expression using something the evaluator does not implement.
    """
    with pytest.raises(UnmodelledExpressionError):
        evaluate(condition, "fallback")
