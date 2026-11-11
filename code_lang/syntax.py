"""Tokenize Code source and parse it into a line-numbered syntax tree."""

from dataclasses import dataclass
from enum import Enum, auto
import json
import math
import re


class CodeError(Exception):
    def __init__(self, line: int, message: str):
        self.line = line
        self.message = message
        super().__init__(f"Line {line}: {message}")


@dataclass(frozen=True)
class Token:
    kind: str
    text: str
    line: int
    text_value: str | None = None


TOKEN = re.compile(
    r'(?P<text>"(?:[^"\\]|\\.)*")'
    r"|(?P<number>-?[0-9]+(?:\.[0-9]+)?)"
    r"|(?P<word>[A-Za-z_][A-Za-z_0-9]*)"
    r"|(?P<symbol>[:,?()])"
    r"|(?P<space>[ \t]+)"
    r"|(?P<invalid>.)"
)
ORDINALS = {
    word: index
    for index, word in enumerate(
        "first second third fourth fifth sixth seventh eighth ninth tenth".split(), 1
    )
}
TRUTH = {"true": True, "false": False}
ARROWS = {"up": "⬆️", "down": "⬇️", "left": "⬅️", "right": "➡️"}
FORBIDDEN_CHARACTER = "§"
FORBIDDEN_CHARACTER_MESSAGE = f"We must never speak of {FORBIDDEN_CHARACTER}!"
SECTION_SIGN_RUN = re.compile(f"{FORBIDDEN_CHARACTER}+")


def decode_text(text: str, line: int) -> str:
    value = json.loads(text)
    value.encode("utf-8")

    def replace_run(match: re.Match[str]) -> str:
        if len(match.group()) != 7:
            raise CodeError(line, FORBIDDEN_CHARACTER_MESSAGE)
        return "🐍"

    return SECTION_SIGN_RUN.sub(replace_run, value)


def tokenize(source: str) -> list[Token]:
    tokens = []
    # A forbidden section sign takes precedence over ordinary syntax errors.
    first_error = None
    comment_start = None
    lines = source.split("\n")
    for number, raw in enumerate(lines, 1):
        line = raw.strip()
        if (
            comment_start is not None
            or not line
            or line.startswith(("note:", "comment on one line:"))
        ):
            if FORBIDDEN_CHARACTER in line:
                raise CodeError(number, FORBIDDEN_CHARACTER_MESSAGE)
            if line == "end of comment":
                comment_start = None
            continue
        if line == "comment:":
            comment_start = number
            continue
        if line == "end of comment":
            first_error = first_error or CodeError(number, "Unexpected end of comment")
            continue
        for match in TOKEN.finditer(line):
            kind, text = match.lastgroup, match.group()
            if kind == "space":
                continue
            if kind == "invalid":
                if text == FORBIDDEN_CHARACTER:
                    raise CodeError(number, FORBIDDEN_CHARACTER_MESSAGE)
                message = "Unterminated text" if text == '"' else f"Unexpected character {text!r}"
                first_error = first_error or CodeError(number, message)
                continue
            text_value = None
            if kind == "text":
                try:
                    text_value = decode_text(text, number)
                except ValueError as error:
                    if FORBIDDEN_CHARACTER in text:
                        raise CodeError(number, FORBIDDEN_CHARACTER_MESSAGE) from error
                    first_error = first_error or CodeError(number, "Invalid text literal or escape")
                    continue
            tokens.append(Token(kind, text, number, text_value))
        tokens.append(Token("newline", "", number))
    if comment_start is not None:
        first_error = first_error or CodeError(comment_start, "Missing end of comment")
    if first_error is not None:
        raise first_error
    tokens.append(Token("eof", "", len(lines)))
    return tokens


@dataclass(frozen=True)
class Expression:
    line: int


@dataclass(frozen=True)
class Literal(Expression):
    value: int | float | bool | str


@dataclass(frozen=True)
class Variable(Expression):
    name: str


@dataclass(frozen=True)
class Group(Expression):
    value: Expression


class ComparisonOperator(Enum):
    LESS = auto()
    GREATER = auto()
    EQUAL = auto()
    NOT_EQUAL = auto()


COMPARISON_PHRASES = {
    ("less", "than"): ComparisonOperator.LESS,
    ("greater", "than"): ComparisonOperator.GREATER,
    ("equal", "to"): ComparisonOperator.EQUAL,
    ("not", "equal", "to"): ComparisonOperator.NOT_EQUAL,
}


@dataclass(frozen=True)
class Comparison(Expression):
    left: Expression
    operator: ComparisonOperator
    right: Expression


@dataclass(frozen=True)
class Position(Expression):
    index: int
    collection: Expression


@dataclass(frozen=True)
class Named(Expression):
    collection: Expression
    name: str


@dataclass(frozen=True)
class Entry:
    value: Expression
    name: str | None


@dataclass(frozen=True)
class CollectionLiteral(Expression):
    entries: list[Entry]


@dataclass(frozen=True)
class Statement:
    line: int


