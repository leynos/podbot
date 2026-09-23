"""The publisher readings, driven on guards and steps this repository lacks.

``codescene_publisher_test`` asserts the real upload step, which uses
one spelling of each guard and binding, so it agrees with a broken
reading as readily as with a working one. Each case here is the shape
the reading exists to refuse, or the shape it must still accept.

Run via ``make test-workflow-contracts``.
"""

from __future__ import annotations

import typing as typ

import pytest
from publisher_rules import (
    MAIN_REF_CONJUNCT,
    binds_the_credential,
    cancelling_scopes,
    guard_conjuncts,
    requires,
    stray_credential_sites,
)
from workflow_reading import load_workflow

#: The binding the upload step carries, as parsed.
BOUND: typ.Final[dict[str, str]] = {"CS_ACCESS_TOKEN": "${{ secrets.CS_ACCESS_TOKEN }}"}
PASSED: typ.Final[dict[str, str]] = {"access-token": "${{ env.CS_ACCESS_TOKEN }}"}


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
    "step",
    [
        pytest.param({"with": PASSED}, id="binding-deleted"),
        pytest.param({"env": BOUND}, id="input-deleted"),
        pytest.param(
            {"env": {"CS_ACCESS_TOKEN": "${{ secrets.OTHER }}"}, "with": PASSED},
            id="another-secret",
        ),
        pytest.param(
            {"env": BOUND, "with": {"access-token": "literal"}}, id="literal-input"
        ),
        pytest.param({"env": "not-a-mapping", "with": PASSED}, id="env-malformed"),
    ],
)
def test_an_upload_step_missing_half_the_binding_is_refused(
    step: dict[str, object],
) -> None:
    """Either half deleted leaves the guard well formed and the upload skipped."""
    assert not binds_the_credential(step), step


#: A publisher whose upload is job ``a``'s second step, with a stray
#: read of the secret at each place a wider or neighbouring scope puts it.
STRAYS: typ.Final[str] = (
    "env:\n  T: ${{ secrets.CS_ACCESS_TOKEN }}\n"
    "jobs:\n  a:\n    env:\n      CS_ACCESS_TOKEN: x\n    steps:\n"
    "      - run: echo ${{ env.CS_ACCESS_TOKEN }}\n"
    "      - if: env.CS_ACCESS_TOKEN != ''\n"
    "        env:\n          CS_ACCESS_TOKEN: ${{ secrets.CS_ACCESS_TOKEN }}\n"
    "        uses: x/upload@v1\n"
    "        with:\n          access-token: ${{ env.CS_ACCESS_TOKEN }}\n"
)


def test_a_stray_credential_is_found_and_the_upload_step_is_not() -> None:
    """The upload step's three uses are allowed; every other read is a stray."""
    strays = stray_credential_sites("m.yml", load_workflow(STRAYS), "jobs.a.steps[1]")
    assert strays == [
        "m.yml: env.T",
        "m.yml: jobs.a.env.CS_ACCESS_TOKEN<key>",
        "m.yml: jobs.a.steps[0].run",
    ]


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
