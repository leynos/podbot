"""Decide whether a workflow `run:` block is one unconditional command.

A contract that requires a lane to *run* a command cannot match the command as
a substring of the script: `echo make coverage` and `# make coverage` both
contain it and run nothing. Splitting the script into simple commands is not
enough either, because a list decides whether its members run at all:
`false && make coverage`, `true || make coverage`, `exit 0; make coverage` and
`make coverage &` all contain the command as a simple command, and none runs it
to completion.

So the reader asks a narrower question with a certain answer: is the script
exactly one simple command, with no list or pipeline operator anywhere, whose
words begin with the required ones? A step written that way runs the command
unconditionally under the step's own shell, and a lane requiring it has to give
it a dedicated step.

Scope and re-use: this is a reader for requirements ("the lane runs X"). A
prohibition ("no lane mentions X") should stay a substring test, because there
over-matching is the safe direction and this reader deliberately under-matches.
It reads line continuations, comments (at the start of a word, as the shell
does) and leading variable assignments; anything else, a subshell included, is
refused rather than interpreted, and a line it cannot tokenize makes the whole
script unreadable.

Run via ``make test-workflow-contracts``.
"""

import itertools
import re
import shlex
import typing as typ

#: The characters shlex groups into operator tokens.
_OPERATOR_CHARACTERS: typ.Final[str] = ";&|()"

#: A leading `NAME=value` word, which sets the environment of the command
#: after it rather than being the command.
_ASSIGNMENT: typ.Final[re.Pattern[str]] = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")


def _is_operator(token: str) -> bool:
    """Return whether a token is a list, pipeline or subshell operator."""
    return bool(token) and set(token) <= set(_OPERATOR_CHARACTERS)


def _line_tokens(line: str) -> list[str]:
    """Return one line's words and operators, stopping at a comment."""
    lexer = shlex.shlex(line, posix=True, punctuation_chars=_OPERATOR_CHARACTERS)
    lexer.whitespace_split = True
    # shlex would end a word at a `#` anywhere; the shell starts a comment only
    # at the start of a word, so comments are cut here instead.
    lexer.commenters = ""
    try:
        tokens = list(lexer)
    except ValueError:
        # An unbalanced quote. A lone operator stands in for the unreadable
        # line, so the script is refused rather than read without it.
        return [";"]
    cut = next(
        (index for index, token in enumerate(tokens) if token.startswith("#")),
        len(tokens),
    )
    return tokens[:cut]


def runs_unconditionally(script: str, command: str) -> bool:
    """Return whether a script is exactly one command beginning as given.

    Parameters
    ----------
    script : str
        The body of a `run:` step.
    command : str
        The command's leading words, such as `make coverage`.

    Returns
    -------
    bool
        True when the script, once comments and blank lines are removed, is a
        single simple command with no operator, whose words after any leading
        variable assignments begin with those of `command`.

    >>> runs_unconditionally("RUSTFLAGS=-Dwarnings make coverage", "make coverage")
    True
    >>> runs_unconditionally("false && make coverage", "make coverage")
    False
    """
    joined = script.replace("\\\n", " ")
    lines = [tokens for line in joined.splitlines() if (tokens := _line_tokens(line))]
    if len(lines) != 1 or any(_is_operator(token) for token in lines[0]):
        return False
    words = list(itertools.dropwhile(_ASSIGNMENT.match, lines[0]))
    expected = command.split()
    return words[: len(expected)] == expected