@dataclass(frozen=True)
class Assignment(Statement):
    variable: Variable
    value: Expression
    declare: bool


@dataclass(frozen=True)
class Arithmetic(Statement):
    variable: Variable
    amount: Expression
    subtract: bool


@dataclass(frozen=True)
class Output(Statement):
    value: Expression
    numeric: bool = False
    newline: bool = True


@dataclass(frozen=True)
class Ask(Statement):
    prompt: Expression
    variable: Variable


@dataclass(frozen=True)
class Count(Statement):
    collection: Expression


@dataclass(frozen=True)
class Repeat(Statement):
    count: Expression
    body: list[Statement]


@dataclass(frozen=True)
class While(Statement):
    condition: Expression
    body: list[Statement]


@dataclass(frozen=True)
class If(Statement):
    condition: Expression
    body: list[Statement]
    alternative: list[Statement]


@dataclass(frozen=True)
class ForEach(Statement):
    variable: Variable
    collection: Expression
    body: list[Statement]


@dataclass(frozen=True)
class Put(Statement):
    value: Expression
    target: Expression


@dataclass(frozen=True)
class Remove(Statement):
    target: Position


class Parser:
    def __init__(self, source: str):
        self.tokens = tokenize(source)
        self.index = 0

    def peek(self, ahead: int = 0) -> Token:
        return self.tokens[min(self.index + ahead, len(self.tokens) - 1)]

    def advance(self) -> Token:
        token = self.peek()
        self.index += 1
        return token

    def accept(self, *words: str) -> bool:
        if all(self.peek(i).text == word for i, word in enumerate(words)):
            self.index += len(words)
            return True
        return False

    def expect(self, word: str) -> None:
        if not self.accept(word):
            raise CodeError(self.peek().line, f"Expected {word!r}")

    def end_line(self) -> None:
        if self.peek().kind != "newline":
            raise CodeError(self.peek().line, f"Unexpected {self.peek().text!r} at end of line")
        self.advance()

    def identifier(self, description: str) -> Token:
        if self.peek().kind != "word":
            raise CodeError(self.peek().line, f"Expected {description}")
        return self.advance()

    def variable(self) -> Variable:
        token = self.identifier("a variable name")
        if token.text in TRUTH:
            raise CodeError(token.line, f"{token.text!r} is a truth value")
        return Variable(token.line, token.text)

    def parse(self) -> list[Statement]:
        try:
            return self.block()
        except RecursionError as error:
            raise CodeError(self.peek().line, "Program is nested too deeply") from error

    def block(
        self, opening: Token | None = None, *, allow_otherwise: bool = False
    ) -> list[Statement]:
        statements = []
        while self.peek().kind != "eof":
            if self.peek().text == "ok":
                if opening is None:
                    raise CodeError(self.peek().line, "Unexpected ok")
                return statements
            if self.peek().text == "otherwise" and self.peek(1).text != "but":
                if allow_otherwise:
                    return statements
                raise CodeError(self.peek().line, "Unexpected otherwise")
            statements.append(self.statement())
            self.end_line()
        if opening is not None:
            block_name = "if" if opening.text == "if" else "loop"
            raise CodeError(opening.line, f"Missing ok for {block_name}")
        return statements

    def closed_block(self, opening: Token) -> list[Statement]:
        body = self.block(opening)
        self.expect("ok")
        return body

    def statement(self) -> Statement:
        start = self.peek()
        line = start.line

        match self.advance().text:
            case "remember":
                if (
                    self.peek().kind == "word"
                    and self.peek().text not in TRUTH
                    and self.peek(1).text == "as"
                ):
                    variable = self.variable()
                    self.expect("as")
                    return Assignment(line, variable, self.expression(), declare=True)
                value = self.expression()
                self.expect("as")
                return Assignment(line, self.variable(), value, declare=True)
            case "set":
                variable = self.variable()
                self.expect("to")
                return Assignment(line, variable, self.expression(), declare=False)
            case "add":
                amount = Literal(line, 1) if self.accept("one", "to") else None
                if amount is None:
                    amount = self.expression()
                    self.expect("to")
                return Arithmetic(line, self.variable(), amount, subtract=False)
            case "take":
                amount = self.expression()
                self.expect("from")
                return Arithmetic(line, self.variable(), amount, subtract=True)
            case "say":
                newline = not self.accept("on", "one", "line")
                return Output(line, self.expression(), newline=newline)
            case "ask":
                prompt = self.expression()
                self.expect("and")
                self.expect("remember")
                self.expect("as")
                return Ask(line, prompt, self.variable())
            case "what":
                self.expect("is")
                value = self.expression()
                self.expect("?")
                return Output(line, value)
            case "how":
                self.expect("many")
                if self.accept("are", "in"):
                    collection = self.expression()
                    self.expect("?")
                    return Count(line, collection)
                variable = self.variable()
                self.expect("?")
                return Output(line, variable, numeric=True)
            case "do":
                self.expect("this")
                count = self.expression()
                self.expect("times")
                self.expect(":")
                self.end_line()
                return Repeat(line, count, self.closed_block(start))
            case "while":
                condition = self.condition_header()
                return While(line, condition, self.closed_block(start))
            case "if":
                condition = self.condition_header()
                body = self.block(start, allow_otherwise=True)
                alternative = []
                if self.accept("otherwise"):
                    self.expect(":")
                    self.end_line()
                    alternative = self.block(start)
                self.expect("ok")
                return If(line, condition, body, alternative)
            case "for":
                self.expect("each")
                variable = self.variable()
                self.expect("in")
                collection = self.expression()
                self.expect(":")
                self.end_line()
                return ForEach(line, variable, collection, self.closed_block(start))
            case "put":
                value = self.expression()
                self.expect("into")
                return Put(line, value, self.expression())
            case "remove":
                target = self.expression()
                if not isinstance(target, Position):
                    raise CodeError(line, "Remove requires a position")
                return Remove(line, target)
            case _:
                if start.kind == "word" and self.accept("but"):
                    if start.text in TRUTH:
                        raise CodeError(line, "Expected a variable before 'but'")
                    variable = Variable(line, start.text)
                    amount = self.expression()
                    if self.accept("more"):
                        return Arithmetic(line, variable, amount, subtract=False)
                    if self.accept("less"):
                        return Arithmetic(line, variable, amount, subtract=True)
                    raise CodeError(self.peek().line, "Expected 'more' or 'less'")
                raise CodeError(line, f"Unknown instruction {start.text!r}")

    def condition_header(self) -> Expression:
        condition = self.expression()
        if self.accept("is"):
            for phrase, operator in COMPARISON_PHRASES.items():
                if self.accept(*phrase):
                    condition = Comparison(condition.line, condition, operator, self.expression())
                    break
            else:
                line = self.peek().line
                right = self.expression()
                if self.peek().text != ":":
                    raise CodeError(line, "Expected a comparison after 'is'")
                condition = Comparison(condition.line, condition, ComparisonOperator.EQUAL, right)
        self.expect(":")
        self.end_line()
        return condition

    def expression(self, *, entry: bool = False) -> Expression:
        token = self.advance()
        if token.text == "all" and self.accept("of", "these", ":"):
            return self.collection(token.line, entry=entry)
        if token.text == "the" and self.peek(1).text == "of":
            ordinal = self.advance()
            if ordinal.text not in ORDINALS:
                raise CodeError(ordinal.line, f"Unknown ordinal {ordinal.text!r}")
            self.expect("of")
            return Position(token.line, ORDINALS[ordinal.text], self.expression(entry=entry))
        if token.text == "position" and self.peek().kind == "number":
            number = self.advance()
            index = self.number(number)
            if type(index) is not int:
                raise CodeError(number.line, "Expected a whole number for the position")
            self.expect("of")
            return Position(token.line, index, self.expression(entry=entry))

        value = self.primary(token)
        while not entry and self.accept("named"):
            name = self.identifier("an item name")
            value = Named(name.line, value, name.text)
        return value

    def primary(self, token: Token) -> Expression:
        if token.text == "arrow" and self.accept("("):
            direction = self.advance()
            if direction.text not in ARROWS:
                raise CodeError(direction.line, "Expected arrow direction up, down, left, or right")
            self.expect(")")
            return Literal(token.line, ARROWS[direction.text])
        if token.text == "(":
            value = self.expression()
            self.expect(")")
            return Group(token.line, value)
        if token.kind == "number":
            return Literal(token.line, self.number(token))
        if token.kind == "text":
            assert token.text_value is not None
            return Literal(token.line, token.text_value)
        if token.text in TRUTH:
            return Literal(token.line, TRUTH[token.text])
        if token.kind == "word":
            return Variable(token.line, token.text)
        raise CodeError(token.line, "Expected a value")

    @staticmethod
    def number(token: Token) -> int | float:
        try:
            if "." not in token.text:
                return int(token.text)
            value = float(token.text)
            if math.isfinite(value):
                return value
        except ValueError as error:
            raise CodeError(token.line, "Number literal is too large") from error
        raise CodeError(token.line, "Number literal is too large")

    def collection(self, line: int, *, entry: bool) -> CollectionLiteral:
        entries = []
        if self.peek().kind == "newline":
            self.end_line()
            while not self.accept("ok"):
                if self.peek().kind == "eof":
                    raise CodeError(line, "Missing ok for collection")
                entries.append(self.entry())
                self.accept(",")
                self.end_line()
        elif self.peek().text != ")":
            if entry:
                raise CodeError(line, "Wrap an inline nested collection in parentheses")
            entries.append(self.entry())
            while self.accept(","):
                if self.peek().kind == "newline" or self.peek().text == ")":
                    break
                entries.append(self.entry())
        return CollectionLiteral(line, entries)

    def entry(self) -> Entry:
        value = self.expression(entry=True)
        name = self.identifier("an item name").text if self.accept("named") else None
        return Entry(value, name)
