"""Load Cargo metadata and restore the environment for direct test execution.

This module selects test executables from the current Cargo JSON build and
recreates the package and dynamic-library variables Cargo normally supplies.
The JSON parser provides a small, stable entry point for consuming that stream.

Examples
--------
>>> parse_cargo_json_message('{"reason":"build-finished","success":true}')
{'reason': 'build-finished', 'success': True}
"""

from __future__ import annotations

import json
import os
import pathlib
import sys
import typing as typ

from test_runner_models import CargoTestOptions, RunnerError, Target
from test_runner_context import TestRunnerContext
from test_runner_supervisor import CommandRequest


def load_cargo_metadata(
    context: TestRunnerContext,
    options: CargoTestOptions,
) -> dict[str, typ.Any]:
    """Return workspace metadata without downloading dependency metadata.

    Examples
    --------
    The runner passes a versioned JSON request so package and target selection
    does not depend on Cargo's human-readable output.
    """
    request = _metadata_request(context, options)
    status, stdout, stderr = context.supervisor.run_capture(request)
    if status != 0:
        _raise_metadata_failure(status, stderr, context.supervisor.terminal_status)
    return _decode_metadata(stdout)


def _metadata_request(
    context: TestRunnerContext, options: CargoTestOptions
) -> CommandRequest:
    """Build a metadata request with the workspace and offline selectors."""
    command = [*context.cargo_command, "metadata", "--no-deps", "--format-version", "1"]
    if options.manifest_path is not None:
        command.extend(["--manifest-path", str(options.manifest_path)])
    command.extend(
        flag
        for flag in ("--offline", "--locked", "--frozen")
        if flag in options.common and flag not in command
    )
    return CommandRequest(command, context.cwd, context.environment, "Cargo metadata")


def _raise_metadata_failure(
    status: int, stderr: str, terminal_status: int | None
) -> typ.NoReturn:
    """Preserve supervisor interruption or Cargo's ordinary failure status."""
    if stderr:
        sys.stderr.write(stderr)
    if terminal_status is not None:
        raise RunnerCommandFailure(terminal_status)
    raise RunnerError(f"cargo metadata exited with status {status}")


def _decode_metadata(stdout: str) -> dict[str, typ.Any]:
    """Decode Cargo's versioned workspace metadata document."""
    try:
        metadata = json.loads(stdout)
    except json.JSONDecodeError as exc:
        raise RunnerError("cargo metadata returned invalid JSON") from exc
    if not isinstance(metadata, dict):
        raise RunnerError("cargo metadata returned a non-object document")
    return metadata


class RunnerCommandFailure(RunnerError):
    """Carry a timeout or interruption status through metadata discovery."""

    def __init__(self, status: int) -> None:
        super().__init__(f"test runner stopped with status {status}")
        self.status = status


def parse_cargo_json_message(line: str) -> dict[str, typ.Any] | None:
    """Decode one Cargo JSON message, returning None for ordinary tool output.

    Examples
    --------
    >>> parse_cargo_json_message('{"reason":"build-finished","success":true}')
    {'reason': 'build-finished', 'success': True}
    >>> parse_cargo_json_message('compiler says hello') is None
    True
    """
    try:
        value = json.loads(line)
    except json.JSONDecodeError:
        return None
    return value if isinstance(value, dict) else None


def select_test_executables(
    messages: list[dict[str, typ.Any]], expected_targets: tuple[Target, ...]
) -> dict[tuple[str, str], pathlib.Path]:
    """Choose each nested test executable from this build's artifact messages.

    Examples
    --------
    An artifact for a same-named target in another package cannot satisfy a
    registered package and target pair.
    """
    selected: dict[tuple[str, str], pathlib.Path] = {}
    for expected in expected_targets:
        key = (expected.package_id, expected.name)
        candidates = tuple(
            message for message in messages if _is_test_artifact(message, expected)
        )
        selected[key] = _current_executable(expected, candidates)
    return selected


def _is_test_artifact(message: dict[str, typ.Any], expected: Target) -> bool:
    """Match one JSON message to the requested package's test executable."""
    artifact_metadata = _artifact_target_and_profile(message)
    if artifact_metadata is None:
        return False
    target, profile = artifact_metadata
    kinds = target.get("kind")
    return (
        message.get("reason") == "compiler-artifact"
        and message.get("package_id") == expected.package_id
        and target.get("name") == expected.name
        and isinstance(kinds, list)
        and "test" in kinds
        and profile.get("test") is True
        and isinstance(message.get("executable"), str)
    )


