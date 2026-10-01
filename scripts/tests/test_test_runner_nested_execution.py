"""Verify nested Cargo builds and direct test executable invocation."""

from __future__ import annotations

import json
import pathlib
import typing as typ

import pytest
import test_runner
from conftest import RunnerExecutionHarness
import test_runner_nested

from test_runner_fixtures import fake_cargo_environment as _fake_cargo_environment


@pytest.mark.parametrize(
    ("selection", "target_name"),
    [
        (("--test", "compile_contract"), "compile_contract"),
        (
            ("--no-default-features", "--test", "cli_feature_gating"),
            "cli_feature_gating",
        ),
    ],
    ids=["compile-contract", "no-default-cli-boundary"],
)
def test_cargo_build_exits_before_nested_test_process_starts(
    runner_harness: RunnerExecutionHarness,
    selection: tuple[str, ...],
    target_name: str,
) -> None:
    """Each trybuild executable starts after its current Cargo build exits."""
    environment = runner_harness.fake_environment()
    runner_harness.monkeypatch.setenv("FAKE_REQUIRE_BUILD_RETURNED", "true")
    real_build = test_runner_nested._run_json_build

    def mark_build_complete(*args: typ.Any, **kwargs: typ.Any) -> int:
        status = real_build(*args, **kwargs)
        pathlib.Path(environment["FAKE_BUILD_RETURNED"]).touch()
        return status

    runner_harness.monkeypatch.setattr(
        test_runner_nested, "_run_json_build", mark_build_complete
    )

    status = test_runner.main(["--cargo", environment["FAKE_CARGO"], "--", *selection])

    assert status == 0, "a successful no-run build and direct test should pass"
    invocation = json.loads(pathlib.Path(environment["FAKE_TEST_ARGS"]).read_text())
    assert invocation == [], "the test binary should receive no unexpected arguments"
    commands = runner_harness.read_commands()
    build = next(command for command in commands if "--no-run" in command)
    assert build.index("--test") < build.index(
        "--message-format=json-render-diagnostics"
    ), "the nested target selector must precede Cargo's JSON output option"
    assert build[build.index("--package") + 1] == "podbot", (
        "the nested target build must be scoped to its owning package"
    )
    assert build[build.index("--test") + 1] == target_name, (
        "the isolated Cargo build must select the requested trybuild target"
    )
    assert not any(
        "--test" in command
        and command[command.index("--test") + 1] == target_name
        and "--no-run" not in command
        for command in commands
    ), "no trybuild target may execute inside an outer Cargo test process"


def test_filters_and_harness_arguments_reach_the_current_artifact(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Cargo's positional test filter and post-separator flags reach libtest."""
    environment = _fake_cargo_environment(tmp_path, monkeypatch)

    status = test_runner.main(
        [
            "--cargo",
            environment["FAKE_CARGO"],
            "--",
            "--test",
            "compile_contract",
            "stable_exec_context_signatures_compile",
            "--",
            "--exact",
            "--nocapture",
        ]
    )

    assert status == 0, "the direct compile-contract test should succeed"
    invocation = json.loads(pathlib.Path(environment["FAKE_TEST_ARGS"]).read_text())
    assert invocation == [
        "stable_exec_context_signatures_compile",
        "--exact",
        "--nocapture",
    ], "Cargo filters and harness arguments must reach the direct test binary"
