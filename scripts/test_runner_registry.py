"""Register test targets whose harnesses start nested Cargo processes.

This registry belongs to the repository test runner. Add a target only when
its test executable can start Cargo while another Cargo process is alive.
The runner checks every entry against `cargo metadata` before scheduling it.
"""

from __future__ import annotations

NESTED_CARGO_TARGETS: frozenset[tuple[str, str]] = frozenset(
    {("podbot", "compile_contract")}
)
