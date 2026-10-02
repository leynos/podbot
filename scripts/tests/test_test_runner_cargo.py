"""Specify artifact selection and reconstructed Cargo runtime environment."""

from __future__ import annotations

import dataclasses
import os
import pathlib
import typing as typ

import pytest
import test_runner_cargo
from test_runner_cargo import (
    _resolve_cargo_executable,
    _package_version_environment,
    create_test_runtime_environment,
    select_test_executables,
)
from test_runner_models import RunnerError
from test_runner_options import parse_cargo_test_options
from test_runner_plan import create_test_plan
from test_runner_context import TestRunnerContext
from test_runner_supervisor import CommandRequest, ProcessSupervisor

from test_runner_fixtures import cargo_artifact_message as _artifact
from test_runner_fixtures import executable_file as _executable
from test_runner_fixtures import package_document


@dataclasses.dataclass(frozen=True)
class RuntimeEnvironmentCase:
    """Hold expected paths and environments for one direct-test fixture."""

    environment: dict[str, str]
    inherited_environment: dict[str, str]
    package_directory: pathlib.Path
    binary: pathlib.Path
    linked_directory: pathlib.Path
    target_directory: pathlib.Path


@pytest.fixture
def runtime_environment_case(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
) -> RuntimeEnvironmentCase:
    """Build one Cargo environment fixture with package and native outputs."""
    monkeypatch.setattr(
        test_runner_cargo, "_library_path_variable", lambda: "LD_LIBRARY_PATH"
    )
    metadata = package_document(tmp_path)
    package = metadata["packages"][0]
    executable = _executable(tmp_path / "target/debug/deps/compile_contract")
    binary = tmp_path / "target/debug/podbot"
    binary.touch()
    linked = tmp_path / "native"
    linked.mkdir()
    (linked / "libsupport.so").touch()
    inherited = {
        "LD_LIBRARY_PATH": "/caller/native",
        "CARGO_TARGET_DIR": str(tmp_path / "target"),
    }
    context = TestRunnerContext(
        ("cargo", "+1.88.0"),
        tmp_path,
        inherited,
        ProcessSupervisor(timeout_seconds=1800),
    )
    messages = [
        _artifact(package["id"], "compile_contract", executable),
        _artifact(package["id"], "podbot", binary, kind="bin"),
        {"reason": "build-script-executed", "linked_paths": [f"native={linked}"]},
        {
            "reason": "compiler-artifact",
            "package_id": package["id"],
            "target": {"name": "support", "kind": ["lib"]},
            "filenames": [str(linked / "libsupport.so")],
            "profile": {"test": False},
        },
    ]
    environment = create_test_runtime_environment(
        context, package, executable, messages
    )
    return RuntimeEnvironmentCase(
        environment,
        inherited,
        tmp_path,
        binary,
        linked,
        tmp_path / "target",
    )


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

    assert selected[(target.package_id, "compile_contract")] == executable, (
        "artifact selection must match the package, target, and test profile"
    )


@pytest.mark.parametrize(
    ("field", "value"),
    [
        pytest.param("target", None, id="missing-target-object"),
        pytest.param("target", [], id="non-object-target"),
        pytest.param(
            "target",
            {"name": "compile_contract", "kind": "test"},
            id="non-list-target-kinds",
        ),
        pytest.param("profile", None, id="missing-profile-object"),
        pytest.param("profile", [], id="non-object-profile"),
    ],
)
def test_artifact_selection_ignores_malformed_target_and_profile_values(
    tmp_path: pathlib.Path, field: str, value: typ.Any
) -> None:
    """Malformed Cargo mappings cannot crash current-build selection."""
    metadata = package_document(tmp_path)
    target = create_test_plan(
        metadata, parse_cargo_test_options(["--test", "compile_contract"])
    ).nested_targets[0]
    malformed = _artifact(
        target.package_id,
        target.name,
        _executable(tmp_path / "target/debug/deps/compile_contract"),
    )
    malformed[field] = value

    with pytest.raises(RunnerError, match="found 0"):
        select_test_executables([malformed], (target,))


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


