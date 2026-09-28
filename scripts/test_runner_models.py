"""Represent Cargo test options, target metadata, and planned test phases.

These immutable records keep option parsing, workspace discovery, and process
execution independent while carrying the package identity needed to select
the exact artifacts from one Cargo build. For example, a target record maps a
registered integration test to Cargo's ``--test`` selector:

>>> target = Target(
...     "podbot", "id", pathlib.Path("."), "api", ("test",),
...     True, False, False, ()
... )
>>> target.cargo_selector()
('--test', 'api')
"""

from __future__ import annotations

import dataclasses
import pathlib
import typing as typ


class RunnerError(ValueError):
    """A test command cannot be mapped safely onto separated phases."""


@dataclasses.dataclass(frozen=True)
class Target:
    """A Cargo target tied to its owning package."""

    package_name: str
    package_id: str
    manifest_dir: pathlib.Path
    name: str
    kinds: tuple[str, ...]
    is_test: bool
    is_bench: bool
    is_doctest: bool
    required_features: tuple[str, ...]

    def cargo_selector(self) -> tuple[str, ...]:
        """Return Cargo's exact selector for this target.

        Examples
        --------
        >>> target = Target("podbot", "id", pathlib.Path("."), "ui", ("test",), True, False, False, ())
        >>> target.cargo_selector()
        ('--test', 'ui')
        """
        selector = next(
            (
                kind
                for kind in ("lib", "bin", "test", "example", "bench")
                if kind in self.kinds
            ),
            None,
        )
        if selector is None:
            raise RunnerError(
                f"cannot test Cargo target {self.package_name}:{self.name} "
                f"with kinds {self.kinds!r}"
            )
        if selector == "lib":
            return ("--lib",)
        return (f"--{selector}", self.name)


@dataclasses.dataclass(frozen=True)
class CargoTestOptions:
    """The Cargo options the runner can preserve across its phases."""

    common: tuple[str, ...]
    selectors: tuple[tuple[str, str | None], ...]
    package_specs: tuple[str, ...]
    excludes: tuple[str, ...]
    workspace: bool
    doc_only: bool
    no_run: bool
    no_fail_fast: bool
    test_filter: str | None
    harness_args: tuple[str, ...]
    manifest_path: pathlib.Path | None
    target_dir: pathlib.Path | None
    all_targets: bool

    @property
    def has_explicit_selection(self) -> bool:
        """Whether the caller selected target kinds explicitly."""
        return self.doc_only or bool(self.selectors)


@dataclasses.dataclass(frozen=True)
class CargoTestPlan:
    """The ordinary, doctest, and nested-Cargo phases for one invocation."""

    options: CargoTestOptions
    selected_packages: tuple[dict[str, typ.Any], ...]
    selected_targets: tuple[Target, ...]
    selected_target_args: tuple[str, ...]
    ordinary_targets: tuple[Target, ...]
    ordinary_target_args: tuple[str, ...]
    ordinary_package_args: tuple[tuple[str, tuple[str, ...]], ...]
    nested_targets: tuple[Target, ...]
    run_doctests: bool
    workspace_root: pathlib.Path
    target_directory: pathlib.Path
