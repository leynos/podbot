"""Validate nested-Cargo registry metadata against workspace targets."""

from __future__ import annotations

import pathlib

import pytest
from test_runner_models import RunnerError
from test_runner_selection import validate_nested_registry
from test_runner_fixtures import package_document


@pytest.mark.parametrize("target_name", ["cli_feature_gating", "compile_contract"])
def test_registry_rejects_a_removed_target(
    tmp_path: pathlib.Path, target_name: str
) -> None:
    """The registry cannot silently outlive a renamed or removed test target."""
    metadata = package_document(tmp_path)
    metadata["packages"][0]["targets"] = [
        target
        for target in metadata["packages"][0]["targets"]
        if target["name"] != target_name
    ]

    with pytest.raises(RunnerError, match="registered nested-Cargo target"):
        validate_nested_registry(metadata)


def test_registry_ignores_packages_outside_the_workspace(
    tmp_path: pathlib.Path,
) -> None:
    """A repository-specific registry does not constrain another workspace."""
    metadata = package_document(tmp_path)
    metadata["workspace_members"] = []

    validate_nested_registry(metadata)
