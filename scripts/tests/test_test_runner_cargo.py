"""Specify artifact selection and reconstructed Cargo runtime environment."""

from __future__ import annotations

import os
import pathlib
import typing as typ

import pytest
from test_runner_cargo import create_test_runtime_environment, select_test_executables
from test_runner_models import RunnerError
from test_runner_options import parse_cargo_test_options
from test_runner_plan import create_test_plan

from test_runner_fixtures import package_document


def test_artifact_selection_matches_package_target_and_test_profile(
    tmp_path: pathlib.Path,
) -> None:
    """Same-named artifacts and non-test executables cannot satisfy selection."""
    metadata = package_document(tmp_path)
    target = create_test_plan(
        metadata, parse_cargo_test_options(["--test", "compile_contract"])
    ).nested_targets[0]
    executable = _executable(tmp_path / "target/debug/deps/compile_contract-current")
    other_package = _artifact("other-id", "compile_contract", executable)
    wrong_kind = _artifact(target.package_id, "compile_contract", executable)
    wrong_kind["target"]["kind"] = ["bin"]
    right = _artifact(target.package_id, "compile_contract", executable)

    selected = select_test_executables([other_package, wrong_kind, right], (target,))

    assert selected[(target.package_id, "compile_contract")] == executable


def test_artifact_selection_rejects_missing_and_duplicate_current_outputs(
    tmp_path: pathlib.Path,
) -> None:
    """A stale file on disk cannot replace a current Cargo artifact message."""
    metadata = package_document(tmp_path)
    target = create_test_plan(
        metadata, parse_cargo_test_options(["--test", "compile_contract"])
    ).nested_targets[0]
    executable = _executable(tmp_path / "target/debug/deps/compile_contract")
    right = _artifact(target.package_id, target.name, executable)

    with pytest.raises(RunnerError, match="found 0"):
        select_test_executables([], (target,))
    with pytest.raises(RunnerError, match="found 2"):
        select_test_executables([right, right], (target,))


def test_runtime_environment_restores_cargo_values_and_library_paths(
    tmp_path: pathlib.Path,
) -> None:
    """Direct execution receives Cargo metadata, binaries, and link paths."""
    metadata = package_document(tmp_path)
    package = metadata["packages"][0]
    executable = _executable(tmp_path / "target/debug/deps/compile_contract")
    binary = tmp_path / "target/debug/podbot"
    binary.touch()
    linked = tmp_path / "native"
    linked.mkdir()
    dylib = linked / "libsupport.so"
    dylib.touch()
    messages = [
        _artifact(package["id"], "compile_contract", executable),
        _artifact(package["id"], "podbot", binary, kind="bin", test=False),
        {"reason": "build-script-executed", "linked_paths": [f"native={linked}"]},
        {
            "reason": "compiler-artifact",
            "package_id": package["id"],
            "target": {"name": "support", "kind": ["lib"]},
            "filenames": [str(dylib)],
            "profile": {"test": False},
        },
    ]
    inherited = {"LD_LIBRARY_PATH": "/caller/native"}

    environment = create_test_runtime_environment(
        inherited,
        package,
        executable,
        messages,
        target_directory=tmp_path / "target",
        cargo_command=("cargo",),
    )

    assert environment["CARGO_MANIFEST_DIR"] == str(tmp_path), (
        "direct test execution must retain Cargo's package manifest directory"
    )
    assert environment["CARGO_PKG_NAME"] == "podbot", (
        "direct test execution must retain Cargo's package name"
    )
    assert environment["CARGO_PKG_VERSION_PATCH"] == "0", (
        "Cargo's patch-version variable must be restored"
    )
    assert environment["CARGO_BIN_EXE_podbot"] == str(binary), (
        "Cargo's binary executable variable must point to the current build"
    )
    assert environment["CARGO_TARGET_DIR"] == str(tmp_path / "target"), (
        "nested Cargo must use the outer build's target directory"
    )
    assert environment["CARGO_TARGET_TMPDIR"] == str(tmp_path / "target/debug/tmp"), (
        "direct tests must receive Cargo's target temporary directory"
    )
    assert environment["LD_LIBRARY_PATH"].split(os.pathsep)[-1] == "/caller/native", (
        "the inherited native-library path must remain available"
    )
    assert str(linked) in environment["LD_LIBRARY_PATH"], (
        "native paths emitted by build scripts must be available"
    )
    assert inherited["LD_LIBRARY_PATH"] == "/caller/native", (
        "runtime reconstruction must not mutate the caller's environment"
    )


def _artifact(
    package_id: str,
    name: str,
    executable: pathlib.Path,
    *,
    kind: str = "test",
    test: bool = True,
) -> dict[str, typ.Any]:
    """Return a Cargo compiler-artifact message for one executable."""
    return {
        "reason": "compiler-artifact",
        "package_id": package_id,
        "target": {"name": name, "kind": [kind]},
        "profile": {"test": test, "debug_assertions": True},
        "executable": str(executable),
        "filenames": [str(executable)],
    }


def _executable(path: pathlib.Path) -> pathlib.Path:
    """Create an executable file usable as a fake test artifact."""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
    path.chmod(0o755)
    return path
