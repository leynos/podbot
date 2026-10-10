"""Exercise pure Cargo argument invariants over generated input sequences."""

from __future__ import annotations

import dataclasses

from hypothesis import given, strategies as st

from report_sccache_errors import (
    MAX_DIAGNOSTIC_LINES,
    MAX_LINE_LENGTH,
    sanitize_error_log,
)
from test_runner_commands import without_package_selection
from test_runner_options import parse_cargo_test_options
from test_runner_process_snapshot import ProcessInfo
from test_runner_process_tree import OwnedProcess, OwnedProcessTree, ProcessTreeRoot

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

    assert attached_long == separate_long, (
        "attached and separate long feature options must parse identically"
    )
    assert attached_short == separate_short, (
        "attached and separate short feature options must parse identically"
    )
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

    assert without_package_selection(arguments) == expected, (
        "removing package selectors must preserve every other argument in order"
    )


@given(parent_ids=st.lists(st.integers(min_value=0, max_value=100), max_size=100))
def test_process_tree_discovery_matches_reachable_children(
    parent_ids: list[int],
) -> None:
    """Ownership discovery follows every reachable child and no unrelated one."""
    root = _process_info(1, 0)
    processes = {root.pid: root}
    for pid, parent_pid in enumerate(parent_ids, start=2):
        processes[pid] = _process_info(pid, parent_pid)

    tree = OwnedProcessTree(ProcessTreeRoot(1, "root", 0.0, False))
    tree.owned[root.pid] = OwnedProcess(root, 0.0)
    tree._discover_descendants(processes)

    children_by_parent: dict[int, list[int]] = {}
    for process in processes.values():
        children_by_parent.setdefault(process.parent_pid, []).append(process.pid)
    reachable = {root.pid}
    pending = [root.pid]
    while pending:
        parent_pid = pending.pop()
        for child in children_by_parent.get(parent_pid, ()):
            if child not in reachable:
                reachable.add(child)
                pending.append(child)

    assert set(tree.owned) == reachable, (
        "discovery must own exactly the processes reachable from the root"
    )


@given(
    secret=st.text(
        alphabet="abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_",
        min_size=16,
        max_size=40,
    ),
    count=st.integers(min_value=0, max_value=60),
)
def test_sanitized_diagnostics_redact_secrets_and_stay_bounded(
    secret: str,
    count: int,
) -> None:
    """Generated credential-bearing failures remain redacted and bounded."""
    log = "\n".join(
        f"ERROR failed to write cache: token={secret} item={index}"
        for index in range(count)
    )

    lines = sanitize_error_log(log, (secret,))

    assert len(lines) <= MAX_DIAGNOSTIC_LINES, (
        "sanitized diagnostics must respect the output line cap"
    )
    assert all(len(line) <= MAX_LINE_LENGTH for line in lines), (
        "sanitized diagnostics must respect the per-line character cap"
    )
    assert all(secret not in line for line in lines), (
        "known credential values must not appear in sanitized output"
    )


def _process_info(pid: int, parent_pid: int) -> ProcessInfo:
    """Build a small stable process identity for generated ownership graphs."""
    return ProcessInfo(
        pid=pid,
        parent_pid=parent_pid,
        process_group=1,
        start_time=pid,
        state="S",
        command=f"process-{pid}",
    )
