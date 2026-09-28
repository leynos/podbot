"""Verify phase ordering and exit propagation with a fake Cargo executable."""

from __future__ import annotations

import json
import pathlib
import textwrap
import typing as typ

import pytest
import test_runner

from test_runner_fixtures import package_document, workspace_with_sibling_package
from test_runner_models import RunnerError


@pytest.fixture
def cargo_command_reader(
    tmp_path: pathlib.Path,
) -> typ.Callable[[], list[list[str]]]:
    """Read fake Cargo invocations after each runner execution."""
    command_file = tmp_path / "commands.jsonl"

    def read_commands() -> list[list[str]]:
        if not command_file.exists():
            return []
        return [json.loads(line) for line in command_file.read_text().splitlines()]

    return read_commands


def test_cargo_build_exits_before_nested_test_process_starts(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    cargo_command_reader: typ.Callable[[], list[list[str]]],
) -> None:
    """The executable observes a completion marker written after Cargo wait."""
    environment = _fake_cargo_environment(tmp_path, monkeypatch)
    monkeypatch.setenv("FAKE_REQUIRE_BUILD_RETURNED", "true")
    real_build = test_runner._run_json_build

    def mark_build_complete(*args: typ.Any, **kwargs: typ.Any) -> int:
        status = real_build(*args, **kwargs)
        pathlib.Path(environment["FAKE_BUILD_RETURNED"]).touch()
        return status

    monkeypatch.setattr(test_runner, "_run_json_build", mark_build_complete)

    status = test_runner.main(
        ["--cargo", environment["FAKE_CARGO"], "--", "--test", "compile_contract"]
    )

    assert status == 0, "a successful no-run build and direct test should pass"
    invocation = json.loads(pathlib.Path(environment["FAKE_TEST_ARGS"]).read_text())
    assert invocation == [], "the test binary should receive no unexpected arguments"
    commands = cargo_command_reader()
    build = next(command for command in commands if "--no-run" in command)
    assert build.index("--test") < build.index("--message-format=json"), (
        "the nested target selector must precede Cargo's JSON output option"
    )
    assert build[build.index("--package") + 1] == "podbot", (
        "the nested target build must be scoped to its owning package"
    )


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


@pytest.mark.parametrize("selector", ["--package=podbot", "-ppodbot"])
def test_attached_package_selectors_are_removed_before_per_package_commands(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    cargo_command_reader: typ.Callable[[], list[list[str]]],
    selector: str,
) -> None:
    """Attached package filters do not duplicate package-scoped phases."""
    environment = _fake_cargo_environment(tmp_path, monkeypatch)

    status = test_runner.main(
        [
            "--cargo",
            environment["FAKE_CARGO"],
            "--",
            "--all-targets",
            selector,
        ]
    )

    commands = cargo_command_reader()
    assert status == 0, "a valid attached package selector must preserve test success"
    assert all(command.count("--package") == 1 for command in commands), (
        "each per-package Cargo phase must receive exactly one package selector"
    )


@pytest.mark.parametrize(
    ("build_exit", "test_exit", "expected"),
    [(19, 0, 19), (0, 17, 17)],
    ids=["build-failure", "test-failure"],
)
def test_build_and_direct_test_failures_propagate(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    build_exit: int,
    test_exit: int,
    expected: int,
) -> None:
    """Neither a Cargo build error nor the direct harness failure is hidden."""
    environment = _fake_cargo_environment(
        tmp_path, monkeypatch, build_exit=build_exit, test_exit=test_exit
    )

    status = test_runner.main(
        ["--cargo", environment["FAKE_CARGO"], "--", "--test", "compile_contract"]
    )

    assert status == expected, "build and test failures must preserve their exit code"
    test_args = pathlib.Path(environment["FAKE_TEST_ARGS"])
    assert test_args.exists() is (build_exit == 0), (
        "the test binary must only run after a successful build"
    )