def _current_executable(
    expected: Target, candidates: tuple[dict[str, typ.Any], ...]
) -> pathlib.Path:
    """Require one executable from this build and verify it can run."""
    if len(candidates) != 1:
        raise RunnerError(
            f"expected one current compiler-artifact for "
            f"{expected.package_name}:{expected.name}, found {len(candidates)}"
        )
    executable = pathlib.Path(candidates[0]["executable"])
    if not executable.is_file():
        raise RunnerError(
            f"Cargo reported a missing test executable for "
            f"{expected.package_name}:{expected.name}: {executable}"
        )
    if os.name != "nt" and not os.access(executable, os.X_OK):
        raise RunnerError(f"Cargo test executable is not executable: {executable}")
    return executable


def create_test_runtime_environment(
    context: TestRunnerContext,
    package: dict[str, typ.Any],
    executable: pathlib.Path,
    messages: list[dict[str, typ.Any]],
) -> dict[str, str]:
    """Add the Cargo package, target, binary, and dynamic-library environment.

    Examples
    --------
    The inherited library path remains present after Cargo's target paths are
    prepended, so custom native libraries remain discoverable.
    """
    environment = context.environment.copy()
    _set_package_environment(environment, package)
    target_directory = pathlib.Path(environment["CARGO_TARGET_DIR"])
    target_temporary_directory = target_directory / "tmp"
    target_temporary_directory.mkdir(parents=True, exist_ok=True)
    environment["CARGO_TARGET_TMPDIR"] = str(target_temporary_directory)
    environment["CARGO"] = _resolve_cargo_executable(context.cargo_command)
    if len(context.cargo_command) > 1 and context.cargo_command[1].startswith("+"):
        environment["RUSTUP_TOOLCHAIN"] = context.cargo_command[1][1:]
    for message in messages:
        if message.get("reason") == "compiler-artifact":
            _record_binary_executable(environment, package, message)
    _set_executable_profile_environment(environment, executable, messages)
    _set_dynamic_library_environment(environment, executable, messages)
    return environment


def _set_package_environment(
    environment: dict[str, str], package: dict[str, typ.Any]
) -> None:
    """Restore Cargo's package metadata environment variables."""
    manifest_dir = pathlib.Path(str(package["manifest_path"])).parent
    environment["CARGO_MANIFEST_DIR"] = str(manifest_dir)
    environment["CARGO_MANIFEST_PATH"] = str(package["manifest_path"])
    values = {
        "CARGO_PKG_NAME": package.get("name", ""),
        "CARGO_PKG_AUTHORS": ":".join(package.get("authors", [])),
        "CARGO_PKG_DESCRIPTION": package.get("description") or "",
        "CARGO_PKG_REPOSITORY": package.get("repository") or "",
        "CARGO_PKG_LICENSE": package.get("license") or "",
        "CARGO_PKG_LICENSE_FILE": package.get("license_file") or "",
        "CARGO_PKG_HOMEPAGE": package.get("homepage") or "",
        "CARGO_PKG_RUST_VERSION": package.get("rust_version") or "",
        "CARGO_PKG_CATEGORIES": ":".join(package.get("categories", [])),
        "CARGO_PKG_KEYWORDS": ":".join(package.get("keywords", [])),
    }
    values.update(_package_version_environment(package))
    environment.update({name: str(value) for name, value in values.items()})


def _package_version_environment(package: dict[str, typ.Any]) -> dict[str, str]:
    """Split Cargo package version metadata into core and pre-release values."""
    version = str(package.get("version") or "0.0.0")
    without_build_metadata = version.partition("+")[0]
    core_version, separator, pre_release = without_build_metadata.partition("-")
    version_parts = (core_version.split(".") + ["0", "0", "0"])[:3]
    return {
        "CARGO_PKG_VERSION": version,
        "CARGO_PKG_VERSION_MAJOR": version_parts[0],
        "CARGO_PKG_VERSION_MINOR": version_parts[1],
        "CARGO_PKG_VERSION_PATCH": version_parts[2],
        "CARGO_PKG_VERSION_PRE": pre_release if separator else "",
    }


