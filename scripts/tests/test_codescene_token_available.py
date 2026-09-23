"""The token check reports presence and the decision, never the secret.

Run via ``make workflow-contracts``.
"""

from __future__ import annotations

import importlib.util
import os
import subprocess
import sys
import types
import typing as typ
from pathlib import Path

import pytest
from hypothesis import given
from hypothesis import strategies as st

SCRIPT = Path(__file__).resolve().parents[1] / "codescene_token_available.py"

#: A token value no line the script writes may contain.
SECRET = "cs-token-3f9a"


@pytest.fixture(scope="module")
def checker() -> types.ModuleType:
    """Import the token check script as a module."""
    spec = importlib.util.spec_from_file_location("codescene_token_available", SCRIPT)
    if spec is None or spec.loader is None:
        message = "could not load the token check script"
        raise ImportError(message)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.mark.parametrize(
    ("token", "expected"),
    [
        pytest.param(SECRET, "available=true", id="set"),
        pytest.param("", "available=false", id="empty"),
        pytest.param(" \t", "available=false", id="blank"),
        pytest.param(None, "available=false", id="absent"),
    ],
)
def test_the_output_says_whether_the_token_is_there(
    checker: typ.Any, tmp_path: Path, token: str | None, expected: str
) -> None:
    """Exactly one line is appended, and the secret's value is not in it."""
    output = tmp_path / "output"
    output.write_text("earlier=1\n", encoding="utf-8")
    environment = {"GITHUB_OUTPUT": str(output)}
    if token is not None:
        environment["CS_ACCESS_TOKEN"] = token

    assert checker.main(environment) == 0
    written = output.read_text(encoding="utf-8")
    assert written == f"earlier=1\n{expected}\n", written


def test_a_missing_output_file_fails_loudly(
    checker: typ.Any, capsys: pytest.CaptureFixture[str]
) -> None:
    """Without GITHUB_OUTPUT the upload would skip forever, so this fails."""
    assert checker.main({"CS_ACCESS_TOKEN": SECRET}) == 1
    assert "::error::" in capsys.readouterr().out


@given(token=st.one_of(st.none(), st.text()))
def test_the_output_is_one_of_two_fixed_lines(token: str | None) -> None:
    """Whatever the token is, the output line is fixed and carries none of it.

    Unicode, control characters and every kind of whitespace are drawn, so
    the rule that only a non-blank token counts is checked over the space
    rather than at four points.
    """
    spec = importlib.util.spec_from_file_location("codescene_token_available", SCRIPT)
    assert spec is not None and spec.loader is not None, "the script must load"
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    environment = {} if token is None else {"CS_ACCESS_TOKEN": token}

    line = module.availability(environment)

    expected = "available=true" if token and token.strip() else "available=false"
    assert line == expected, (token, line)
    assert module.decision(environment).startswith("operation=codescene_upload "), line


def _run(environment: dict[str, str]) -> subprocess.CompletedProcess[str]:
    """Run the script as the workflow does, in a child process."""
    return subprocess.run(  # noqa: S603 - fixed argv
        [sys.executable, str(SCRIPT)],
        env={"PATH": os.environ.get("PATH", "")} | environment,
        capture_output=True,
        text=True,
        check=False,
    )


class CommandCase(typ.NamedTuple):
    """One end-to-end run: the environment given and what must come out."""

    token: str
    ref: str
    expected: str
    record: str


@pytest.mark.parametrize(
    "case",
    [
        pytest.param(
            CommandCase(SECRET, "refs/heads/main", "available=true", "decision=upload"),
            id="main",
        ),
        pytest.param(
            CommandCase(
                "", "refs/heads/main", "available=false", "decision=skip_no_token"
            ),
            id="no-token",
        ),
        pytest.param(
            CommandCase(
                SECRET, "refs/heads/wip", "available=true", "decision=skip_not_main"
            ),
            id="branch",
        ),
    ],
)
def test_the_command_writes_the_output_and_a_secret_free_record(
    tmp_path: Path, case: CommandCase
) -> None:
    """End to end: the exact command, a real output file, a real summary."""
    output, summary = tmp_path / "output", tmp_path / "summary"
    result = _run(
        {
            "CS_ACCESS_TOKEN": case.token,
            "GITHUB_REF": case.ref,
            "GITHUB_OUTPUT": str(output),
            "GITHUB_STEP_SUMMARY": str(summary),
        }
    )

    assert result.returncode == 0, result.stdout + result.stderr
    assert output.read_text(encoding="utf-8") == f"{case.expected}\n"
    everything = result.stdout + result.stderr + summary.read_text(encoding="utf-8")
    assert case.record in result.stdout and case.record in everything, everything
    assert SECRET not in everything, "the secret must never be written"


def test_the_command_fails_without_an_output_file() -> None:
    """End to end: exit status 1 and an error annotation."""
    result = _run({"CS_ACCESS_TOKEN": SECRET})

    assert result.returncode == 1, result.stdout
    assert "::error::" in result.stdout, result.stdout
    assert SECRET not in result.stdout + result.stderr
