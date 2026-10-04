"""Every `sccache --show-stats` reader stands down for a declared sccache fallback.

`setup-rust` gives sccache a startup timeout and, if the server still will not
start, clears the compiler wrapper, raises a `sccache-fallback` annotation and
sets its `sccache-status` output to `fallback`. Three steps read the cache's
statistics: the cache report, the JSON write and the health check. With no
wrapper and no server they would print a table of zeros for a job that was
never cached, and the health check would turn the job red and hide a fallback
the action already announced. Each therefore reads the output of the
`setup-rust` step by its id and skips on `fallback`, and a companion step keeps
the skip visible. Anything else (`started`, empty on an older pin) still runs
all three, so a genuinely broken integration is still refused.

The contracts here hold the step id, the guard on every reader and the notice
to both workflows. They also evaluate each reader's real `if:` for a
`fallback` status and for normal ones with a small evaluator that models only
the operators the guards use and refuses the rest, and reject synthetic guards
that would run under a fallback. They cannot run `setup-rust` itself, which has
no input that forces its fallback.
"""

from __future__ import annotations

import typing as typ

import pytest
from workflow_condition import STATUS_PATH, UnmodelledExpressionError, evaluate
from workflow_contracts import of_type, parse

#: Every workflow that runs the health check, with the `setup-rust` step id.
WORKFLOWS: typ.Final = ("ci.yml", "coverage-main.yml")
SETUP_ID: typ.Final = "setup-rust"
STATUS: typ.Final = "steps.setup-rust.outputs.sccache-status"
RUN_GUARD: typ.Final = f"always() && {STATUS} != 'fallback'"
NOTE_GUARD: typ.Final = f"always() && {STATUS} == 'fallback'"


def build_steps(text: str, workflow: str) -> list[dict[str, object]]:
    """Return the steps of a workflow's only job that runs the health check.

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
        if "check_sccache_health.py" in str(job)
    ]
    return [of_type(step, dict) for step in steps]


@pytest.mark.parametrize("workflow", WORKFLOWS)
def test_the_health_check_skips_only_on_a_declared_fallback(
    workflow: str, workflow_texts: dict[str, str]
) -> None:
    """Skip the check on `fallback`, and only there, with a visible note.

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
    check = [s for s in steps if "check_sccache_health.py" in str(s.get("run"))]
    assert len(check) == 1, f"{workflow}: expected one health check step"
    assert check[0].get("if") == RUN_GUARD, (
        f"{workflow}: the health check must run unless the status is 'fallback', "
        f"found {check[0].get('if')!r}"
    )
    note = [s for s in steps if s.get("if") == NOTE_GUARD]
    assert len(note) == 1, f"{workflow}: expected one note step for a fallback"
    assert "::notice" in str(note[0].get("run")), (
        f"{workflow}: the fallback note must be a notice annotation"
    )
    assert steps.index(note[0]) == steps.index(check[0]) + 1, (
        f"{workflow}: the note must follow the check it stands in for"
    )


@pytest.mark.parametrize("workflow", WORKFLOWS)
def test_every_step_reading_sccache_stats_stands_down_on_a_fallback(
    workflow: str, workflow_texts: dict[str, str]
) -> None:
    """Guard each `sccache --show-stats` step, not only the health check.

    With the wrapper cleared and no server, a stats read prints a table of
    zero requests for a job that was never cached; the guard keeps that noise
    out of a declared fallback.

    Parameters
    ----------
    workflow : str
        The workflow file to read.
    workflow_texts : dict[str, str]
        Every workflow's text, from the shared fixture.
    """
    steps = build_steps(workflow_texts[workflow], workflow)
    readers = [s for s in steps if "sccache --show-stats" in str(s.get("run"))]
    assert len(readers) == 2, f"{workflow}: expected the report and the JSON write"
    for step in readers:
        assert step.get("if") == RUN_GUARD, (
            f"{workflow}: {step.get('name')!r} must stand down on a fallback, "
            f"found {step.get('if')!r}"
        )


def runs(step: dict[str, object], status: str) -> bool:
    """Return whether a step runs for a status, an omitted `if` meaning it does."""
    condition = step.get("if")
    return True if condition is None else evaluate(str(condition), status)


@pytest.mark.parametrize("workflow", WORKFLOWS)
def test_each_stats_reader_is_skipped_under_a_fallback_and_runs_otherwise(
    workflow: str, workflow_texts: dict[str, str]
) -> None:
    """Execute each reader's real `if:` for a fallback and for normal statuses.

    The expression is evaluated, not compared as text, so an equivalent guard
    passes and a guard that merely resembles the right one does not. What this
    cannot do is run `setup-rust`: the action has no input that forces its
    fallback in a hosted run, which only a real sccache startup failure
    reaches.

    Parameters
    ----------
    workflow : str
        The workflow file to read.
    workflow_texts : dict[str, str]
        Every workflow's text, from the shared fixture.
    """
    steps = build_steps(workflow_texts[workflow], workflow)
    readers = [s for s in steps if "sccache --show-stats" in str(s.get("run"))]
    readers += [s for s in steps if "check_sccache_health.py" in str(s.get("run"))]
    assert len(readers) == 3, (
        f"{workflow}: expected the report, the JSON write and the check"
    )
    for step in readers:
        assert not runs(step, "fallback"), (
            f"{workflow}: {step.get('name')!r} runs under a fallback"
        )
        for status in ("started", "active", ""):
            assert runs(step, status), (
                f"{workflow}: {step.get('name')!r} is skipped for status {status!r}"
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
