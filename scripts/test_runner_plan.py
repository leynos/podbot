"""Plan Cargo test phases from workspace package and target metadata.

`create_test_plan` separates ordinary targets from registered tests that run
nested Cargo, preserving a concrete target inventory for each phase. For
example, selecting Podbot's compile-contract test records it in the isolated
phase:

>>> from test_runner_options import parse_cargo_test_options
>>> package_id = "podbot 0.1.0"
>>> metadata = {
...     "workspace_root": ".",
...     "target_directory": "target",
...     "workspace_members": [package_id],
...     "workspace_default_members": [package_id],
...     "packages": [{
...         "id": package_id,
...         "name": "podbot",
...         "manifest_path": "Cargo.toml",
...         "targets": [{
...             "name": "compile_contract",
...             "kind": ["test"],
...             "test": True,
...             "bench": False,
...             "doctest": False,
...             "required-features": [],
...         }, {
...             "name": "cli_feature_gating",
...             "kind": ["test"],
...             "test": True,
...             "bench": False,
...             "doctest": False,
...             "required-features": [],
...         }],
...     }],
... }
>>> options = parse_cargo_test_options(["--test", "compile_contract"])
>>> [target.name for target in create_test_plan(metadata, options).nested_targets]
['compile_contract']
"""

from __future__ import annotations

import pathlib
import typing as typ

from test_runner_models import CargoTestOptions, CargoTestPlan, Target
from test_runner_selection import (
    cargo_target_arguments,
    select_packages,
    select_targets,
    targets_for_package,
    validate_nested_registry,
)
from test_runner_registry import NESTED_CARGO_TARGETS


def create_test_plan(
    metadata: dict[str, typ.Any], options: CargoTestOptions
) -> CargoTestPlan:
    """Expand metadata and options into ordinary, doc, and nested phases.

    Examples
    --------
    Selecting only `compile_contract` yields no ordinary target and keeps the
    trybuild executable in its own phase.
    """
    packages = select_packages(metadata, options)
    targets = tuple(
        target for package in packages for target in targets_for_package(package)
    )
    selected = select_targets(targets, options)
    validate_nested_registry(metadata)
    nested = tuple(
        target
        for target in selected
        if (target.package_name, target.name) in NESTED_CARGO_TARGETS
    )
    ordinary = tuple(target for target in selected if target not in nested)
    selected_args = cargo_target_arguments(targets, selected, options, omit_nested=())
    ordinary_package_args = _ordinary_package_phases(
        packages, selected, nested, options
    )
    ordinary_args = tuple(
        argument
        for _, package_arguments in ordinary_package_args
        for argument in package_arguments
    )
    workspace_root = pathlib.Path(str(metadata["workspace_root"]))
    return CargoTestPlan(
        options=options,
        selected_packages=packages,
        selected_targets=selected,
        selected_target_args=selected_args,
        ordinary_targets=ordinary,
        ordinary_target_args=ordinary_args,
        ordinary_package_args=ordinary_package_args,
        nested_targets=nested,
        run_doctests=_should_run_doctests(targets, options),
        workspace_root=workspace_root,
        target_directory=_resolve_target_directory(metadata, options),
    )


def _ordinary_package_phases(
    packages: tuple[dict[str, typ.Any], ...],
    selected: tuple[Target, ...],
    nested: tuple[Target, ...],
    options: CargoTestOptions,
) -> tuple[tuple[str, tuple[str, ...]], ...]:
    """Build ordinary Cargo target arguments independently for each package."""
    if options.doc_only:
        return ()
    phases: list[tuple[str, tuple[str, ...]]] = []
    for package in packages:
        package_id = str(package["id"])
        package_selected = tuple(
            target for target in selected if target.package_id == package_id
        )
        package_nested = tuple(
            target for target in nested if target.package_id == package_id
        )
        package_arguments = cargo_target_arguments(
            targets_for_package(package),
            package_selected,
            options,
            omit_nested=package_nested,
        )
        if package_arguments:
            phases.append((str(package["name"]), package_arguments))
    return tuple(phases)


def _should_run_doctests(
    targets: tuple[Target, ...], options: CargoTestOptions
) -> bool:
    """Select default documentation tests or an explicit doc-only request."""
    return options.doc_only or (
        not options.has_explicit_selection
        and any(target.is_doctest for target in targets if "lib" in target.kinds)
    )


def _resolve_target_directory(
    metadata: dict[str, typ.Any], options: CargoTestOptions
) -> pathlib.Path:
    """Resolve target paths from the selected workspace root."""
    target_directory = options.target_dir or pathlib.Path(
        str(metadata["target_directory"])
    )
    if not target_directory.is_absolute():
        workspace_root = pathlib.Path(str(metadata["workspace_root"])).resolve()
        target_directory = workspace_root / target_directory
    return target_directory
