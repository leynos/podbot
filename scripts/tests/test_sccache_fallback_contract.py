"""The compiler-cache health check stands down for a declared sccache fallback.

`setup-rust` gives sccache a startup timeout and, if the server still will not
start, clears the compiler wrapper, raises a `sccache-fallback` annotation and
sets its `sccache-status` output to `fallback`. The health check then finds
that nothing was wrapped and would turn the job red, hiding a fallback the
action already announced. The check therefore reads the output of the
`setup-rust` step by its id, skips on `fallback`, and a companion step keeps
the skip visible. Anything else (`started`, empty on an older pin) still runs
the check, so a genuinely broken integration is still refused.
"""

from __future__ import annotations

import typing as typ

import pytest
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

    With the wrapper cleared and no server, a stats read would restart the
    dead server or report zero requests and turn the declared fallback red.

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
