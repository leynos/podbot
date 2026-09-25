"""The publisher readings, driven on guards and steps this repository lacks.

``codescene_publisher_test`` asserts the real upload step, which uses
one spelling of each guard, so it agrees with a broken reading as
readily as with a working one. Each case here is the shape
the reading exists to refuse, or the shape it must still accept.

Run via ``make test-workflow-contracts``.
"""

from __future__ import annotations

import pytest
from publisher_rules import (
    MAIN_REF_CONJUNCT,
    cancelling_scopes,
    guard_conjuncts,
    requires,
)
from workflow_reading import load_workflow


@pytest.mark.parametrize(
    "condition",
    [
        pytest.param(
            "env.CS_ACCESS_TOKEN != '' && github.ref == 'refs/heads/main' "
            "|| github.event_name == 'workflow_dispatch'",
            id="appended-disjunction",
        ),
        pytest.param(
            "github.ref == 'refs/heads/main' || github.event_name == "
            "'workflow_dispatch' && env.CS_ACCESS_TOKEN != ''",
            id="ref-first-disjunction",
        ),
        pytest.param(
            "env.CS_ACCESS_TOKEN != '' || github.event_name == "
            "'workflow_dispatch' && github.ref == 'refs/heads/main'",
            id="ref-last-disjunction",
        ),
        pytest.param("${{ env.CS_ACCESS_TOKEN != '' }}", id="token-only"),
        pytest.param("github.ref != 'refs/heads/main'", id="negated-ref"),
        pytest.param("", id="absent"),
    ],
)
def test_a_guard_that_does_not_confine_to_main_is_refused(condition: str) -> None:
    """The ref test must be a conjunct, and no ``||`` may appear.

    ``&&`` binds tighter than ``||``, so a split on ``&&`` alone can
    yield the ref test whole while a disjunction makes it optional. The
    ref-last case is the one that proves the ``||`` refusal: split
    naively its last conjunct is exactly the ref test, yet a token-bearing
    run on any branch satisfies the first disjunct. The appended and
    ref-first cases fail the comparison with or without the refusal.
    """
    assert not requires(condition, MAIN_REF_CONJUNCT), condition


@pytest.mark.parametrize(
    "condition",
    [
        pytest.param(
            "github.ref == 'refs/heads/main' && env.CS_ACCESS_TOKEN != ''", id="bare"
        ),
        pytest.param(
            "${{ env.CS_ACCESS_TOKEN != ''  &&\n  github.ref  ==  'refs/heads/main' }}",
            id="wrapped-and-spaced",
        ),
        pytest.param(
            "github.ref == 'refs/heads/main' && github.actor != 'a||b'",
            id="quoted-operator",
        ),
    ],
)
def test_a_guard_that_confines_to_main_is_accepted(condition: str) -> None:
    """Wrapper, whitespace and a quoted ``||`` do not change the reading."""
    assert requires(condition, MAIN_REF_CONJUNCT), condition


def test_the_conjuncts_are_split_outside_quotes() -> None:
    """A quoted ``&&`` stays inside its conjunct."""
    assert guard_conjuncts("a == 'x && y' && b") == ["a == 'x && y'", "b"]


@pytest.mark.parametrize(
    ("body", "expected"),
    [
        pytest.param(
            "concurrency:\n  group: g\n  cancel-in-progress: true\n",
            ["workflow"],
            id="workflow-true",
        ),
        pytest.param(
            "concurrency:\n  group: g\n  cancel-in-progress: ${{ github.x }}\n",
            ["workflow"],
            id="an-expression",
        ),
        pytest.param(
            "jobs:\n  a:\n    concurrency:\n      group: g\n"
            "      cancel-in-progress: true\n",
            ["job a"],
            id="job-true",
        ),
        pytest.param(
            "concurrency:\n  group: g\n  cancel-in-progress: false\n", [], id="false"
        ),
        pytest.param("concurrency:\n  group: g\n", [], id="absent"),
        pytest.param("concurrency: g\n", [], id="group-only-scalar"),
    ],
)
def test_a_cancelling_scope_is_found(body: str, expected: list[str]) -> None:
    """Only an explicit ``false``, or no setting, reads as never cancelling."""
    assert cancelling_scopes(load_workflow(body)) == expected
