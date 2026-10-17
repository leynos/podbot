"""Decode Cargo JSON lines for the supervised test runner.

This private module owns stream decoding only; executable selection and runtime
environment restoration remain in test_runner_cargo.
"""

from __future__ import annotations

import json
import typing as typ


def parse_cargo_json_message(line: str) -> dict[str, typ.Any] | None:
    """Decode one Cargo JSON message, returning None for ordinary tool output.

    Examples
    --------
    >>> parse_cargo_json_message('{"reason":"build-finished","success":true}')
    {'reason': 'build-finished', 'success': True}
    >>> parse_cargo_json_message('compiler says hello') is None
    True
    """
    try:
        value = json.loads(line)
    except json.JSONDecodeError:
        return None
    return value if isinstance(value, dict) else None
