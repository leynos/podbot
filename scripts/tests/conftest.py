"""Shared set-up for the workflow contracts in this directory.

The readers live in `scripts/`, which is not a package and is not on
`sys.path` when pytest collects these files from the repository root.
pytest imports this module before any test module beside it, so putting
the directory on the path here lets every test import the readers
normally, rather than each carrying its own bootstrap.
"""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))


@pytest.fixture(name="workflow_texts")
def fixture_workflow_texts() -> dict[str, str]:
    """Return every workflow file's text, keyed by file name.

    Imported here rather than at the top of the module: the spelling
    helper's tests share this directory and run without PyYAML, so a
    module-level import of the readers would break their collection.
    """
    from workflow_contracts import load_workflow_documents

    return load_workflow_documents()
