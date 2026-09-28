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
from test_runner_supervisor import ProcessSupervisor


def load_cargo_metadata(
    cargo_command: tuple[str, ...],
    options: CargoTestOptions,
    cwd: pathlib.Path,
    environment: dict[str, str],
    supervisor: ProcessSupervisor,
) -> dict[str, typ.Any]:
    """Return workspace metadata without downloading dependency metadata.

    Examples
    --------
    The runner passes a versioned JSON request so package and target selection
    does not depend on Cargo's human-readable output.
    """
    command = [*cargo_command, "metadata", "--no-deps", "--format-version", "1"]
    if options.manifest_path is not None:
        command.extend(["--manifest-path", str(options.manifest_path)])
    for flag in ("--offline", "--locked", "--frozen"):
        if flag in options.common and flag not in command:
            command.append(flag)
    status, stdout, stderr = supervisor.run_capture(
        command,
        cwd,
        environment,
        purpose="Cargo metadata",
    )
    if status != 0:
        if stderr:
            sys.stderr.write(stderr)
        if supervisor.terminal_status is not None:
            raise RunnerCommandFailure(supervisor.terminal_status)
        raise RunnerError(f"cargo metadata exited with status {status}")
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
        candidates = [
            message
            for message in messages
            if message.get("reason") == "compiler-artifact"
            and message.get("package_id") == expected.package_id
            and message.get("target", {}).get("name") == expected.name
            and "test" in message.get("target", {}).get("kind", [])
            and message.get("profile", {}).get("test") is True
            and isinstance(message.get("executable"), str)
        ]
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
        selected[key] = executable
    return selected


def create_test_runtime_environment(
    base_environment: dict[str, str],
    package: dict[str, typ.Any],
    executable: pathlib.Path,
    messages: list[dict[str, typ.Any]],
    *,
    target_directory: pathlib.Path,
    cargo_command: tuple[str, ...],
) -> dict[str, str]:
    """Add the Cargo package, target, binary, and dynamic-library environment.

    Examples
    --------
    The inherited library path remains present after Cargo's target paths are
    prepended, so custom native libraries remain discoverable.
    """
    environment = base_environment.copy()
    _set_package_environment(environment, package)
    environment["CARGO_TARGET_DIR"] = str(target_directory)
    target_temporary_directory = target_directory / "tmp"
    target_temporary_directory.mkdir(parents=True, exist_ok=True)
    environment["CARGO_TARGET_TMPDIR"] = str(target_temporary_directory)
    environment["CARGO"] = _resolve_cargo_executable(cargo_command)
    if len(cargo_command) > 1 and cargo_command[1].startswith("+"):
        environment["RUSTUP_TOOLCHAIN"] = cargo_command[1][1:]
    for message in messages:
        if message.get("reason") == "compiler-artifact":
            _record_binary_executable(environment, package, message)
    profile = next(
        (
            message.get("profile", {})
            for message in messages
            if message.get("reason") == "compiler-artifact"
            and message.get("executable") == str(executable)
        ),
        {},
    )
    if "debug_assertions" in profile:
        environment["CARGO_DEBUG_ASSERTIONS"] = str(profile["debug_assertions"]).lower()
    _set_dynamic_library_environment(environment, executable, messages)
    return environment


def _set_package_environment(
    environment: dict[str, str], package: dict[str, typ.Any]
) -> None:
    """Restore Cargo's package metadata environment variables."""
    manifest_dir = pathlib.Path(str(package["manifest_path"])).parent
    environment["CARGO_MANIFEST_DIR"] = str(manifest_dir)
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
    target = message.get("target", {})
    if (
        message.get("package_id") == package.get("id")
        and "bin" in target.get("kind", [])
        and message.get("profile", {}).get("test") is False
        and isinstance(message.get("executable"), str)
    ):
        environment[f"CARGO_BIN_EXE_{target['name']}"] = message["executable"]


def _set_dynamic_library_environment(
    environment: dict[str, str],
    executable: pathlib.Path,
    messages: list[dict[str, typ.Any]],
) -> None:
    """Reconstruct Cargo's runtime library search paths for the host platform."""
    profile_directory = executable.parent.parent
    search_paths = [profile_directory / "deps", profile_directory]
    for message in messages:
        if message.get("reason") == "build-script-executed":
            for linked_path in message.get("linked_paths", []):
                _, separator, path = str(linked_path).partition("=")
                candidate = pathlib.Path(path if separator else linked_path)
                if candidate.is_dir():
                    search_paths.append(candidate)
        if message.get("reason") == "compiler-artifact":
            filenames = message.get("filenames", [])
            for filename in filenames:
                path = pathlib.Path(str(filename))
                if path.suffix.lower() in {".so", ".dylib", ".dll"}:
                    search_paths.append(path.parent)

    variable = _library_path_variable()
    inherited_paths = environment.get(variable, "").split(os.pathsep)
    combined = _unique_paths([*map(str, search_paths), *inherited_paths])
    if combined:
        environment[variable] = os.pathsep.join(combined)


def _library_path_variable() -> str:
    """Return the host's dynamic library search-path variable."""
    if sys.platform == "win32":
        return "PATH"
    if sys.platform == "darwin":
        return "DYLD_FALLBACK_LIBRARY_PATH"
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