def test_runtime_environment_restores_cargo_package_metadata(
    runtime_environment_case: RuntimeEnvironmentCase,
) -> None:
    """Direct execution receives the package values Cargo normally sets."""
    case = runtime_environment_case
    assert case.environment["CARGO_MANIFEST_DIR"] == str(case.package_directory), (
        "direct tests must retain Cargo's package manifest directory"
    )
    assert case.environment["CARGO_MANIFEST_PATH"] == str(
        case.package_directory / "Cargo.toml"
    ), "direct tests must retain Cargo's package manifest path"
    assert case.environment["CARGO_PKG_NAME"] == "podbot", (
        "direct test execution must retain Cargo's package name"
    )
    assert case.environment["CARGO_PKG_VERSION_PATCH"] == "0", (
        "Cargo's patch-version variable must be restored"
    )


def test_runtime_environment_restores_target_and_toolchain(
    runtime_environment_case: RuntimeEnvironmentCase,
) -> None:
    """Direct tests keep the outer target directory and selected toolchain."""
    case = runtime_environment_case
    assert case.environment["CARGO_TARGET_DIR"] == str(case.target_directory), (
        "nested Cargo must use the outer build's target directory"
    )
    target_tmpdir = pathlib.Path(case.environment["CARGO_TARGET_TMPDIR"])
    assert target_tmpdir == case.target_directory / "tmp", (
        "direct tests must receive Cargo's target temporary directory"
    )
    assert target_tmpdir.is_dir(), "Cargo's target temporary directory must exist"
    assert case.environment["CARGO"] == "cargo", (
        "bare Cargo names must continue to resolve through PATH"
    )
    assert case.environment["RUSTUP_TOOLCHAIN"] == "1.88.0", (
        "nested Cargo must retain the runner's explicit toolchain selection"
    )


def test_runtime_environment_preserves_binary_and_native_paths(
    runtime_environment_case: RuntimeEnvironmentCase,
) -> None:
    """Direct tests retain Cargo binary paths and inherited native libraries."""
    case = runtime_environment_case
    library_path = case.environment["LD_LIBRARY_PATH"]
    assert case.environment["CARGO_BIN_EXE_podbot"] == str(case.binary), (
        "Cargo's binary executable variable must point to the current build"
    )
    assert str(case.linked_directory) in library_path, (
        "native paths emitted by build scripts must be available"
    )
    assert library_path.split(os.pathsep)[-1] == "/caller/native", (
        "the inherited native-library path must remain available"
    )
    assert case.inherited_environment["LD_LIBRARY_PATH"] == "/caller/native", (
        "runtime reconstruction must not mutate the caller's environment"
    )


def test_runtime_environment_ignores_malformed_artifact_mappings(
    runtime_environment_case: RuntimeEnvironmentCase,
) -> None:
    """Malformed target and profile objects do not break environment setup."""
    case = runtime_environment_case
    package = package_document(case.package_directory)["packages"][0]
    test_executable = case.target_directory / "debug/deps/compile_contract"
    context = TestRunnerContext(
        ("cargo", "+1.88.0"),
        case.package_directory,
        case.inherited_environment,
        ProcessSupervisor(timeout_seconds=1800),
    )
    messages = [
        {
            "reason": "compiler-artifact",
            "package_id": package["id"],
            "target": None,
            "profile": None,
            "executable": str(test_executable),
        },
        {
            "reason": "compiler-artifact",
            "package_id": package["id"],
            "target": {"name": "podbot", "kind": "bin"},
            "profile": [],
            "executable": str(case.binary),
        },
    ]

    environment = create_test_runtime_environment(
        context, package, test_executable, messages
    )

    assert "CARGO_BIN_EXE_podbot" not in environment, (
        "malformed target kinds must not create Cargo binary variables"
    )
    assert "CARGO_DEBUG_ASSERTIONS" not in environment, (
        "malformed profile objects must not create profile variables"
    )


