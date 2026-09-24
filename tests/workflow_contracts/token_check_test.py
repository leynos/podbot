"""The token-check readings, driven on steps this repository does not have.

``codescene_publisher_test`` asserts the real publisher, which uses one
spelling of the check step and the upload input, so it agrees with a
broken reading as readily as with a working one. Each case here is a
shape the reading exists to refuse, or one it must still accept.

Run via ``make test-workflow-contracts``.
"""

from __future__ import annotations

import typing as typ

import pytest
from token_check import (
    TOKEN_CHECK_COMMAND,
    is_token_check,
    passes_the_secret_directly,
    stray_credential_sites,
)
from workflow_reading import load_workflow

#: The check step as the publisher writes it, parsed.
CHECK: typ.Final[dict[str, object]] = {
    "id": "codescene-token",
    "run": TOKEN_CHECK_COMMAND,
}


def test_the_publishers_check_step_is_accepted() -> None:
    """Narrow as well as sufficient: the real shape passes."""
    assert is_token_check(CHECK), (
        f"the publisher's own check shape was refused: {CHECK}"
    )


@pytest.mark.parametrize(
    "change",
    [
        pytest.param({"id": None}, id="no-id"),
        pytest.param({"if": "false"}, id="guarded"),
        pytest.param({"if": "github.ref == 'refs/heads/main'"}, id="main-only"),
        pytest.param({"continue-on-error": "true"}, id="may-fail"),
        pytest.param(
            {"run": TOKEN_CHECK_COMMAND.replace("!= ''", "== ''")}, id="inverted"
        ),
        pytest.param(
            {"run": TOKEN_CHECK_COMMAND.replace("CS_ACCESS_TOKEN", "OTHER")},
            id="other-secret",
        ),
        pytest.param(
            {"run": 'echo "available=true" >> "$GITHUB_OUTPUT"'}, id="constant"
        ),
        pytest.param({"run": f"{TOKEN_CHECK_COMMAND} || true"}, id="extra-command"),
        pytest.param({"run": f"echo {TOKEN_CHECK_COMMAND}"}, id="echoed"),
        pytest.param({"run": f"false && {TOKEN_CHECK_COMMAND}"}, id="short-circuited"),
        pytest.param(
            {"run": "python3 scripts/codescene_token_available.py"}, id="script"
        ),
    ],
)
def test_a_check_step_in_any_other_shape_is_refused(change: dict[str, object]) -> None:
    """Each change leaves a step that cannot be relied on to write the output."""
    step = {key: value for key, value in (CHECK | change).items() if value is not None}
    assert not is_token_check(step), step


@pytest.mark.parametrize(
    ("step", "passes"),
    [
        pytest.param(
            {"with": {"access-token": "${{ secrets.CS_ACCESS_TOKEN }}"}},
            True,
            id="direct",
        ),
        pytest.param(
            {
                "env": {"CS_ACCESS_TOKEN": "${{ secrets.CS_ACCESS_TOKEN }}"},
                "with": {"access-token": "${{ secrets.CS_ACCESS_TOKEN }}"},
            },
            False,
            id="also-in-env",
        ),
        pytest.param(
            {"with": {"access-token": "${{ env.CS_ACCESS_TOKEN }}"}},
            False,
            id="via-env",
        ),
        pytest.param({"with": {}}, False, id="input-deleted"),
        pytest.param({"with": {"access-token": "literal"}}, False, id="literal"),
    ],
)
def test_the_upload_takes_the_secret_as_its_input_alone(
    step: dict[str, object], *, passes: bool
) -> None:
    """Bound in the upload step's env, the secret reaches every nested step."""
    assert passes_the_secret_directly(step) is passes, step


#: A publisher with the check at step 1 and the upload at step 2 of job
#: `a`, and a stray read of the secret at each other scope.
STRAYS: typ.Final[str] = (
    "env:\n  T: ${{ secrets.CS_ACCESS_TOKEN }}\n"
    "jobs:\n  a:\n    env:\n      CS_ACCESS_TOKEN: x\n    steps:\n"
    "      - run: echo ${{ secrets.CS_ACCESS_TOKEN }}\n"
    "      - id: t\n        env:\n          CS_ACCESS_TOKEN: ${{ secrets.CS_ACCESS_TOKEN }}\n"
    f"        run: {TOKEN_CHECK_COMMAND}\n"
    "      - uses: x/upload@v1\n"
    "        env:\n          CS_ACCESS_TOKEN: ${{ secrets.CS_ACCESS_TOKEN }}\n"
    "        with:\n          access-token: ${{ secrets.CS_ACCESS_TOKEN }}\n"
)


def test_every_read_but_the_two_allowed_is_a_stray() -> None:
    """The check's command and the upload's input are allowed; nothing else is."""
    strays = stray_credential_sites(
        "m.yml", load_workflow(STRAYS), "jobs.a.steps[1]", "jobs.a.steps[2]"
    )
    assert strays == [
        "m.yml: env.T",
        "m.yml: jobs.a.env.CS_ACCESS_TOKEN<key>",
        "m.yml: jobs.a.steps[0].run",
        "m.yml: jobs.a.steps[1].env.CS_ACCESS_TOKEN<key>",
        "m.yml: jobs.a.steps[1].env.CS_ACCESS_TOKEN",
        "m.yml: jobs.a.steps[2].env.CS_ACCESS_TOKEN<key>",
        "m.yml: jobs.a.steps[2].env.CS_ACCESS_TOKEN",
    ], f"unexpected stray sites: {strays}"
