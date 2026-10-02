"""Exercise strict Cargo artifact and runtime-profile validation."""

from __future__ import annotations

import dataclasses
import os
import pathlib
import typing as typ

import pytest
import test_runner_cargo
from test_runner_cargo import create_test_runtime_environment
from test_runner_cargo import select_test_executables
from test_runner_context import TestRunnerContext
from test_runner_fixtures import cargo_artifact_message as _artifact
from test_runner_fixtures import executable_file as _executable
from test_runner_fixtures import package_document
from test_runner_models import RunnerError, Target
from test_runner_options import parse_cargo_test_options
from test_runner_plan import create_test_plan
from test_runner_supervisor import ProcessSupervisor


@dataclasses.dataclass(frozen=True)
class CargoArtifactCase:
    """Keep the package, test target, and inherited environment together."""

    package: dict[str, typ.Any]
    target: Target
    executable: pathlib.Path
    binary: pathlib.Path
    context: TestRunnerContext
    inherited_environment: dict[str, str]


@pytest.fixture
def cargo_artifact_case(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
) -> CargoArtifactCase:
    """Create current test and binary paths with inherited Cargo settings."""
    monkeypatch.setattr(
        test_runner_cargo, "_library_path_variable", lambda: "LD_LIBRARY_PATH"
    )
    metadata = package_document(tmp_path)
    package = metadata["packages"][0]
    target = create_test_plan(
        metadata, parse_cargo_test_options(["--test", "compile_contract"])
    ).nested_targets[0]
    executable = _executable(tmp_path / "target/debug/deps/compile_contract")
    binary = tmp_path / "target/debug/podbot"
    binary.parent.mkdir(parents=True, exist_ok=True)
    binary.touch()
    inherited = {
        "CARGO_TARGET_DIR": str(tmp_path / "target"),
        "CARGO_DEBUG_ASSERTIONS": "inherited",
        "LD_LIBRARY_PATH": "/caller/native",
    }
    context = TestRunnerContext(
        ("cargo", "+1.88.0"),
        tmp_path,
        inherited,
        ProcessSupervisor(timeout_seconds=1800),
    )
    return CargoArtifactCase(package, target, executable, binary, context, inherited)


@pytest.mark.parametrize(
    ("field", "value"),
    [
        pytest.param("package_id", "other-id", id="wrong-package"),
        pytest.param("target.name", "other-target", id="wrong-target-name"),
        pytest.param("reason", "build-finished", id="wrong-reason"),
        pytest.param("executable", 42, id="non-string-executable"),
    ],
)
def test_selection_rejects_artifacts_outside_the_requested_contract(
    cargo_artifact_case: CargoArtifactCase,
    field: str,
    value: typ.Any,
) -> None:
    """Reject mismatched identity, reason, and executable types."""
    case = cargo_artifact_case
    artifact = _artifact(case.package["id"], case.target.name, case.executable)
    if field == "target.name":
        artifact["target"]["name"] = value
    else:
        artifact[field] = value

    with pytest.raises(RunnerError, match="found 0"):
        select_test_executables([artifact], (case.target,))


@pytest.mark.parametrize(
    ("test_value", "matches"),
    [
        pytest.param(True, True, id="true-is-test"),
        pytest.param(False, False, id="false-is-not-test"),
        pytest.param(1, False, id="integer-is-not-boolean-true"),
        pytest.param("true", False, id="string-is-not-boolean-true"),
        pytest.param(None, False, id="none-is-not-boolean-true"),
    ],
)
def test_selection_requires_the_exact_boolean_test_flag(
    cargo_artifact_case: CargoArtifactCase, test_value: object, matches: bool
) -> None:
    """Accept only the Boolean true test-profile marker."""
    case = cargo_artifact_case
    artifact = _artifact(case.package["id"], case.target.name, case.executable)
    artifact["profile"]["test"] = test_value

    if matches:
        selected = select_test_executables([artifact], (case.target,))
        assert selected[(case.target.package_id, case.target.name)] == case.executable
    else:
        with pytest.raises(RunnerError, match="found 0"):
            select_test_executables([artifact], (case.target,))


@pytest.mark.parametrize(
    "invalid_field",
    [
        pytest.param("missing-name", id="missing-name"),
        pytest.param("non-string-name", id="non-string-name"),
        pytest.param("missing-executable", id="missing-executable"),
        pytest.param("non-string-executable", id="non-string-executable"),
        pytest.param("wrong-reason", id="non-artifact-message"),
    ],
)
def test_binary_registration_requires_string_name_and_executable(
    cargo_artifact_case: CargoArtifactCase, invalid_field: str
) -> None:
    """Do not create Cargo binary variables from incomplete metadata."""
    case = cargo_artifact_case
    artifact = _artifact(case.package["id"], "podbot", case.binary, kind="bin")
    artifact["profile"]["test"] = False
    if invalid_field == "missing-name":
        artifact["target"].pop("name")
    elif invalid_field == "non-string-name":
        artifact["target"]["name"] = 42
    elif invalid_field == "missing-executable":
        artifact.pop("executable")
    elif invalid_field == "wrong-reason":
        artifact["reason"] = "build-finished"
    else:
        artifact["executable"] = 42

    environment = create_test_runtime_environment(
        case.context, case.package, case.executable, [artifact]
    )

    assert "CARGO_BIN_EXE_podbot" not in environment


