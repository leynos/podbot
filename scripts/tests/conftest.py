"""Shared set-up for the workflow contracts in this directory.

The readers live in `scripts/`, which is not a package and is not on
`sys.path` when pytest collects these files from the repository root.
pytest imports this module before any test module beside it, so putting
the directory on the path here lets every test import the readers
normally, rather than each carrying its own bootstrap.
"""

from __future__ import annotations

import sys
import json
import dataclasses
import typing as typ
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))


@dataclasses.dataclass
class RunnerExecutionHarness:
    """Bundle fake Cargo, filesystem, and command-log inputs for runner tests."""

    tmp_path: Path
    monkeypatch: pytest.MonkeyPatch
    read_commands: typ.Callable[[], list[list[str]]]

    def fake_environment(self, configuration: typ.Any = None) -> dict[str, str]:
        """Create a fake Cargo executable with the requested outcomes."""
        from test_runner_fixtures import (
            FakeCargoConfiguration,
            fake_cargo_environment,
        )

        selected = configuration or FakeCargoConfiguration()
        return fake_cargo_environment(self.tmp_path, self.monkeypatch, selected)

    def run(self, environment: dict[str, str], cargo_arguments: tuple[str, ...]) -> int:
        """Run the test runner against the harness's fake Cargo executable."""
        import test_runner

        return test_runner.main(
            ["--cargo", environment["FAKE_CARGO"], "--", *cargo_arguments]
        )


@pytest.fixture
def cargo_command_reader(tmp_path: Path) -> typ.Callable[[], list[list[str]]]:
    """Read fake Cargo invocations after each runner execution."""
    command_file = tmp_path / "commands.jsonl"

    def read_commands() -> list[list[str]]:
        if not command_file.exists():
            return []
        return [json.loads(line) for line in command_file.read_text().splitlines()]

    return read_commands


@pytest.fixture
def runner_harness(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    cargo_command_reader: typ.Callable[[], list[list[str]]],
) -> RunnerExecutionHarness:
    """Provide the shared process-order and phase-failure test harness."""
    return RunnerExecutionHarness(tmp_path, monkeypatch, cargo_command_reader)


@pytest.fixture(name="workflow_texts")
def fixture_workflow_texts() -> dict[str, str]:
    """Return every workflow file's text, keyed by file name.

    Imported here rather than at the top of the module: the spelling
    helper's tests share this directory and run without PyYAML, so a
    module-level import of the readers would break their collection.
    """
    from workflow_contracts import load_workflow_documents

    return load_workflow_documents()
