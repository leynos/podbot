"""Select workspace packages and targets from Cargo metadata.

These selectors apply explicit package and target filters while retaining the
metadata identity required by later build and artifact phases. For example:

>>> target = Target(
...     "podbot", "id", pathlib.Path("."), "api", ("test",),
...     True, False, False, ()
... )
>>> targets_matching((target,), "tests", None)[0].name
'api'
"""

from __future__ import annotations

import fnmatch
import pathlib
import typing as typ
from collections.abc import Callable

from test_runner_models import CargoTestOptions, RunnerError, Target
from test_runner_registry import NESTED_CARGO_TARGETS

_TARGET_MATCHERS: dict[str, Callable[[Target], bool]] = {
    "all-targets": lambda target: target.is_test or target.is_bench,
    "lib": lambda target: "lib" in target.kinds,
    "bins": lambda target: "bin" in target.kinds,
    "examples": lambda target: "example" in target.kinds,
    "tests": lambda target: _is_default_test_target(target),
    "benches": lambda target: target.is_bench,
}


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


def select_targets(
    targets: tuple[Target, ...], options: CargoTestOptions
) -> tuple[Target, ...]:
    """Apply explicit Cargo target selectors or the default test inventory."""
    if options.doc_only:
        return ()
    if not options.selectors:
        return _default_test_targets(targets)
    return _unique_selected_targets(targets, options.selectors)


def _default_test_targets(targets: tuple[Target, ...]) -> tuple[Target, ...]:
    """Select the integration and executable targets tested by Cargo by default."""
    return tuple(
        target
        for target in targets
        if _is_default_test_target(target) and target.has_required_features_enabled
    )


def _unique_selected_targets(
    targets: tuple[Target, ...], selectors: tuple[tuple[str, str | None], ...]
) -> tuple[Target, ...]:
    """Apply selectors once, retaining Cargo's explicit target handling."""
    matched_targets: list[Target] = []
    selected_targets: list[Target] = []
    for selector, pattern in selectors:
        matches = targets_matching(targets, selector, pattern)
        matched_targets.extend(matches)
        if selector not in {"bin", "test", "example", "bench"}:
            matches = tuple(
                target for target in matches if target.has_required_features_enabled
            )
        selected_targets.extend(matches)
    unique = {
        (target.package_id, target.name, target.kinds): target
        for target in selected_targets
    }
    if not unique:
        if matched_targets:
            return ()
        raise RunnerError("Cargo target selection contains no targets")
    return tuple(unique.values())


def _is_default_test_target(target: Target) -> bool:
    """Match Cargo's default test inventory for one metadata target."""
    return target.is_test and bool(
        set(target.kinds) & {"lib", "bin", "test", "example"}
    )


def targets_for_package(
    package: dict[str, typ.Any], options: CargoTestOptions | None = None
) -> tuple[Target, ...]:
    """Convert one Cargo metadata package into target records."""
    manifest_dir = pathlib.Path(str(package["manifest_path"])).parent
    enabled_features = tuple(sorted(_enabled_features(package, options)))
    return tuple(
        Target(
            package_name=str(package["name"]),
            package_id=str(package["id"]),
            manifest_dir=manifest_dir,
            name=str(target["name"]),
            kinds=tuple(str(kind) for kind in target.get("kind", [])),
            is_test=bool(target.get("test", False)),
            is_bench=bool(target.get("bench", False)),
            is_doctest=bool(target.get("doctest", False)),
            required_features=tuple(
                str(feature) for feature in target.get("required-features", [])
            ),
            enabled_features=enabled_features,
        )
        for target in package.get("targets", [])
    )


def _enabled_features(
    package: dict[str, typ.Any], options: CargoTestOptions | None
) -> frozenset[str]:
    """Resolve package feature defaults and requested features for selection.

    Dependency features do not satisfy a target's package-level
    ``required-features`` declaration.
    """
    feature_map = package.get("features", {})
    if not isinstance(feature_map, dict) or options is None:
        return frozenset()
    if "--all-features" in options.common:
        return frozenset(str(feature) for feature in feature_map)

    pending: list[str] = []
    if "--no-default-features" not in options.common:
        pending.extend(_feature_values(feature_map.get("default", [])))
    pending.extend(_requested_package_features(package, options.common))

    enabled: set[str] = set()
    while pending:
        feature = pending.pop()
        if feature in enabled:
            continue
        enabled.add(feature)
        for activated in _feature_values(feature_map.get(feature, [])):
            if activated.startswith("dep:") or "/" in activated:
                continue
            if activated in feature_map:
                pending.append(activated)
    return frozenset(enabled)


def _requested_package_features(
    package: dict[str, typ.Any], common: tuple[str, ...]
) -> list[str]:
    """Read feature flags that apply to the current Cargo package."""
    requested: list[str] = []
    index = 0
    while index < len(common):
        if common[index] in {"--features", "-F"} and index + 1 < len(common):
            requested.extend(_feature_values_for_package(package, common[index + 1]))
            index += 2
        else:
            index += 1
    return requested


def _feature_values_for_package(package: dict[str, typ.Any], value: str) -> list[str]:
    """Extract bare and package-qualified features from one Cargo value."""
    package_names = {
        str(package.get("name", "")),
        str(package.get("id", "")),
        f"{package.get('name', '')}@{package.get('version', '')}",
    }
    selected: list[str] = []
    for feature in _feature_values(value):
        if "/" in feature:
            package_name, feature_name = feature.split("/", maxsplit=1)
            if package_name not in package_names:
                continue
            feature = feature_name
        selected.append(feature)
    return selected


def _feature_values(value: typ.Any) -> list[str]:
    """Return comma-separated Cargo feature names from metadata or arguments."""
    if not isinstance(value, (list, tuple)):
        value = [value]
    return [
        feature.strip()
        for item in value
        for feature in str(item).split(",")
        if feature.strip()
    ]


def targets_matching(
    targets: tuple[Target, ...], selector: str, pattern: str | None
) -> tuple[Target, ...]:
    """Return targets matching one singular or plural Cargo selector."""
    matcher = _TARGET_MATCHERS.get(selector)
    if matcher is None:
        return _matching_named_targets(targets, selector, pattern)
    return tuple(target for target in targets if matcher(target))


def _matching_named_targets(
    targets: tuple[Target, ...], selector: str, pattern: str | None
) -> tuple[Target, ...]:
    """Match singular Cargo target kinds and fail clearly when none exist."""
    matches = tuple(
        target
        for target in targets
        if selector in target.kinds
        and pattern is not None
        and fnmatch.fnmatchcase(target.name, pattern)
    )
    if not matches:
        raise RunnerError(f"no Cargo target matches --{selector} {pattern!r}")
    return matches


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


def _package_spec_matches(package: dict[str, typ.Any], spec: str) -> bool:
    """Match a Cargo package selector against its name, id, or version."""
    name = str(package.get("name", ""))
    package_id = str(package.get("id", ""))
    versioned_name = f"{name}@{package.get('version', '')}"
    return any(
        fnmatch.fnmatchcase(candidate, spec)
        for candidate in (name, package_id, versioned_name)
    )
