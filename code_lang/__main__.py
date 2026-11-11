"""Run a UTF-8 Code program from the command line."""

import argparse
from pathlib import Path
import sys

from . import CodeError, Interpreter, __version__


def main(argv: list[str] | None = None, *, prog: str = "python3 -m code_lang") -> int:
    parser = argparse.ArgumentParser(prog=prog, description="Run a Code program.")
    parser.add_argument("--version", action="version", version=f"Code {__version__}")
    parser.add_argument("file", type=Path, help="UTF-8 .code file to run")
    args = parser.parse_args(argv)

    try:
        Interpreter().run(args.file.read_text(encoding="utf-8"))
    except (CodeError, OSError, UnicodeError) as error:
        print(f"{args.file}: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
