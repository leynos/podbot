"""Specify Cargo option parsing and unsupported mappings."""

from __future__ import annotations

import pathlib

import pytest
from test_runner_models import RunnerError
from test_runner_options import parse_cargo_test_options


@pytest.mark.parametrize(
    "arguments",
    [
        ["--config", "build.target=x86_64-unknown-linux-gnu"],
        ["--doc", "--test", "compile_contract"],
        ["--exclude", "podbot"],
        ["--target-dir"],
        ["one", "two"],
    ],
    ids=[
        "unsafe-config",
        "doc-and-test",
        "exclude-needs-workspace",
        "missing-option-value",
        "two-filters",
    ],
)
def test_unsupported_mappings_fail_before_running_cargo(arguments: list[str]) -> None:
    """Options the runner cannot map consistently fail at the CLI boundary."""
    with pytest.raises(RunnerError):
        parse_cargo_test_options(arguments)


def test_option_values_may_start_with_a_hyphen() -> None:
    """Required values are consumed even when they resemble options."""
    options = parse_cargo_test_options(["--target-dir", "-build"])

    assert options.target_dir == pathlib.Path.cwd().resolve() / "-build", (
        "a leading hyphen in a value must not be mistaken for a missing value"
    )


@pytest.mark.parametrize(
    ("argument", "common", "package_specs"),
    [
        ("-j8", ("-j", "8"), ()),
        ("-Finternal", ("-F", "internal"), ()),
        ("-F=internal", ("-F", "internal"), ()),
        ("-ppodbot", ("-p", "podbot"), ("podbot",)),
        ("-p=podbot", ("-p", "podbot"), ("podbot",)),
    ],
)
def test_attached_short_value_options_are_normalized(
    argument: str,
    common: tuple[str, ...],
    package_specs: tuple[str, ...],
) -> None:
    """Attached short-option values map to the same phases as separated ones."""
    options = parse_cargo_test_options([argument])

    assert options.common == common, "the option and value must remain explicit"
    assert options.package_specs == package_specs, (
        "attached package selection must remain package-scoped"
    )
