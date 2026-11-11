"""Evaluate Code programs using native values and ordered collections."""

from __future__ import annotations

from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import dataclass
import json
import math
import sys
from typing import TextIO

from . import syntax


@dataclass
class Item:
    value: Value
    name: str | None


class Collection:
    def __init__(self):
        self._items: list[Item] = []
        self._names: dict[str, Item] = {}
        self._active_loops = 0

    def __len__(self) -> int:
        return len(self._items)

    def _at(self, position: int, line: int) -> Item:
        if not 1 <= position <= len(self):
            raise syntax.CodeError(
                line, f"Position {position} is out of range (collection has {len(self)} items)"
            )
        return self._items[position - 1]

    def _named(self, name: str, line: int) -> Item:
        if name not in self._names:
            raise syntax.CodeError(line, f"No item named {name!r}")
        return self._names[name]

    def _check_resize(self, line: int) -> None:
        if self._active_loops:
            raise syntax.CodeError(line, "Cannot change a collection's size during for each")

    def at(self, position: int, line: int) -> Value:
        return self._at(position, line).value

    def named(self, name: str, line: int) -> Value:
        return self._named(name, line).value

    def append(self, value: Value, line: int, name: str | None = None) -> None:
        self._check_resize(line)
        item = Item(value, name)
        if name is not None:
            if name in self._names:
                raise syntax.CodeError(line, f"Duplicate item name {name!r}")
            self._names[name] = item
        self._items.append(item)

    def update(self, name: str, value: Value, line: int) -> None:
        self._named(name, line).value = value

    def remove(self, position: int, line: int) -> None:
        self._check_resize(line)
        item = self._at(position, line)
        del self._items[position - 1]
        if item.name is not None:
            del self._names[item.name]

    @contextmanager
    def iterating(self) -> Iterator[Iterator[Value]]:
        self._active_loops += 1
        try:
            yield (item.value for item in self._items)
        finally:
            self._active_loops -= 1

    def display(self, ancestors: set[Collection]) -> str:
        if self in ancestors:
            return "<cycle>"
        ancestors.add(self)
        try:
            parts = []
            for item in self._items:
                text = display(item.value, ancestors)
                if item.name is not None:
                    text += f" named {item.name}"
                parts.append(text)
            return "[" + ", ".join(parts) + "]"
        finally:
            ancestors.remove(self)


Value = int | float | bool | str | Collection


def display(value: Value, ancestors: set[Collection] | None = None) -> str:
    if isinstance(value, Collection):
        return value.display(set() if ancestors is None else ancestors)
    if isinstance(value, str) and ancestors is not None:
        return json.dumps(value, ensure_ascii=False)
    if isinstance(value, bool):
        return "true" if value else "false"
    return str(value)


