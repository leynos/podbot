"""Check readable output from Cargo's structured nested-target build."""

from __future__ import annotations

import json
import pathlib
import types
import typing as typ

import pytest
import test_runner_nested


def test_json_build_hides_messages_and_keeps_rendered_diagnostics(
    capsys: pytest.CaptureFixture[str], tmp_path: pathlib.Path
) -> None:
    """Cargo messages stay structured while human output remains readable."""
    artifact = {
        "reason": "compiler-artifact",
        "package_id": "podbot 0.1.0",
        "target": {"name": "compile_contract"},
    }
    diagnostic = {
        "reason": "compiler-message",
        "message": {"rendered": "warning: fixture warning\n"},
    }
    lines = [
        json.dumps(artifact) + "\n",
        json.dumps(diagnostic) + "\n",
        "cargo wrapper notice\n",
    ]

    class LineSupervisor:
        """Send controlled JSON and ordinary output through the callback."""

        def run_lines(
            self, _request: object, emit_line: typ.Callable[[str], None]
        ) -> int:
            for line in lines:
                emit_line(line)
            return 7

    context = types.SimpleNamespace(
        supervisor=LineSupervisor(), cwd=tmp_path, environment={}
    )
    messages: list[dict[str, typ.Any]] = []

    status = test_runner_nested._run_json_build(
        context, ["cargo", "test", "--no-run"], messages
    )

    captured = capsys.readouterr()
    assert status == 7, "the nested Cargo exit status must be preserved"
    assert messages == [artifact, diagnostic], (
        "Cargo JSON must remain available to the runner"
    )
    assert captured.out == "cargo wrapper notice\n", (
        "ordinary Cargo wrapper output must remain visible without raw JSON"
    )
    assert captured.err == "warning: fixture warning\n", (
        "rendered compiler diagnostics must remain visible to the developer"
    )
