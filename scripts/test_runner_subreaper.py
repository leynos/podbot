"""Control Linux child-subreaper state for supervised process cleanup."""

from __future__ import annotations

import sys

_PR_GET_CHILD_SUBREAPER = 37
_PR_SET_CHILD_SUBREAPER = 36


def enable_child_subreaper() -> bool:
    """Make this Linux process adopt orphaned descendants when supported."""
    return set_child_subreaper(True)


def get_child_subreaper() -> bool | None:
    """Read the current Linux child-subreaper flag when supported."""
    if not _is_linux():
        return None
    enabled = _prctl(_PR_GET_CHILD_SUBREAPER, read_integer=True)
    return bool(enabled) if enabled is not None else None


def set_child_subreaper(enabled: bool) -> bool:
    """Set Linux child-subreaper behaviour without affecting other platforms."""
    if not _is_linux():
        return False
    return _prctl(_PR_SET_CHILD_SUBREAPER, int(enabled)) == 0


def _prctl(option: int, argument: int = 0, *, read_integer: bool = False) -> int | None:
    """Call prctl because Python's stdlib has no child-subreaper interface."""
    import ctypes

    # FIXME(#188): Revisit this bridge if Python adds stdlib subreaper controls.
    try:
        if read_integer:
            value = ctypes.c_int()
            result = ctypes.CDLL(None, use_errno=True).prctl(
                option, ctypes.byref(value), 0, 0, 0
            )
            return value.value if result == 0 else None
        return ctypes.CDLL(None, use_errno=True).prctl(option, argument, 0, 0, 0)
    except (AttributeError, OSError):
        return None


def _is_linux() -> bool:
    """Return whether the interpreter runs with Linux prctl semantics."""
    return sys.platform.startswith("linux")
