"""Reusable Cargo metadata and artifact documents for runner tests."""

from __future__ import annotations

import pathlib
import typing as typ


def package_document(root: pathlib.Path) -> dict[str, typ.Any]:
    """Return a workspace metadata document with each Cargo target kind."""
    package_id = "path+file:///workspace/podbot#podbot@0.1.0"
    targets = [
        _target("podbot", ["lib"], test=True, bench=True, doctest=True),
        _target("podbot", ["bin"], test=True, bench=True, features=["cli"]),
        _target("compile_contract", ["test"], test=True),
        _target("cli_feature_gating", ["test"], test=True),
        _target("example_check", ["example"], test=True),
        _target("benchmarks", ["bench"], bench=True),
    ]
    return {
        "workspace_root": str(root),
        "target_directory": str(root / "target"),
        "workspace_members": [package_id],
        "workspace_default_members": [package_id],
        "packages": [
            {
                "id": package_id,
                "name": "podbot",
                "version": "0.1.0",
                "manifest_path": str(root / "Cargo.toml"),
                "authors": ["Podbot Authors"],
                "description": "A test package",
                "repository": "https://example.invalid/podbot",
                "license": "MIT",
                "license_file": None,
                "homepage": None,
                "rust_version": "1.88",
                "categories": [],
                "keywords": [],
                "targets": targets,
            }
        ],
    }


def _target(
    name: str,
    kinds: list[str],
    *,
    test: bool = False,
    bench: bool = False,
    doctest: bool = False,
    features: list[str] | None = None,
) -> dict[str, typ.Any]:
    """Build a Cargo metadata target row."""
    return {
        "name": name,
        "kind": kinds,
        "test": test,
        "bench": bench,
        "doctest": doctest,
        "required-features": features or [],
    }
