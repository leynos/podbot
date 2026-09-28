"""Expand Cargo metadata package and target selection for test execution."""

from __future__ import annotations

import fnmatch
import pathlib
import typing as typ

from test_runner_models import CargoTestOptions, RunnerError, Target
from test_runner_registry import NESTED_CARGO_TARGETS


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
    if options.workspace:
        selected_ids = workspace_ids
    elif options.package_specs:
        selected_ids = {
            str(package["id"])
            for package in packages
            if any(
                _package_spec_matches(package, spec) for spec in options.package_specs
            )
        }
        if not selected_ids:
            raise RunnerError(f"no workspace package matches {options.package_specs!r}")
    else:
        selected_ids = set(metadata.get("workspace_default_members", workspace_ids))
    if options.excludes:
        selected_ids = {
            package_id
            for package_id in selected_ids
            if not any(
                _package_spec_matches(package, spec)
                for package in packages
                if package.get("id") == package_id
                for spec in options.excludes
            )
        }
    selected = tuple(
        package for package in packages if package.get("id") in selected_ids
    )
    if not selected:
        raise RunnerError("Cargo package selection contains no workspace packages")
    return selected


def select_targets(
    targets: tuple[Target, ...], options: CargoTestOptions
) -> tuple[Target, ...]:
    """Apply explicit Cargo target selectors or the default test inventory."""
    if options.doc_only:
        return ()
    if not options.selectors:
        return tuple(
            target
            for target in targets
            if target.is_test
            and bool(set(target.kinds) & {"lib", "bin", "test", "example"})
        )
    selected = [
        target
        for selector, pattern in options.selectors
        for target in targets_matching(targets, selector, pattern)
    ]
    unique: dict[tuple[str, str, tuple[str, ...]], Target] = {
        (target.package_id, target.name, target.kinds): target for target in selected
    }
    if not unique:
        raise RunnerError("Cargo target selection contains no targets")
    return tuple(unique.values())


def targets_for_package(package: dict[str, typ.Any]) -> tuple[Target, ...]:
    """Convert one Cargo metadata package into target records."""
    manifest_dir = pathlib.Path(str(package["manifest_path"])).parent
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
        )
        for target in package.get("targets", [])
    )


def targets_matching(
    targets: tuple[Target, ...], selector: str, pattern: str | None
) -> tuple[Target, ...]:
    """Return targets matching one singular or plural Cargo selector."""
    if selector == "all-targets":
        return tuple(target for target in targets if target.is_test or target.is_bench)
    if selector == "lib":
        return tuple(target for target in targets if "lib" in target.kinds)
    if selector == "bins":
        return tuple(target for target in targets if "bin" in target.kinds)
    if selector == "examples":
        return tuple(target for target in targets if "example" in target.kinds)
    if selector == "tests":
        return tuple(
            target
            for target in targets
            if target.is_test
            and bool(set(target.kinds) & {"lib", "bin", "test", "example"})
        )
    if selector == "benches":
        return tuple(target for target in targets if target.is_bench)
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


def cargo_target_arguments(
    all_targets: tuple[Target, ...],
    selected: tuple[Target, ...],
    options: CargoTestOptions,
    *,
    omit_nested: tuple[Target, ...],
) -> tuple[str, ...]:
    """Build Cargo selectors, expanding groups that include nested tests."""
    if options.doc_only:
        return ("--doc",)
    omitted = set(omit_nested)
    arguments: list[str] = []
    selectors = options.selectors or (("default", None),)
    for selector, pattern in selectors:
        arguments.extend(
            _arguments_for_selector(selector, pattern, all_targets, selected, omitted)
        )
    return tuple(_deduplicate_selectors(arguments))


def _arguments_for_selector(
    selector: str,
    pattern: str | None,
    all_targets: tuple[Target, ...],
    selected: tuple[Target, ...],
    omitted: set[Target],
) -> tuple[str, ...]:
    """Dispatch one Cargo target selector to its argument builder."""
    match selector:
        case "all-targets":
            return _all_target_arguments(all_targets, omitted)
        case "tests":
            return _test_arguments(selected, omitted)
        case "benches" | "bins" | "examples" | "lib":
            return _group_arguments(selector, selected)
        case "default":
            return _default_arguments(selected, omitted)
        case _:
            return _named_target_arguments(pattern, selected, omitted)


