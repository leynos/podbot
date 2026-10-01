"""Exercise pure Cargo argument invariants over generated input sequences."""

from __future__ import annotations

import dataclasses

from hypothesis import given, strategies as st

from test_runner_commands import without_package_selection
from test_runner_options import parse_cargo_test_options

_FEATURE_NAME = st.text(
    alphabet="abcdefghijklmnopqrstuvwxyz0123456789_-", min_size=1, max_size=12
)
_PACKAGE_NAME = st.from_regex(r"[a-z][a-z0-9_-]{0,12}", fullmatch=True)


@given(features=st.lists(_FEATURE_NAME, min_size=1, max_size=5).map(",".join))
def test_attached_and_separate_feature_options_are_equivalent(features: str) -> None:
    """Cargo feature options retain the same plan in attached and split forms."""
    attached_long = parse_cargo_test_options([f"--features={features}"])
    separate_long = parse_cargo_test_options(["--features", features])
    attached_short = parse_cargo_test_options([f"-F{features}"])
    separate_short = parse_cargo_test_options(["-F", features])

    assert attached_long == separate_long
    assert attached_short == separate_short
    assert dataclasses.replace(attached_long, common=attached_short.common) == (
        attached_short
    ), "long and short feature aliases must produce the same parsed semantics"


def _argument_chunks() -> st.SearchStrategy[tuple[tuple[str, ...], tuple[str, ...]]]:
    """Generate valid package selectors alongside unrelated Cargo options."""
    package_selectors = st.one_of(
        _PACKAGE_NAME.map(lambda value: (("--package", value), ())),
        _PACKAGE_NAME.map(lambda value: (("-p", value), ())),
        _PACKAGE_NAME.map(lambda value: (("--workspace", "--exclude", value), ())),
        _PACKAGE_NAME.map(lambda value: ((f"--package={value}",), ())),
        _PACKAGE_NAME.map(lambda value: ((f"-p{value}",), ())),
    )
    kept_flags = st.sampled_from(
        (
            "--release",
            "--lib",
            "--all-features",
            "--no-run",
            "--no-fail-fast",
            "--quiet",
            "-q",
        )
    ).map(lambda argument: ((argument,), (argument,)))
    kept_values = st.one_of(
        _FEATURE_NAME.map(
            lambda value: ((f"--features={value}",), (f"--features={value}",))
        ),
        _FEATURE_NAME.map(
            lambda value: ((f"--target-dir={value}",), (f"--target-dir={value}",))
        ),
    )
    return st.one_of(package_selectors, kept_flags, kept_values)


@given(chunks=st.lists(_argument_chunks(), max_size=50))
def test_package_removal_preserves_every_unrelated_argument(
    chunks: list[tuple[tuple[str, ...], tuple[str, ...]]],
) -> None:
    """Removing expanded package filters preserves other options in order."""
    arguments = tuple(argument for chunk, _ in chunks for argument in chunk)
    expected = tuple(argument for _, chunk in chunks for argument in chunk)

    assert without_package_selection(arguments) == expected