def test_artifact_selection_error_returns_runner_failure(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    """A malformed current-build artifact fails through the runner contract."""
    environment = _fake_cargo_environment(tmp_path, monkeypatch)

    def reject_artifacts(*args: typ.Any, **kwargs: typ.Any) -> typ.NoReturn:
        raise RunnerError("no current test artifact")

    monkeypatch.setattr(test_runner, "select_test_executables", reject_artifacts)
    status = test_runner.main(
        ["--cargo", environment["FAKE_CARGO"], "--", "--test", "compile_contract"]
    )

    assert status == 2, "artifact selection errors must return the runner error status"
    assert "no current test artifact" in capsys.readouterr().err, (
        "artifact selection failures must be visible on stderr"
    )


def test_ordinary_failure_stops_nested_phase_by_default(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    cargo_command_reader: typ.Callable[[], list[list[str]]],
) -> None:
    """Default fail-fast behaviour stops after ordinary tests fail."""
    environment = _fake_cargo_environment(tmp_path, monkeypatch, ordinary_exit=23)

    status = test_runner.main(
        ["--cargo", environment["FAKE_CARGO"], "--", "--all-targets"]
    )

    commands = cargo_command_reader()
    assert status == 23, "the ordinary test failure must be returned"
    assert not any("--no-run" in command for command in commands), (
        "fail-fast must stop before building nested-Cargo tests"
    )


def test_fail_fast_reports_unstarted_workspace_package(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    cargo_command_reader: typ.Callable[[], list[list[str]]],
    capsys: pytest.CaptureFixture[str],
) -> None:
    """A failure names ordinary package phases that fail-fast skips."""
    metadata = workspace_with_sibling_package(tmp_path)
    environment = _fake_cargo_environment(
        tmp_path, monkeypatch, ordinary_exit=23, metadata=metadata
    )

    status = test_runner.main(
        ["--cargo", environment["FAKE_CARGO"], "--", "--workspace", "--all-targets"]
    )

    commands = cargo_command_reader()
    output = capsys.readouterr().out
    assert status == 23, "the first ordinary failure must be returned"
    assert "remaining packages: sibling" in output, (
        "fail-fast diagnostics must identify the skipped sibling package"
    )
    assert not any("sibling" in command for command in commands), (
        "fail-fast must not start later package phases"
    )


def test_no_fail_fast_continues_after_ordinary_failure(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    cargo_command_reader: typ.Callable[[], list[list[str]]],
) -> None:
    """`--no-fail-fast` still runs the isolated compile-contract target."""
    environment = _fake_cargo_environment(tmp_path, monkeypatch, ordinary_exit=23)

    status = test_runner.main(
        ["--cargo", environment["FAKE_CARGO"], "--", "--all-targets", "--no-fail-fast"]
    )

    commands = cargo_command_reader()
    assert status == 23, "the ordinary test failure must remain the final status"
    assert any("--no-run" in command for command in commands), (
        "--no-fail-fast must continue to isolated nested-Cargo tests"
    )


def _fake_cargo_environment(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    *,
    build_exit: int = 0,
    test_exit: int = 0,
    ordinary_exit: int = 0,
    metadata: dict[str, typ.Any] | None = None,
) -> dict[str, str]:
    """Write fake Cargo and a test executable controlled by environment."""
    metadata_path = tmp_path / "metadata.json"
    metadata_path.write_text(
        json.dumps(metadata or package_document(tmp_path)), encoding="utf-8"
    )
    cargo_path = tmp_path / "fake-cargo.py"
    cargo_path.write_text(
        textwrap.dedent(
            """\
            #!/usr/bin/env python3
            import json
            import os
            import pathlib
            import sys
            import time

            args = sys.argv[1:]
            if args[0] == "metadata":
                print(pathlib.Path(os.environ["FAKE_METADATA"]).read_text())
                raise SystemExit(0)
            with pathlib.Path(os.environ["FAKE_COMMANDS"]).open("a") as commands:
                commands.write(json.dumps(args) + "\\n")
            if "--no-run" in args:
                status = int(os.environ["FAKE_BUILD_EXIT"])
                if status:
                    raise SystemExit(status)
                executable = pathlib.Path(os.environ["FAKE_EXECUTABLE"])
                executable.parent.mkdir(parents=True, exist_ok=True)
                executable.write_text(
                    "#!/usr/bin/env python3\\n"
                    "import json, os, pathlib, sys\\n"
                    "if os.environ.get('FAKE_REQUIRE_BUILD_RETURNED') == 'true' and not pathlib.Path(os.environ['FAKE_BUILD_RETURNED']).exists():\\n"
                    "    raise SystemExit(41)\\n"
                    "pathlib.Path(os.environ['FAKE_TEST_ARGS']).write_text(json.dumps(sys.argv[1:]))\\n"
                    "raise SystemExit(int(os.environ['FAKE_TEST_EXIT']))\\n"
                )
                executable.chmod(0o755)
                message = {
                    "reason": "compiler-artifact",
                    "package_id": "path+file:///workspace/podbot#podbot@0.1.0",
                    "target": {"name": "compile_contract", "kind": ["test"]},
                    "profile": {"test": True, "debug_assertions": True},
                    "executable": str(executable),
                    "filenames": [str(executable)],
                }
                print(json.dumps(message), flush=True)
                time.sleep(0.05)
                raise SystemExit(0)
            if "--test" in args and "cli_feature_gating" in args:
                raise SystemExit(int(os.environ["FAKE_ORDINARY_EXIT"]))
            raise SystemExit(0)
            """
        ),
        encoding="utf-8",
    )
    cargo_path.chmod(0o755)
    values = {
        "FAKE_METADATA": str(metadata_path),
        "FAKE_CARGO": str(cargo_path),
        "FAKE_COMMANDS": str(tmp_path / "commands.jsonl"),
        "FAKE_EXECUTABLE": str(tmp_path / "target/debug/deps/compile_contract-current"),
        "FAKE_BUILD_RETURNED": str(tmp_path / "build-returned"),
        "FAKE_TEST_ARGS": str(tmp_path / "test-args.json"),
        "FAKE_BUILD_EXIT": str(build_exit),
        "FAKE_TEST_EXIT": str(test_exit),
        "FAKE_ORDINARY_EXIT": str(ordinary_exit),
    }
    for name, value in values.items():
        monkeypatch.setenv(name, value)
    return values
