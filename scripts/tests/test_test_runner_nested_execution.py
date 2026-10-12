"""Verify nested Cargo builds and direct test executable invocation."""

from __future__ import annotations

import errno
import json
import os
import pathlib
import typing as typ
from types import SimpleNamespace
from unittest.mock import patch

import pytest
import test_runner
from conftest import RunnerExecutionHarness
import test_runner_nested

from test_runner_context import TestRunnerContext
from test_runner_fixtures import (
    FakeCargoConfiguration,
    cargo_artifact_message,
    fake_cargo_environment as _fake_cargo_environment,
    package_document,
)


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


@pytest.mark.parametrize(
    "case",
    [
        pytest.param(("missing", "missing test executable"), id="missing-artifact"),
        pytest.param(
            ("non-executable", "not executable"), id="non-executable-artifact"
        ),
    ],
)
def test_artifact_errors_redact_known_environment_secrets(
    runner_harness: RunnerExecutionHarness,
    capsys: pytest.CaptureFixture[str],
    tmp_path: pathlib.Path,
    case: tuple[str, str],
) -> None:
    """Artifact-selection diagnostics must not disclose secret path segments."""
    artifact_state, expected_message = case
    if artifact_state == "non-executable" and os.name == "nt":
        pytest.skip("non-executable artifact checks are POSIX-only")
    environment = runner_harness.fake_environment()
    secret = "artifact-path-secret"
    artifact_path = tmp_path / secret / "compile_contract-current"
    if artifact_state == "non-executable":
        artifact_path.parent.mkdir(parents=True)
        artifact_path.write_text("not executable", encoding="utf-8")
        artifact_path.chmod(0o644)
    invocation_environment = dict(os.environ)
    invocation_environment.update(environment)
    invocation_environment["GITHUB_TOKEN"] = secret
    package_id = package_document(tmp_path)["packages"][0]["id"]

    def report_artifact(
        context: TestRunnerContext,
        command: list[str],
        messages: list[dict[str, typ.Any]],
    ) -> int:
        messages.append(
            cargo_artifact_message(package_id, "compile_contract", artifact_path)
        )
        return 0

    with (
        patch.object(test_runner_nested, "_run_json_build", report_artifact),
        patch.object(
            test_runner, "os", SimpleNamespace(environ=invocation_environment)
        ),
    ):
        status = test_runner.main(
            [
                "--cargo",
                environment["FAKE_CARGO"],
                "--",
                "--test",
                "compile_contract",
            ]
        )

    error = capsys.readouterr().err
    assert status == 2, "invalid current artifacts must retain runner failure status"
    assert expected_message in error, "safe artifact failure wording must be preserved"
    assert secret not in error, "known artifact-path secrets must not reach stderr"


def test_tmpdir_errors_redact_invocation_environment_secrets(
    runner_harness: RunnerExecutionHarness,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
    tmp_path: pathlib.Path,
) -> None:
    """Top-level tmpdir failures must redact paths using invocation secrets."""
    secret = "tmpdir-path-secret"
    metadata = package_document(tmp_path)
    metadata["target_directory"] = str(tmp_path / secret / "target")
    environment = runner_harness.fake_environment(
        FakeCargoConfiguration(metadata=metadata)
    )
    invocation_environment = dict(os.environ)
    invocation_environment.update(environment)
    invocation_environment["GITHUB_TOKEN"] = secret
    invocation_environment["FAKE_EXECUTABLE"] = str(
        tmp_path / secret / "target/debug/deps/compile_contract-current"
    )
    monkeypatch.setattr(
        test_runner,
        "os",
        SimpleNamespace(environ=invocation_environment),
    )

    def reject_tmpdir(runtime_environment: dict[str, str]) -> None:
        raise OSError(
            errno.EACCES,
            "Permission denied",
            runtime_environment["CARGO_TARGET_TMPDIR"],
        )

    monkeypatch.setattr(test_runner_nested, "ensure_test_runtime_tmpdir", reject_tmpdir)

    status = test_runner.main(
        [
            "--cargo",
            environment["FAKE_CARGO"],
            "--",
            "--test",
            "compile_contract",
        ]
    )

    error = capsys.readouterr().err
    assert status == 2, "tmpdir preparation errors must retain runner failure status"
    assert "Permission denied" in error, "safe operating-system wording must remain"
    assert secret not in error, "known tmpdir path secrets must not reach stderr"
