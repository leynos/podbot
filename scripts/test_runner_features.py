"""Convert Cargo package metadata and resolve package-local feature reachability."""

from __future__ import annotations

import pathlib
import typing as typ

from test_runner_models import CargoTestOptions, Target


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
        pending.extend(_package_local_feature_activations(feature_map, feature))
    return frozenset(enabled)


def _package_local_feature_activations(
    feature_map: dict[str, typ.Any], feature: str
) -> list[str]:
    """Filter one traversal step to package-local feature references.

    This is an implementation detail of `_enabled_features`, not a general
    Cargo feature parser.
    """
    return [
        activated
        for activated in _feature_values(feature_map.get(feature, []))
        if not activated.startswith("dep:")
        and "/" not in activated
        and activated in feature_map
    ]


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
    """Return comma- or whitespace-separated Cargo feature names."""
    if not isinstance(value, (list, tuple)):
        value = [value]
    return [
        feature.strip()
        for item in value
        for feature in str(item).replace(",", " ").split()
        if feature.strip()
    ]
