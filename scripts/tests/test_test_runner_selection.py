"""Check Cargo feature argument splitting and package target selection."""

from __future__ import annotations

import pathlib
import typing as typ

import pytest
from test_runner_fixtures import package_document
from test_runner_options import parse_cargo_test_options
from test_runner_plan import create_test_plan
from test_runner_selection import _feature_values


@pytest.mark.parametrize(
    ("value", "expected"),
    [
        pytest.param("internal gate", ["internal", "gate"], id="whitespace"),
        pytest.param("internal,gate", ["internal", "gate"], id="comma"),
        pytest.param("internal,, gate ", ["internal", "gate"], id="empty-separators"),
        pytest.param(["internal", "gate"], ["internal", "gate"], id="list"),
        pytest.param(("internal", "gate"), ["internal", "gate"], id="tuple"),
    ],
)
def test_feature_values_accept_cargo_separators(
    value: typ.Any, expected: list[str]
) -> None:
    """Whitespace, commas, and sequence values retain their feature names."""
    assert _feature_values(value) == expected


def test_space_separated_requested_features_enable_target_gate(
    tmp_path: pathlib.Path,
) -> None:
    """A Cargo feature string with spaces enables the named target."""
    metadata = package_document(tmp_path)
    package = metadata["packages"][0]
    package["features"] = {"default": [], "internal": [], "gate": []}
    contract = next(
        target for target in package["targets"] if target["name"] == "compile_contract"
    )
    contract["required-features"] = ["gate"]
    options = parse_cargo_test_options(
        ["--no-default-features", "--features", "internal gate"]
    )

    plan = create_test_plan(metadata, options)
    selected_contract = next(
        target for target in plan.selected_targets if target.name == "compile_contract"
    )

    assert selected_contract.enabled_features == ("gate", "internal"), (
        "space-separated features must reach selected target metadata"
    )
