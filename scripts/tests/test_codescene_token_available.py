"""The token check reports presence and never the secret itself.

Run via ``make workflow-contracts``.
"""

from __future__ import annotations

import importlib.util
import types
import typing as typ
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parents[1] / "codescene_token_available.py"


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
        pytest.param("a-token", "available=true", id="set"),
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
    assert output.read_text(encoding="utf-8") == f"earlier=1\n{expected}\n"


def test_a_missing_output_file_fails_loudly(
    checker: typ.Any, capsys: pytest.CaptureFixture[str]
) -> None:
    """Without GITHUB_OUTPUT the upload would skip forever, so this fails."""
    assert checker.main({"CS_ACCESS_TOKEN": "a-token"}) == 1
    assert "::error::" in capsys.readouterr().out