@pytest.mark.parametrize(
    ("test_value", "registers"),
    [
        pytest.param(False, True, id="boolean-false-registers-binary"),
        pytest.param(True, False, id="boolean-true-is-test-harness"),
        pytest.param(0, False, id="integer-zero-is-not-boolean-false"),
        pytest.param("false", False, id="string-false-is-not-boolean-false"),
    ],
)
def test_binary_registration_requires_the_exact_boolean_false_flag(
    cargo_artifact_case: CargoArtifactCase, test_value: object, registers: bool
) -> None:
    """Register non-test binaries only for Cargo's Boolean false value."""
    case = cargo_artifact_case
    artifact = _artifact(case.package["id"], "podbot", case.binary, kind="bin")
    artifact["profile"]["test"] = test_value

    environment = create_test_runtime_environment(
        case.context, case.package, case.executable, [artifact]
    )

    assert ("CARGO_BIN_EXE_podbot" in environment) is registers


@pytest.mark.parametrize(
    ("debug_assertions", "expected"),
    [(True, "true"), (False, "false")],
)
def test_runtime_profile_exports_boolean_debug_assertions(
    cargo_artifact_case: CargoArtifactCase,
    debug_assertions: bool,
    expected: str,
) -> None:
    """Convert Cargo's profile flag to its established environment string."""
    case = cargo_artifact_case
    artifact = _artifact(case.package["id"], case.target.name, case.executable)
    artifact["profile"]["debug_assertions"] = debug_assertions

    environment = create_test_runtime_environment(
        case.context, case.package, case.executable, [artifact]
    )

    assert environment["CARGO_DEBUG_ASSERTIONS"] == expected


@pytest.mark.parametrize(
    "profile_kind",
    [
        pytest.param("absent", id="absent-profile"),
        pytest.param("malformed", id="malformed-profile"),
        pytest.param("missing-field", id="missing-debug-assertions"),
    ],
)
def test_runtime_profile_preserves_inherited_value_when_unavailable(
    cargo_artifact_case: CargoArtifactCase, profile_kind: str
) -> None:
    """Missing or malformed profile metadata leaves inherited state intact."""
    case = cargo_artifact_case
    artifact = _artifact(case.package["id"], case.target.name, case.executable)
    if profile_kind == "absent":
        artifact.pop("profile")
    elif profile_kind == "malformed":
        artifact["profile"] = []
    else:
        artifact["profile"].pop("debug_assertions")

    environment = create_test_runtime_environment(
        case.context, case.package, case.executable, [artifact]
    )

    assert environment["CARGO_DEBUG_ASSERTIONS"] == "inherited"


def test_runtime_profile_uses_the_first_matching_artifact(
    cargo_artifact_case: CargoArtifactCase,
) -> None:
    """Keep first-match profile lookup when duplicate messages share a path."""
    case = cargo_artifact_case
    first = _artifact(case.package["id"], case.target.name, case.executable)
    second = _artifact(case.package["id"], case.target.name, case.executable)
    first["profile"]["debug_assertions"] = False
    second["profile"]["debug_assertions"] = True

    environment = create_test_runtime_environment(
        case.context, case.package, case.executable, [first, second]
    )

    assert environment["CARGO_DEBUG_ASSERTIONS"] == "false"


def test_runtime_library_paths_keep_the_cargo_order(
    cargo_artifact_case: CargoArtifactCase, tmp_path: pathlib.Path
) -> None:
    """Prepend Cargo output paths in order and retain inherited paths last."""
    case = cargo_artifact_case
    native = tmp_path / "native"
    native.mkdir()
    library = native / "libsupport.so"
    library.touch()
    test_artifact = _artifact(case.package["id"], case.target.name, case.executable)
    binary_artifact = _artifact(case.package["id"], "podbot", case.binary, kind="bin")
    binary_artifact["profile"]["test"] = False
    build_script = {
        "reason": "build-script-executed",
        "linked_paths": [f"native={native}"],
    }
    library_artifact = {
        "reason": "compiler-artifact",
        "filenames": [str(library)],
    }

    environment = create_test_runtime_environment(
        case.context,
        case.package,
        case.executable,
        [test_artifact, binary_artifact, build_script, library_artifact],
    )

    assert environment["LD_LIBRARY_PATH"].split(os.pathsep) == [
        str(case.executable.parent),
        str(case.executable.parent.parent),
        str(native),
        "/caller/native",
    ]
    assert case.context.environment == case.inherited_environment