@pytest.mark.parametrize(
    ("version", "expected_components"),
    [
        ("1.2.3-rc.1", ("1", "2", "3", "rc.1")),
        ("1.2-alpha", ("1", "2", "0", "alpha")),
        ("2.5.7+build.4", ("2", "5", "7", "")),
    ],
)
def test_package_version_environment_splits_semver_components(
    version: str,
    expected_components: tuple[str, str, str, str],
) -> None:
    """Cargo version variables keep pre-release and build text out of patch."""
    values = _package_version_environment({"version": version})

    assert values["CARGO_PKG_VERSION"] == version, (
        "the full package version must retain pre-release and build metadata"
    )
    actual_components = (
        values["CARGO_PKG_VERSION_MAJOR"],
        values["CARGO_PKG_VERSION_MINOR"],
        values["CARGO_PKG_VERSION_PATCH"],
        values["CARGO_PKG_VERSION_PRE"],
    )
    assert actual_components == expected_components, (
        "Cargo's version variables must separate core and pre-release values"
    )


@pytest.mark.parametrize(
    ("platform", "expected"),
    [
        ("win32", "PATH"),
        ("darwin", "DYLD_FALLBACK_LIBRARY_PATH"),
        ("aix", "LIBPATH"),
        ("linux", "LD_LIBRARY_PATH"),
    ],
)
def test_library_path_variable_matches_cargo_platform_name(
    monkeypatch: pytest.MonkeyPatch, platform: str, expected: str
) -> None:
    """Direct harnesses use Cargo's platform-specific dynamic-library path."""
    monkeypatch.setattr(test_runner_cargo.sys, "platform", platform)

    actual = test_runner_cargo._library_path_variable()

    assert actual == expected, "the loader path variable must match Cargo"


def test_relative_cargo_executable_path_is_anchored_before_chdir(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Explicit relative Cargo paths remain valid from the workspace root."""
    monkeypatch.chdir(tmp_path)
    cargo = tmp_path / "tools" / "cargo"
    cargo.parent.mkdir()
    cargo.write_text("cargo wrapper", encoding="utf-8")

    resolved = _resolve_cargo_executable(("./tools/cargo",))

    assert resolved == str(cargo), (
        "relative executable paths must become absolute before runtime cwd changes"
    )


def test_relative_manifest_is_anchored_to_caller_directory(
    tmp_path: pathlib.Path,
) -> None:
    """Metadata commands retain caller-relative manifest and target paths."""
    caller_directory = tmp_path / "caller"
    workspace_directory = tmp_path / "workspace"
    caller_directory.mkdir()
    workspace_directory.mkdir()
    expected_manifest = workspace_directory / "Cargo.toml"
    expected_target_directory = caller_directory / "build"
    observed: dict[str, typ.Any] = {}

    class FakeSupervisor:
        def run_capture(
            self,
            request: CommandRequest,
        ) -> tuple[int, str, str]:
            observed["command"] = request.command
            observed["cwd"] = request.cwd
            observed["purpose"] = request.purpose
            return 0, "{}", ""

    options = parse_cargo_test_options(
        ["--manifest-path", "../workspace/Cargo.toml", "--target-dir", "build"],
        cwd=caller_directory,
    )

    context = TestRunnerContext(("cargo",), workspace_directory, {}, FakeSupervisor())
    test_runner_cargo.load_cargo_metadata(context, options)

    assert observed["cwd"] == workspace_directory, (
        "metadata may run from the selected workspace root"
    )
    assert observed["purpose"] == "Cargo metadata", (
        "metadata subprocesses must be identified to the shared supervisor"
    )
    assert str(expected_manifest) in observed["command"], (
        "relative manifest paths must be anchored before changing directories"
    )
    assert options.manifest_path == expected_manifest, (
        "metadata package discovery must use the same absolute manifest"
    )
    assert str(expected_target_directory) in options.common, (
        "relative target paths must preserve their caller-relative meaning"
    )
