"""Expand selected Cargo test targets into command-line selectors."""

from __future__ import annotations

import dataclasses
import fnmatch

from test_runner_models import CargoTestOptions, Target

_ALL_TARGET_GROUPS = (
    ("--lib", lambda target: "lib" in target.kinds),
    ("--bins", lambda target: "bin" in target.kinds),
    ("--examples", lambda target: "example" in target.kinds),
    ("--benches", lambda target: "bench" in target.kinds or target.is_bench),
)


@dataclasses.dataclass(frozen=True)
class TargetArgumentContext:
    """Hold one selection's target data while expanding Cargo selectors.

    This record is local to target argument expansion. It keeps selector
    dispatch separate from package selection and test-plan construction.
    """

    all_targets: tuple[Target, ...]
    selected_targets: tuple[Target, ...]
    omitted_targets: frozenset[Target]


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
    context = TargetArgumentContext(all_targets, selected, frozenset(omit_nested))
    arguments: list[str] = []
    selectors = options.selectors or (("default", None),)
    for selector, pattern in selectors:
        arguments.extend(_arguments_for_selector(selector, pattern, context))
    return tuple(_deduplicate_selectors(arguments))


def _arguments_for_selector(
    selector: str,
    pattern: str | None,
    context: TargetArgumentContext,
) -> tuple[str, ...]:
    """Dispatch one Cargo target selector to its argument builder."""
    match selector:
        case "all-targets":
            return _all_target_arguments(context.all_targets, context.omitted_targets)
        case "tests":
            return _test_arguments(context.selected_targets, context.omitted_targets)
        case "benches" | "bins" | "examples" | "lib":
            return _group_arguments(selector, context.selected_targets)
        case "default":
            return _default_arguments(context.selected_targets, context.omitted_targets)
        case _:
            return _named_target_arguments(
                pattern, context.selected_targets, context.omitted_targets
            )


def _all_target_arguments(
    targets: tuple[Target, ...], omitted: frozenset[Target]
) -> tuple[str, ...]:
    """Expand Cargo's all-target selector while excluding nested tests."""
    groups = tuple(
        group
        for group, matches in _ALL_TARGET_GROUPS
        if any(
            matches(target) and target.has_required_features_enabled
            for target in targets
        )
    )
    test_arguments = tuple(
        part
        for target in targets
        if (
            "test" in target.kinds
            and target.has_required_features_enabled
            and target not in omitted
        )
        for part in ("--test", target.name)
    )
    return (*groups, *test_arguments)


def _test_arguments(
    targets: tuple[Target, ...], omitted: frozenset[Target]
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
        if target not in omitted and target.has_required_features_enabled
        for part in target.cargo_selector()
    )
    return tuple(arguments)


def _group_arguments(selector: str, targets: tuple[Target, ...]) -> tuple[str, ...]:
    """Return a plural Cargo target selector when the package has that kind."""
    kind = selector[:-1] if selector.endswith("s") else selector
    has_selected_target = (
        any(target.is_bench for target in targets)
        if selector == "benches"
        else any(kind in target.kinds for target in targets)
    )
    if has_selected_target:
        return (f"--{selector}",)
    return ()


def _default_arguments(
    targets: tuple[Target, ...], omitted: frozenset[Target]
) -> tuple[str, ...]:
    """Select default test targets except those that invoke nested Cargo."""
    return tuple(
        part
        for target in targets
        if target not in omitted and target.has_required_features_enabled
        for part in target.cargo_selector()
    )


def _named_target_arguments(
    pattern: str | None,
    targets: tuple[Target, ...],
    omitted: frozenset[Target],
) -> tuple[str, ...]:
    """Select explicitly named or globbed Cargo targets."""
    candidates = tuple(
        target
        for target in targets
        if target.name == pattern or _matches_target_name(target, pattern)
    )
    return tuple(
        part
        for target in candidates
        if target not in omitted
        for part in target.cargo_selector()
    )


def _matches_target_name(target: Target, pattern: str | None) -> bool:
    """Match a target name against the optional Cargo glob pattern."""
    return fnmatch.fnmatchcase(target.name, pattern or "")


def _deduplicate_selectors(arguments: list[str]) -> list[str]:
    """Remove repeated target selector pairs without changing their order."""
    unique: list[str] = []
    seen: set[tuple[str, str | None]] = set()
    index = 0
    while index < len(arguments):
        key, width = _selector_identity(arguments, index)
        if key not in seen:
            seen.add(key)
            unique.extend(arguments[index : index + width])
        index += width
    return unique


def _selector_identity(
    arguments: list[str], index: int
) -> tuple[tuple[str, str | None], int]:
    """Return a singular selector's identity and the number of consumed items."""
    selector = arguments[index]
    if selector in {"--test", "--bench", "--example", "--bin"}:
        return (selector, arguments[index + 1]), 2
    return (selector, None), 1
