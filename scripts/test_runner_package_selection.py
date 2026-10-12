"""Select workspace packages from Cargo metadata and package selectors."""

from __future__ import annotations

import fnmatch
import typing as typ

from test_runner_models import CargoTestOptions, RunnerError


def select_packages(
    metadata: dict[str, typ.Any], options: CargoTestOptions
) -> tuple[dict[str, typ.Any], ...]:
    """Select workspace packages using Cargo's package-name forms."""
    workspace_ids = set(metadata.get("workspace_members", []))
    packages = [
        package
        for package in metadata.get("packages", [])
        if package.get("id") in workspace_ids
    ]
    selected_ids = _initial_package_ids(metadata, options, packages)
    if options.excludes:
        selected_ids = _without_excluded(packages, selected_ids, options.excludes)
    selected = tuple(
        package for package in packages if package.get("id") in selected_ids
    )
    if not selected:
        raise RunnerError("Cargo package selection contains no workspace packages")
    return selected


def _initial_package_ids(
    metadata: dict[str, typ.Any],
    options: CargoTestOptions,
    packages: list[dict[str, typ.Any]],
) -> set[str]:
    """Resolve workspace, explicit-package, or default package identities."""
    workspace_ids = set(metadata.get("workspace_members", []))
    if options.workspace:
        return workspace_ids
    if options.package_specs:
        selected_ids = {
            str(package["id"])
            for package in packages
            if any(
                _package_spec_matches(package, spec) for spec in options.package_specs
            )
        }
        if not selected_ids:
            raise RunnerError(f"no workspace package matches {options.package_specs!r}")
        return selected_ids
    return set(metadata.get("workspace_default_members", workspace_ids))


def _without_excluded(
    packages: list[dict[str, typ.Any]],
    selected_ids: set[str],
    excludes: tuple[str, ...],
) -> set[str]:
    """Remove package identities matched by Cargo's workspace exclusions."""
    return {
        package_id
        for package_id in selected_ids
        if not any(
            _package_spec_matches(package, spec)
            for package in packages
            if package.get("id") == package_id
            for spec in excludes
        )
    }


def _package_spec_matches(package: dict[str, typ.Any], spec: str) -> bool:
    """Match a Cargo package selector against its name, id, or version."""
    name = str(package.get("name", ""))
    package_id = str(package.get("id", ""))
    versioned_name = f"{name}@{package.get('version', '')}"
    return any(
        fnmatch.fnmatchcase(candidate, spec)
        for candidate in (name, package_id, versioned_name)
    )
