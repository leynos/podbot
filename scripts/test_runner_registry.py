"""Register test targets whose harnesses start nested Cargo processes.

This registry belongs to the repository test runner. Add a target only when
its test executable can start Cargo while another Cargo process is alive.
The runner checks every entry against `cargo metadata` before scheduling it.
"""

from __future__ import annotations

import typing as typ

from test_runner_features import targets_for_package
from test_runner_models import RunnerError, Target

NESTED_CARGO_TARGETS: frozenset[tuple[str, str]] = frozenset(
    {
        ("podbot", "cli_feature_gating"),
        ("podbot", "compile_contract"),
    }
)


def validate_nested_registry(metadata: dict[str, typ.Any]) -> None:
    """Ensure registered nested-Cargo targets exist as enabled test targets."""
    known, workspace_package_names = _nested_registry_targets(metadata)
    for registered in NESTED_CARGO_TARGETS:
        if registered[0] in workspace_package_names:
            _validate_registered_target(registered, known)


def _nested_registry_targets(
    metadata: dict[str, typ.Any],
) -> tuple[dict[tuple[str, str], Target], set[str]]:
    """Index workspace metadata for registered nested-Cargo targets."""
    workspace_member_ids = set(metadata.get("workspace_members", []))
    all_targets = tuple(
        target
        for package in metadata.get("packages", [])
        if package.get("id") in workspace_member_ids
        for target in targets_for_package(package)
    )
    known = {(target.package_name, target.name): target for target in all_targets}
    workspace_package_names = {target.package_name for target in all_targets}
    return known, workspace_package_names


def _validate_registered_target(
    registered: tuple[str, str], known: dict[tuple[str, str], Target]
) -> None:
    """Require one registered target to be an enabled integration test."""
    target = known.get(registered)
    if target is None:
        raise RunnerError(
            f"registered nested-Cargo target {registered[0]}:{registered[1]} "
            "is missing from cargo metadata"
        )
    if "test" in target.kinds and target.is_test:
        return
    raise RunnerError(
        f"registered nested-Cargo target {registered[0]}:{registered[1]} "
        "is not an enabled integration test"
    )
