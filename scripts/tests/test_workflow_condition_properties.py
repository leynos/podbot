"""Property test: the condition evaluator agrees with an independent model.

Hypothesis builds condition trees from the evaluator's whole grammar (`always()`,
status comparisons, `!`, `&&`, `||`) and renders each to `if:` text. The expected
value is computed from the tree itself, never from the text, so a precedence or
association slip in the parser shows up as a disagreement.
"""

from __future__ import annotations

import typing as typ

from hypothesis import given
from hypothesis import strategies as st
from workflow_condition import STATUS_PATH, evaluate

Tree = typ.Union[
    tuple[typ.Literal["always"]],
    tuple[typ.Literal["cmp"], str, str],
    tuple[typ.Literal["not"], "Tree"],
    tuple[typ.Literal["and", "or"], "Tree", "Tree"],
]

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
    kind = tree[0]
    if kind == "always":
        return "always()"
    if kind == "cmp":
        return f"{STATUS_PATH} {tree[1]} '{tree[2]}'"
    if kind == "not":
        return f"!({render(tree[1])})"
    joiner = "&&" if kind == "and" else "||"
    return f"({render(tree[1])}) {joiner} ({render(tree[2])})"


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
    kind = tree[0]
    if kind == "always":
        return True
    if kind == "cmp":
        same = status.lower() == tree[2].lower()
        return same if tree[1] == "==" else not same
    if kind == "not":
        return not expected(tree[1], status)
    left, right = expected(tree[1], status), expected(tree[2], status)
    return (left and right) if kind == "and" else (left or right)


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
    assert evaluate(render(tree), status) is expected(tree, status)