def _all_target_arguments(
    targets: tuple[Target, ...], omitted: set[Target]
) -> tuple[str, ...]:
    """Expand Cargo's all-target selector while excluding nested tests."""
    arguments: list[str] = []
    for group, kind in (
        ("--lib", "lib"),
        ("--bins", "bin"),
        ("--examples", "example"),
        ("--benches", "bench"),
    ):
        if any(
            kind in target.kinds or (group == "--benches" and target.is_bench)
            for target in targets
        ):
            arguments.append(group)
    arguments.extend(
        part
        for target in targets
        if "test" in target.kinds and target not in omitted
        for part in ("--test", target.name)
    )
    return tuple(arguments)


def _test_arguments(
    targets: tuple[Target, ...], omitted: set[Target]
) -> tuple[str, ...]:
    """Expand Cargo's tests selector into libraries, bins and test targets."""
    arguments = [
        group
        for group, kind in (("--lib", "lib"), ("--bins", "bin"))
        if any(kind in target.kinds for target in targets)
    ]
    candidates = tuple(
        target
        for target in targets
        if bool(set(target.kinds) & {"test", "example"}) and target.is_test
    )
    arguments.extend(
        part
        for target in candidates
        if target not in omitted
        for part in target.cargo_selector()
    )
    return tuple(arguments)


def _group_arguments(selector: str, targets: tuple[Target, ...]) -> tuple[str, ...]:
    """Return a plural Cargo target selector when the package has that kind."""
    kind = selector[:-1] if selector.endswith("s") else selector
    if selector == "benches" or any(kind in target.kinds for target in targets):
        return (f"--{selector}",)
    return ()


def _default_arguments(
    targets: tuple[Target, ...], omitted: set[Target]
) -> tuple[str, ...]:
    """Select default test targets except those that invoke nested Cargo."""
    return tuple(
        part
        for target in targets
        if target not in omitted
        for part in target.cargo_selector()
    )


def _named_target_arguments(
    pattern: str | None,
    targets: tuple[Target, ...],
    omitted: set[Target],
) -> tuple[str, ...]:
    """Select explicitly named or globbed Cargo targets."""
    candidates = tuple(
        target
        for target in targets
        if target.name == pattern or fnmatch.fnmatchcase(target.name, pattern or "")
    )
    return tuple(
        part
        for target in candidates
        if target not in omitted
        for part in target.cargo_selector()
    )


def validate_nested_registry(metadata: dict[str, typ.Any]) -> None:
    """Ensure registered nested-Cargo targets exist as enabled test targets."""
    workspace_member_ids = set(metadata.get("workspace_members", []))
    all_targets = tuple(
        target
        for package in metadata.get("packages", [])
        if package.get("id") in workspace_member_ids
        for target in targets_for_package(package)
    )
    known = {(target.package_name, target.name): target for target in all_targets}
    workspace_package_names = {target.package_name for target in all_targets}
    for registered in NESTED_CARGO_TARGETS:
        if registered[0] not in workspace_package_names:
            continue
        target = known.get(registered)
        if target is None:
            raise RunnerError(
                f"registered nested-Cargo target {registered[0]}:{registered[1]} "
                "is missing from cargo metadata"
            )
        if "test" not in target.kinds or not target.is_test:
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


def _deduplicate_selectors(arguments: list[str]) -> list[str]:
    """Remove repeated target selector pairs without changing their order."""
    unique: list[str] = []
    seen: set[tuple[str, str | None]] = set()
    index = 0
    while index < len(arguments):
        option = arguments[index]
        if option in {"--lib", "--bins", "--examples", "--tests", "--benches"}:
            key = (option, None)
            width = 1
        else:
            value = arguments[index + 1] if index + 1 < len(arguments) else None
            key = (option, value)
            width = 2
        if key not in seen:
            seen.add(key)
            unique.extend(arguments[index : index + width])
        index += width
    return unique
