"""Plan Cargo test phases from workspace package and target metadata."""

from __future__ import annotations

import pathlib
import typing as typ

from test_runner_models import CargoTestOptions, TestPlan
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
) -> TestPlan:
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
    ordinary_package_phases: list[tuple[str, tuple[str, ...]]] = []
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
            ordinary_package_phases.append((str(package["name"]), package_arguments))
    ordinary_package_args = tuple(ordinary_package_phases)
    ordinary_args = tuple(
        argument
        for _, package_arguments in ordinary_package_args
        for argument in package_arguments
    )
    run_doctests = options.doc_only or (
        not options.has_explicit_selection
        and any(target.is_doctest for target in targets if "lib" in target.kinds)
    )
    workspace_root = pathlib.Path(str(metadata["workspace_root"]))
    target_directory = options.target_dir or pathlib.Path(
        str(metadata["target_directory"])
    )
    if not target_directory.is_absolute():
        target_directory = pathlib.Path.cwd() / target_directory
    return TestPlan(
        options=options,
        selected_packages=packages,
        selected_targets=selected,
        selected_target_args=selected_args,
        ordinary_targets=ordinary,
        ordinary_target_args=ordinary_args,
        ordinary_package_args=ordinary_package_args,
        nested_targets=nested,
        run_doctests=run_doctests,
        workspace_root=workspace_root,
        target_directory=target_directory,
    )
