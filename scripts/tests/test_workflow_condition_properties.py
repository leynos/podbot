"""Property test: the condition evaluator agrees with an independent model.

Hypothesis builds condition trees from the evaluator's whole grammar (`always()`,
status comparisons, `!`, `&&`, `||`) and renders each to `if:` text. The expected
value is computed from the tree itself, never from the text, so a precedence or
association slip in the parser shows up as a disagreement.
"""

from __future__ import annotations

import typing as typ

import pytest
from hypothesis import given
from hypothesis import strategies as st
from workflow_condition import STATUS_PATH, evaluate

type Tree = (
    tuple[typ.Literal["always"]]
    | tuple[typ.Literal["cmp"], str, str]
    | tuple[typ.Literal["not"], Tree]
    | tuple[typ.Literal["and", "or"], Tree, Tree]
)

FALLBACK: typ.Final = f"{STATUS_PATH} == 'fallback'"
STARTED: typ.Final = f"{STATUS_PATH} == 'started'"
OTHER: typ.Final = f"{STATUS_PATH} == 'other'"
LITERALS: typ.Final = ("fallback", "Fallback", "started", "")

leaves = st.one_of(
    st.just(("always",)),
    st.tuples(st.just("cmp"), st.sampled_from(("==", "!=")), st.sampled_from(LITERALS)),
)
trees: st.SearchStrategy[Tree] = st.recursive(
    leaves,
    lambda inner: st.one_of(
        st.tuples(st.just("not"), inner),
        st.tuples(st.sampled_from(("and", "or")), inner, inner),
    ),
    max_leaves=8,
)


def render(tree: Tree) -> str:
    """Render a tree as fully parenthesised `if:` text.

    Parameters
    ----------
    tree : Tree
        The condition tree.

    Returns
    -------
    str
        The expression, with explicit brackets around every compound node.
    """
    match tree:
        case ("always",):
            return "always()"
        case ("cmp", operator, literal):
            return f"{STATUS_PATH} {operator} '{literal}'"
        case ("not", inner):
            return f"!({render(inner)})"
        case ("and", left, right):
            return f"({render(left)}) && ({render(right)})"
        case ("or", left, right):
            return f"({render(left)}) || ({render(right)})"
    raise AssertionError(tree)


def expected(tree: Tree, status: str) -> bool:
    """Compute the tree's value directly, independently of the parser.

    Parameters
    ----------
    tree : Tree
        The condition tree.
    status : str
        The `sccache-status` value.

    Returns
    -------
    bool
        Whether the step would run.
    """
    match tree:
        case ("always",):
            return True
        case ("cmp", "==", literal):
            return status.lower() == literal.lower()
        case ("cmp", _, literal):
            return status.lower() != literal.lower()
        case ("not", inner):
            return not expected(inner, status)
        case ("and", left, right):
            return expected(left, status) and expected(right, status)
        case ("or", left, right):
            return expected(left, status) or expected(right, status)
    raise AssertionError(tree)


@given(tree=trees, status=st.sampled_from(LITERALS))
def test_evaluate_agrees_with_an_independent_model(tree: Tree, status: str) -> None:
    """Match the tree model for every supported tree and status.

    Parameters
    ----------
    tree : Tree
        A generated condition tree.
    status : str
        A generated `sccache-status` value.
    """
    text = render(tree)
    assert evaluate(text, status) is expected(tree, status), (
        f"{text!r} with status {status!r} disagrees with the tree model"
    )


@pytest.mark.parametrize(
    ("condition", "status", "runs"),
    [
        pytest.param(
            f"{FALLBACK} || {STARTED} && {OTHER}",
            "fallback",
            True,
            id="and-binds-tighter",
        ),
        pytest.param(
            f"{FALLBACK} && {STARTED} || always()", "started", True, id="or-is-loosest"
        ),
        pytest.param(
            f"!({FALLBACK}) && {STARTED}", "fallback", False, id="not-binds-tightest"
        ),
    ],
)
def test_unbracketed_mixed_operators_follow_github_precedence(
    condition: str, status: str, runs: bool
) -> None:
    """Hold handwritten expectations for `!`, `&&` and `||` without brackets.

    The property test brackets every node, so it cannot see a precedence slip.
    These cases state the GitHub precedence (`!`, then `&&`, then `||`) directly.

    Parameters
    ----------
    condition : str
        An unbracketed `if:` mixing operators.
    status : str
        The `sccache-status` value.
    runs : bool
        The result GitHub gives.
    """
    assert evaluate(condition, status) is runs
