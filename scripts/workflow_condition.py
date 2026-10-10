"""A small evaluator for the step conditions that guard the sccache readers.

It models only what those guards use: `&&`, `||`, `==`, `!=`, `!`,
parentheses, string literals, `always()` and the one `sccache-status` output,
and raises on anything else, so a contract built on it cannot pass vacuously
over syntax it silently misreads.

>>> evaluate("always() && " + STATUS_PATH + " != 'fallback'", "fallback")
False
>>> evaluate("always() && " + STATUS_PATH + " != 'fallback'", "started")
True
"""

from __future__ import annotations

import re
import typing as typ

#: The one context value the guards read; every other lookup is refused.
STATUS_PATH: typ.Final = "steps.setup-rust.outputs.sccache-status"
_TOKEN: typ.Final = re.compile(
    r"\s*(?:(&&|\|\||!=|==|!|\(|\))|'([^']*)'|(always\(\))|([A-Za-z][\w.-]*))"
)


class UnmodelledExpressionError(ValueError):
    """Raised for syntax the evaluator does not model, so it cannot pass vacuously."""


def _tokens(expression: str) -> list[tuple[str, str]]:
    """Split a step condition into `(kind, text)` tokens, refusing the unknown.

    Parameters
    ----------
    expression : str
        The `if:` text, optionally wrapped in `${{ }}`.

    Returns
    -------
    list[tuple[str, str]]
        Operator, string, call and context tokens, in order.

    Raises
    ------
    UnmodelledExpressionError
        If any character falls outside the modelled grammar.
    """
    text = expression.strip().removeprefix("${{").removesuffix("}}").strip()
    tokens: list[tuple[str, str]] = []
    position = 0
    while position < len(text):
        match = _TOKEN.match(text, position)
        if match is None:
            msg = f"unmodelled syntax at {text[position:]!r} in {expression!r}"
            raise UnmodelledExpressionError(msg)
        operator, string, call, name = match.groups()
        kind, value = next(
            (k, v)
            for k, v in (
                ("op", operator),
                ("str", string),
                ("call", call),
                ("ctx", name),
            )
            if v is not None
        )
        tokens.append((kind, value))
        position = match.end()
    return tokens


class _Parser:
    """Recursive-descent evaluator over the tokens of one step condition.

    Precedence, loosest first: `||`, `&&`, `==` and `!=`, then `!`, parentheses
    and operands. Every operand is read before it is combined, so a malformed
    right-hand side is refused even when the left one would decide the result.
    """

    def __init__(self, expression: str, status: str) -> None:
        """Tokenise `expression` and remember the status the context reads.

        Parameters
        ----------
        expression : str
            The `if:` text.
        status : str
            The value of `steps.setup-rust.outputs.sccache-status`.
        """
        self.expression = expression
        self.status = status
        self.tokens = _tokens(expression)
        self.cursor = 0

    def fail(self, reason: str) -> typ.NoReturn:
        """Raise for syntax the model does not cover.

        Parameters
        ----------
        reason : str
            What was unmodelled.

        Raises
        ------
        UnmodelledExpressionError
            Always.
        """
        msg = f"{reason} in {self.expression!r}"
        raise UnmodelledExpressionError(msg)

    def peek(self) -> tuple[str, str] | None:
        """Return the next token without consuming it, or `None` at the end."""
        return self.tokens[self.cursor] if self.cursor < len(self.tokens) else None

    def take(self) -> tuple[str, str]:
        """Consume and return the next token, refusing an early end."""
        token = self.peek()
        if token is None:
            self.fail("unexpected end")
        self.cursor += 1
        return token

    def context(self, name: str) -> str:
        """Return the value of a context lookup, refusing any but the status."""
        if name != STATUS_PATH:
            self.fail(f"unmodelled context {name!r}")
        return self.status

    def group(self) -> str | bool:
        """Read a parenthesised expression after its opening bracket."""
        inner = self.disjunction()
        if self.take() != ("op", ")"):
            self.fail("unbalanced parenthesis")
        return inner

    def operand(self) -> str | bool:
        """Read a string, `always()`, the status, a negation or a group."""
        kind, value = self.take()
        readers = {
            ("op", "!"): lambda: not self.operand(),
            ("op", "("): self.group,
        }
        if (kind, value) in readers:
            return readers[kind, value]()
        if kind == "str":
            return value
        if kind == "call":
            return True
        if kind == "ctx":
            return self.context(value)
        return self.fail(f"unmodelled {kind} {value!r}")

    def comparison(self) -> str | bool:
        """Read operands joined by `==` and `!=`, case-insensitively.

        GitHub coerces unlike types to numbers, which this model does not
        reproduce, so a boolean compared with a string is refused.
        """
        left = self.operand()
        while self.peek() in {("op", "=="), ("op", "!=")}:
            _, operator = self.take()
            right = self.operand()
            if isinstance(left, bool) != isinstance(right, bool):
                self.fail("mixed-type comparison")
            same = str(left).lower() == str(right).lower()
            left = same if operator == "==" else not same
        return left

    def conjunction(self) -> str | bool:
        """Read comparisons joined by `&&`."""
        left = self.comparison()
        while self.peek() == ("op", "&&"):
            self.take()
            right = self.comparison()
            left = left and right
        return left

    def disjunction(self) -> str | bool:
        """Read conjunctions joined by `||`."""
        left = self.conjunction()
        while self.peek() == ("op", "||"):
            self.take()
            right = self.conjunction()
            left = left or right
        return left

    def evaluate(self) -> bool:
        """Evaluate the whole expression, refusing anything left over."""
        result = self.disjunction()
        if self.peek() is not None:
            self.fail("trailing tokens")
        return bool(result)


def evaluate(expression: str, status: str) -> bool:
    """Evaluate a step condition with `always()` true and the given status.

    Handles `&&`, `||`, `==`, `!=`, `!`, parentheses, string literals,
    `always()` and the one `sccache-status` output. Comparison of strings is
    case-insensitive, as in GitHub's expression language. A boolean compared
    with a string is refused, because GitHub would coerce both to numbers.

    Parameters
    ----------
    expression : str
        The `if:` text.
    status : str
        The value of `steps.setup-rust.outputs.sccache-status`.

    Returns
    -------
    bool
        Whether the step runs.

    Raises
    ------
    UnmodelledExpressionError
        If the expression uses syntax or a context outside the model.
    """
    return _Parser(expression, status).evaluate()
