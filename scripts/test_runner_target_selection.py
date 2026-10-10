"""Select Cargo test targets from package metadata and target selectors."""

from __future__ import annotations

import fnmatch
from collections.abc import Callable

from test_runner_models import CargoTestOptions, RunnerError, Target

_TARGET_MATCHERS: dict[str, Callable[[Target], bool]] = {
    "all-targets": lambda target: target.is_test or target.is_bench,
    "lib": lambda target: "lib" in target.kinds,
    "bins": lambda target: "bin" in target.kinds,
    "examples": lambda target: "example" in target.kinds,
    "tests": lambda target: _is_default_test_target(target),
    "benches": lambda target: target.is_bench,
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
    """Select Cargo's default test targets and examples built without tests."""
    return tuple(
        target
        for target in targets
        if (_is_default_test_target(target) or "example" in target.kinds)
        and target.has_required_features_enabled
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
        selected_targets.extend(_targets_for_selector(selector, matches))
    unique = {
        (target.package_id, target.name, target.kinds): target
        for target in selected_targets
    }
    if not unique and not matched_targets:
        raise RunnerError("Cargo target selection contains no targets")
    return tuple(unique.values())


def _targets_for_selector(
    selector: str, matches: tuple[Target, ...]
) -> tuple[Target, ...]:
    """Keep explicitly named targets; feature-filter all category matches."""
    if selector in {"bin", "test", "example", "bench"}:
        return matches
    return tuple(target for target in matches if target.has_required_features_enabled)


def _is_default_test_target(target: Target) -> bool:
    """Match Cargo's default test inventory for one metadata target."""
    return target.is_test and bool(
        set(target.kinds) & {"lib", "bin", "test", "example"}
    )


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
