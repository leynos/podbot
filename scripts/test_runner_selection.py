"""Expose focused selectors used by Cargo test-plan construction.

The implementation lives in package, feature, target, and registry modules;
this module preserves the existing runner import surface.
"""

from __future__ import annotations

from test_runner_features import (
    _enabled_features as _enabled_features,
    _feature_values as _feature_values,
    targets_for_package as targets_for_package,
)
from test_runner_package_selection import select_packages as select_packages
from test_runner_registry import validate_nested_registry as validate_nested_registry
from test_runner_target_selection import (
    select_targets as select_targets,
    targets_matching as targets_matching,
)