class Interpreter:
    def __init__(self, output: TextIO | None = None, *, input_stream: TextIO | None = None):
        self.output = output
        self.input_stream = input_stream
        self.variables: dict[str, Value] = {}

    def run(self, source: str) -> None:
        self.variables.clear()
        program = syntax.Parser(source).parse()
        self.execute(program)

    def execute(self, statements: list[syntax.Statement]) -> None:
        for statement in statements:
            try:
                self.execute_statement(statement)
            except RecursionError as error:
                raise syntax.CodeError(statement.line, "Program is nested too deeply") from error

    def execute_statement(self, statement: syntax.Statement) -> None:
        line = statement.line
        match statement:
            case syntax.Assignment(variable=variable, value=value, declare=declare):
                name = variable.name
                if declare and name in self.variables:
                    raise syntax.CodeError(line, f"Variable {name!r} already exists")
                if not declare and name not in self.variables:
                    raise syntax.CodeError(line, f"Unknown variable {name!r}")
                self.variables[name] = self.evaluate(value)
            case syntax.Arithmetic(variable=variable, amount=amount, subtract=subtract):
                current = self.number(variable)
                change = self.number(amount)
                try:
                    result = current - change if subtract else current + change
                except OverflowError as error:
                    raise syntax.CodeError(line, "Number is too large") from error
                if isinstance(result, float) and not math.isfinite(result):
                    raise syntax.CodeError(line, "Number is too large")
                self.variables[variable.name] = result
            case syntax.Output(value=value, numeric=numeric, newline=newline):
                result = self.number(value) if numeric else self.evaluate(value)
                try:
                    text = display(result)
                except ValueError as error:
                    raise syntax.CodeError(line, "Number is too large to print") from error
                print(text, end="\n" if newline else "", file=self.output)
            case syntax.Ask(prompt=prompt, variable=variable):
                if variable.name in self.variables:
                    raise syntax.CodeError(line, f"Variable {variable.name!r} already exists")
                message = self.evaluate(prompt)
                if not isinstance(message, str):
                    raise syntax.CodeError(prompt.line, "Expected text for the prompt")
                print(message, file=self.output, flush=True)
                stream = sys.stdin if self.input_stream is None else self.input_stream
                try:
                    response = stream.readline()
                except (OSError, UnicodeError) as error:
                    raise syntax.CodeError(line, f"Input error: {error}") from error
                if response == "":
                    raise syntax.CodeError(line, "No input available")
                if response.endswith("\r\n"):
                    response = response[:-2]
                else:
                    response = response.removesuffix("\n")
                self.variables[variable.name] = response
            case syntax.Count(collection=collection):
                print(len(self.collection(collection)), file=self.output)
            case syntax.Repeat(count=count, body=body):
                times = self.evaluate(count)
                if type(times) is not int or times < 0:
                    raise syntax.CodeError(line, "Loop count must be a non-negative whole number")
                for _ in range(times):
                    self.execute(body)
            case syntax.While(condition=condition, body=body):
                while self.condition(condition):
                    self.execute(body)
            case syntax.If(condition=condition, body=body, alternative=alternative):
                self.execute(body if self.condition(condition) else alternative)
            case syntax.ForEach(variable=variable, collection=collection, body=body):
                self.for_each(variable.name, self.collection(collection), body)
            case syntax.Put(value=value, target=target):
                result = self.evaluate(value)
                if isinstance(target, syntax.Named):
                    self.collection(target.collection).update(target.name, result, line)
                else:
                    self.collection(target).append(result, line)
            case syntax.Remove(target=target):
                self.collection(target.collection).remove(target.index, line)
            case _:
                raise TypeError(f"Unknown statement type {type(statement).__name__}")

    def for_each(self, name: str, collection: Collection, body: list[syntax.Statement]) -> None:
        previous = self.variables.get(name)
        try:
            with collection.iterating() as values:
                for value in values:
                    self.variables[name] = value
                    self.execute(body)
        finally:
            if previous is None:
                self.variables.pop(name, None)
            else:
                self.variables[name] = previous

    def evaluate(self, expression: syntax.Expression) -> Value:
        line = expression.line
        match expression:
            case syntax.Literal(value=value):
                return value
            case syntax.Variable(name=name):
                if name not in self.variables:
                    raise syntax.CodeError(line, f"Unknown variable {name!r}")
                return self.variables[name]
            case syntax.Group(value=value):
                return self.evaluate(value)
            case syntax.Comparison():
                return self.compare(expression)
            case syntax.Position(index=index, collection=collection):
                return self.collection(collection).at(index, line)
            case syntax.Named(collection=collection, name=name):
                return self.collection(collection).named(name, line)
            case syntax.CollectionLiteral(entries=entries):
                result = Collection()
                for entry in entries:
                    result.append(self.evaluate(entry.value), entry.value.line, entry.name)
                return result
            case _:
                raise TypeError(f"Unknown expression type {type(expression).__name__}")

    def condition(self, expression: syntax.Expression) -> bool:
        value = self.evaluate(expression)
        if not isinstance(value, bool):
            raise syntax.CodeError(expression.line, "Expected true or false as the condition")
        return value

    def compare(self, comparison: syntax.Comparison) -> bool:
        operator = comparison.operator
        if operator is syntax.ComparisonOperator.LESS:
            return self.number(comparison.left) < self.number(comparison.right)
        if operator is syntax.ComparisonOperator.GREATER:
            return self.number(comparison.left) > self.number(comparison.right)

        left = self.evaluate(comparison.left)
        right = self.evaluate(comparison.right)
        if isinstance(left, Collection) or isinstance(right, Collection):
            raise syntax.CodeError(comparison.line, "Equality requires numbers, text, or truth values")
        both_numbers = type(left) in (int, float) and type(right) in (int, float)
        equal = (both_numbers or type(left) is type(right)) and left == right
        return not equal if operator is syntax.ComparisonOperator.NOT_EQUAL else equal

    def number(self, expression: syntax.Expression) -> int | float:
        value = self.evaluate(expression)
        if type(value) not in (int, float):
            raise syntax.CodeError(expression.line, "Expected a number")
        return value

    def collection(self, expression: syntax.Expression) -> Collection:
        value = self.evaluate(expression)
        if not isinstance(value, Collection):
            raise syntax.CodeError(expression.line, "Expected a collection")
        return value
