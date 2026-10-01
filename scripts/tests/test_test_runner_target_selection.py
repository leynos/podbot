"""Specify default and explicit Cargo target selection behavior."""

from __future__ import annotations

import pathlib

import pytest
from test_runner_models import RunnerError
from test_runner_options import parse_cargo_test_options
from test_runner_plan import create_test_plan
from test_runner_fixtures import package_document


def test_default_selection_compiles_examples_without_test_harnesses(
    tmp_path: pathlib.Path,
) -> None:
    """Default Cargo tests still compile examples marked as non-test targets."""
    metadata = package_document(tmp_path)
    metadata["packages"][0]["targets"].append(
        {
            "name": "example_without_tests",
            "kind": ["example"],
            "test": False,
            "bench": False,
            "doctest": False,
            "required-features": [],
        }
    )

    plan = create_test_plan(metadata, parse_cargo_test_options([]))

    assert ("--example", "example_without_tests") in tuple(
        zip(plan.ordinary_target_args, plan.ordinary_target_args[1:])
    ), "the default phase must compile examples excluded from test harnesses"


@pytest.mark.parametrize(
    ("arguments", "cli_enabled"),
    [
        ([], True),
        (["--no-default-features"], False),
        (["--no-default-features", "--features", "internal"], False),
        (["--no-default-features", "--features", "cli"], True),
    ],
    ids=["package-default", "no-defaults", "other-feature", "explicit-cli"],
)
def test_default_target_selection_respects_required_features(
    tmp_path: pathlib.Path,
    arguments: list[str],
    cli_enabled: bool,
) -> None:
    """Default feature state controls inclusion of the CLI binary target."""
    options = parse_cargo_test_options(arguments)

    plan = create_test_plan(package_document(tmp_path), options)

    selected_cli_binary = any(
        target.name == "podbot" and "bin" in target.kinds
        for target in plan.selected_targets
    )
    target_arguments = tuple(
        zip(plan.ordinary_target_args, plan.ordinary_target_args[1:])
    )
    assert selected_cli_binary is cli_enabled, (
        "default selection must include the binary only when its required cli "
        "feature is active"
    )
    assert (("--bin", "podbot") in target_arguments) is cli_enabled, (
        "ordinary Cargo arguments must match enabled target features"
    )


def test_plural_test_selection_skips_feature_gated_binary(
    tmp_path: pathlib.Path,
) -> None:
    """Plural test selection omits targets excluded by Cargo feature gates."""
    options = parse_cargo_test_options(["--no-default-features", "--tests"])

    plan = create_test_plan(package_document(tmp_path), options)

    selected_cli_binary = any(
        target.name == "podbot" and "bin" in target.kinds
        for target in plan.selected_targets
    )
    assert not selected_cli_binary, (
        "the cli-gated binary must be absent from no-default test selection"
    )
    assert "--bins" not in plan.ordinary_target_args, (
        "the ordinary Cargo phase must not select a disabled binary group"
    )


def test_plural_bin_selection_returns_empty_when_every_match_is_feature_gated(
    tmp_path: pathlib.Path,
) -> None:
    """Plural selectors retain no targets when every match fails its gate."""
    plan = create_test_plan(
        package_document(tmp_path),
        parse_cargo_test_options(["--no-default-features", "--bins"]),
    )

    assert plan.selected_targets == (), (
        "a plural selector with only feature-gated matches must return no targets"
    )


def test_plural_selector_without_matches_raises_runner_error(
    tmp_path: pathlib.Path,
) -> None:
    """A plural category with no metadata matches is an invalid selection."""
    metadata = package_document(tmp_path)
    package = metadata["packages"][0]
    package["targets"] = [
        target for target in package["targets"] if "bench" not in target["kind"]
    ]
    for target in package["targets"]:
        target["bench"] = False

    with pytest.raises(RunnerError, match="Cargo target selection contains no targets"):
        create_test_plan(metadata, parse_cargo_test_options(["--benches"]))


def test_repeated_and_mixed_selectors_preserve_target_order_and_deduplicate(
    tmp_path: pathlib.Path,
) -> None:
    """Repeated selectors preserve first-match order and target identity."""
    plan = create_test_plan(
        package_document(tmp_path),
        parse_cargo_test_options(
            [
                "--test",
                "compile_contract",
                "--example",
                "example_check",
                "--tests",
                "--example",
                "example_check",
            ]
        ),
    )

    assert [(target.name, target.kinds) for target in plan.selected_targets] == [
        ("compile_contract", ("test",)),
        ("example_check", ("example",)),
        ("podbot", ("lib",)),
        ("podbot", ("bin",)),
        ("cli_feature_gating", ("test",)),
    ], "selection must retain first occurrence order and deduplicate by target identity"


def test_explicit_target_selection_preserves_cargo_feature_error(
    tmp_path: pathlib.Path,
) -> None:
    """An explicit gated target reaches Cargo to preserve its feature error."""
    options = parse_cargo_test_options(["--no-default-features", "--bin", "podbot"])

    plan = create_test_plan(package_document(tmp_path), options)

    assert [target.name for target in plan.selected_targets] == ["podbot"], (
        "an explicitly named disabled target must not be silently dropped"
    )
    assert plan.ordinary_target_args == ("--bin", "podbot"), (
        "Cargo must receive the explicit selector and report its normal error"
    )
