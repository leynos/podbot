"""Check that diagnostic rendering consumes captured external observations."""

from __future__ import annotations

import pathlib
import types

import pytest
import test_runner_diagnostic_collection as collection
import test_runner_diagnostics as diagnostics
from test_runner_diagnostics import StallReportContext, format_stall_report
from test_runner_process_tree import ProcessTreeSnapshot


def test_stall_report_formatting_uses_only_supplied_observations(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Formatting does not read time, toolchain files, procfs, or lock paths."""

    def unexpected_observation(*args: object, **kwargs: object) -> None:
        raise AssertionError("report formatting must not collect external state")

    fail_path = types.SimpleNamespace(
        Path=unexpected_observation, home=unexpected_observation
    )
    for module in (collection, diagnostics):
        monkeypatch.setattr(module, "pathlib", fail_path, raising=False)
        monkeypatch.setattr(
            module,
            "time",
            types.SimpleNamespace(monotonic=unexpected_observation),
            raising=False,
        )
        for name in (
            "_find_toolchain_file",
            "_read_lock_records",
            "_toolchain_file_line",
            "lock_path_identities",
        ):
            monkeypatch.setattr(module, name, unexpected_observation, raising=False)

    context = StallReportContext(
        process_snapshot=ProcessTreeSnapshot(1, True, {}, ()),
        command=("cargo", "test"),
        cwd=pathlib.Path("/workspace"),
        environment={},
        elapsed_seconds=2.0,
        timeout_seconds=30.0,
        reason="timed out",
        process_observed_at=100.0,
        toolchain_file_line="    toolchain file: /workspace/rust-toolchain.toml (channel=stable)",
        lock_records=(),
        lock_path_identities=(),
    )

    report = format_stall_report(context)

    assert report == "\n".join(
        (
            "test runner: timed out after 2.0s (deadline 30.0s)",
            "  command: cargo test",
            "  working directory: /workspace",
            "  toolchain/environment:",
            "    toolchain file: /workspace/rust-toolchain.toml (channel=stable)",
            "  owned process tree:",
            "    no live owned process entries are visible",
            "  known Cargo lock ownership and waiters:",
            "    no locks on mapped Cargo package-cache or target files",
        )
    ), "the renderer must preserve report text while using the captured values"
