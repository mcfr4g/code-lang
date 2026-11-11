"""Code- often stylised as "Code", an English-like interpreted language."""

from .runtime import Interpreter
from .syntax import CodeError

__all__ = ["CodeError", "Interpreter"]
__version__ = "0.2.0"
