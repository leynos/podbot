"""The placement rule, driven directly rather than through these files.

Every job in this repository names a literal runner today, so a check
parametrized over the real workflows passes whether or not it
discriminates anything: it would be satisfied by a rule that returned
"no fault" unconditionally. The contract next door asserts the real
files; this one establishes that the rule it uses can tell the cases
apart, by giving it documents this repository does not contain.

The case that matters is the estate's fork fallback. A pull request from
a fork cannot obtain an Ubicloud runner, so the lane selects one by
expression. Written as a folded scalar it fits on two lines, and whether
those two lines become one value or two depends entirely on the indent
of the second. YAML folds a continuation at the same indent into a
space, and keeps the break when the continuation is indented further.
GitHub then evaluates whichever it is given, which is why nothing but a
reading of the value will find the broken form.
"""

from __future__ import annotations

import sys
import typing as typ
from pathlib import Path

import pytest

# The readers live in `scripts/`, which is not a package and is not on
# `sys.path` when pytest collects this file from the repository root.
# The bootstrap therefore has to run before the imports below, which is
# what E402 forbids and why each of them carries the suppression: the
# import order is not a preference here, it is the only order that
# resolves.
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from workflow_placement import (  # noqa: E402 - must follow the sys.path bootstrap above
    line_break_fault,
    runs_on_declarations,
)

#: The fork-fallback expression, as the estate writes it.
EXPRESSION: typ.Final[str] = (
    "${{ github.event.pull_request.head.repo.fork\n"
    "&& 'ubuntu-latest' || 'ubicloud-standard-2' }}"
)


def _document(runs_on: str) -> dict[str, str]:
    """Return a one-job workflow declaring the given `runs-on` block.

    Parameters
    ----------
    runs_on : str
        The lines of the declaration, already indented for a job key.

    Returns
    -------
    dict[str, str]
        A file name to text mapping, as the reader takes.
    """
    return {"lane.yml": "jobs:\n  build:\n" + runs_on + "    steps: []\n"}


def _folded(continuation_indent: int) -> dict[str, str]:
    """Return the fork-fallback lane with a chosen continuation indent.

    Parameters
    ----------
    continuation_indent : int
        Spaces before the expression's second line. Six keeps it at the
        first line's indent and folds; eight is one level deeper and
        keeps the break.

    Returns
    -------
    dict[str, str]
        A file name to text mapping.
    """
    first, second = EXPRESSION.split("\n")
    return _document(
        f"    runs-on: >-\n      {first}\n{' ' * continuation_indent}{second}\n"
    )


def _value(texts: dict[str, str]) -> object:
    """Return the single job's parsed `runs-on`.

    Parameters
    ----------
    texts : dict[str, str]
        A file name to text mapping.

    Returns
    -------
    object
        The parsed value.
    """
    (declaration,) = runs_on_declarations(texts)
    return declaration.value


def test_a_correctly_folded_expression_is_one_line() -> None:
    """The lane the estate prescribes must be accepted.

    Half of a rule is that it does not refuse what it exists to permit.
    A rule refusing every expression would pass the case below and make
    the fork fallback unadoptable, and nothing else here would say so.
    """
    value = _value(_folded(continuation_indent=6))

    assert "\n" not in str(value), "the continuation must fold into a space"
    assert line_break_fault(value) is None, (
        "the correctly folded fork-fallback lane must be accepted"
    )


def test_an_over_indented_continuation_is_refused() -> None:
    """One level deeper, and the value carries a newline.

    This is the whole defect. The file looks right, the workflow parses,
    the run is green, and the expression GitHub evaluates has a line
    break in the middle of it.
    """
    value = _value(_folded(continuation_indent=8))

    assert "\n" in str(value), (
        "the over-indented continuation must still keep its break; if it "
        "does not, this case no longer exercises what it was written for"
    )
    assert line_break_fault(value) is not None, (
        "a runs-on carrying a line break must be refused"
    )


@pytest.mark.parametrize(
    "runs_on",
    [
        pytest.param("    runs-on: ubuntu-latest\n", id="a-literal-label"),
        pytest.param("    runs-on: [self-hosted, linux]\n", id="a-list-of-labels"),
        pytest.param(
            "    runs-on:\n      group: estate\n      labels: [linux]\n",
            id="a-group-and-labels-mapping",
        ),
    ],
)
def test_every_shape_github_accepts_is_accepted(runs_on: str) -> None:
    """The rule is about line breaks, not about shape.

    `runs-on` may be a label, a list of labels or a `group`/`labels`
    mapping. A rule that required a string would refuse two of those and
    would read, from its failures, as though the workflow were wrong.
    """
    assert line_break_fault(_value(_document(runs_on))) is None, (
        "a shape GitHub accepts must not be refused"
    )


@pytest.mark.parametrize(
    ("runs_on", "expected"),
    [
        pytest.param(
            "    runs-on:\n      - self-hosted\n      - >-\n"
            "        ${{ github.event.pull_request.head.repo.fork\n"
            "          && 'a' || 'b' }}\n",
            True,
            id="inside-a-list",
        ),
        pytest.param(
            "    runs-on:\n      group: estate\n      labels:\n        - >-\n"
            "          ${{ github.event.pull_request.head.repo.fork\n"
            "            && 'a' || 'b' }}\n",
            True,
            id="inside-a-mapping",
        ),
    ],
)
def test_a_break_nested_in_a_shape_is_found(runs_on: str, expected: bool) -> None:
    """The defect can sit anywhere in the value, not only at the top.

    A rule reading only a top-level string would pass a list or mapping
    carrying the same broken expression, and the shapes are exactly
    where a lane with a fallback would put it.
    """
    assert (line_break_fault(_value(_document(runs_on))) is not None) is expected, (
        "a line break nested inside the value must be found"
    )