def _record_binary_executable(
    environment: dict[str, str],
    package: dict[str, typ.Any],
    message: dict[str, typ.Any],
) -> None:
    """Restore a Cargo binary path for its owning package when available."""
    artifact_metadata = _artifact_target_and_profile(message)
    if artifact_metadata is None:
        return
    target, profile = artifact_metadata
    kinds = target.get("kind")
    if message.get("package_id") != package.get("id"):
        return
    if not isinstance(kinds, list) or "bin" not in kinds:
        return
    if profile.get("test") is not False:
        return
    executable = message.get("executable")
    name = target.get("name")
    if isinstance(executable, str) and isinstance(name, str):
        environment[f"CARGO_BIN_EXE_{name}"] = executable


def _artifact_target_and_profile(
    message: dict[str, typ.Any],
) -> tuple[dict[str, typ.Any], dict[str, typ.Any]] | None:
    """Return Cargo target and profile mappings only when both are objects."""
    target = message.get("target")
    profile = message.get("profile")
    if not isinstance(target, dict) or not isinstance(profile, dict):
        return None
    return target, profile


def _set_executable_profile_environment(
    environment: dict[str, str],
    executable: pathlib.Path,
    messages: list[dict[str, typ.Any]],
) -> None:
    """Restore debug-assertion metadata from the first matching artifact."""
    profile_message = next(
        (
            message
            for message in messages
            if message.get("reason") == "compiler-artifact"
            and message.get("executable") == str(executable)
        ),
        {},
    )
    profile = profile_message.get("profile")
    if not isinstance(profile, dict):
        profile = {}
    if "debug_assertions" in profile:
        environment["CARGO_DEBUG_ASSERTIONS"] = str(profile["debug_assertions"]).lower()


def _set_dynamic_library_environment(
    environment: dict[str, str],
    executable: pathlib.Path,
    messages: list[dict[str, typ.Any]],
) -> None:
    """Reconstruct Cargo's runtime library search paths for the host platform."""
    variable = _library_path_variable()
    inherited_paths = environment.get(variable, "").split(os.pathsep)
    search_paths = [
        *_profile_library_paths(executable),
        *_build_script_library_paths(messages),
        *_artifact_library_paths(messages),
    ]
    combined = _unique_paths([*map(str, search_paths), *inherited_paths])
    if combined:
        environment[variable] = os.pathsep.join(combined)


def _profile_library_paths(executable: pathlib.Path) -> tuple[pathlib.Path, ...]:
    """Return the Cargo profile and dependency directories for one artifact."""
    profile_directory = executable.parent.parent
    return profile_directory / "deps", profile_directory


def _build_script_library_paths(
    messages: list[dict[str, typ.Any]],
) -> tuple[pathlib.Path, ...]:
    """Collect existing native-library paths emitted by build scripts."""
    return tuple(
        path
        for message in messages
        if message.get("reason") == "build-script-executed"
        for raw_path in message.get("linked_paths", [])
        if (path := _existing_linked_path(raw_path)) is not None
    )


def _existing_linked_path(raw_path: typ.Any) -> pathlib.Path | None:
    """Resolve Cargo's `native=` metadata and retain directories only."""
    _, separator, path_text = str(raw_path).partition("=")
    candidate = pathlib.Path(path_text if separator else str(raw_path))
    return candidate if candidate.is_dir() else None


def _artifact_library_paths(
    messages: list[dict[str, typ.Any]],
) -> tuple[pathlib.Path, ...]:
    """Collect parent directories for dynamic libraries in Cargo artifacts."""
    return tuple(
        path.parent
        for message in messages
        if message.get("reason") == "compiler-artifact"
        for filename in message.get("filenames", [])
        if (path := pathlib.Path(str(filename))).suffix.lower()
        in {".so", ".dylib", ".dll"}
    )


def _library_path_variable() -> str:
    """Return the host's dynamic library search-path variable."""
    if sys.platform == "win32":
        return "PATH"
    if sys.platform == "darwin":
        return "DYLD_FALLBACK_LIBRARY_PATH"
    if sys.platform == "aix":
        return "LIBPATH"
    return "LD_LIBRARY_PATH"


def _unique_paths(paths: list[str]) -> list[str]:
    """Remove empty and duplicate search-path entries while preserving order."""
    result: list[str] = []
    seen: set[str] = set()
    for path in paths:
        if path and path not in seen:
            seen.add(path)
            result.append(path)
    return result


def _resolve_cargo_executable(command: tuple[str, ...]) -> str:
    """Resolve the command used by the runner for Cargo subprocesses."""
    executable = pathlib.Path(command[0])
    has_path_separator = os.sep in command[0] or (
        os.altsep is not None and os.altsep in command[0]
    )
    if executable.is_absolute():
        return str(executable)
    if has_path_separator:
        return str(executable.resolve())
    return command[0]
