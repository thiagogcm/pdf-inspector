#!/usr/bin/env python3
"""Fail if pdf_inspector.h declares a symbol that examples/java/filter.txt
does not pass through to jextract.

filter.txt is a hand-curated `--include-*` allowlist (see its own header
comment, and docs/java-bindings.md section 2, for how it is derived from
`jextract --dump-includes`). Nothing here regenerates that curation --
that still needs jextract and a human trimming out system-header noise.
This only checks the one-directional invariant that actually matters day to
day: every function and constant pdf_inspector.h exports has a matching
`--include-function`/`--include-constant` line, so a newly added C symbol
cannot silently drop out of the Java bindings with no build failure
anywhere (the failure mode docs/java-bindings.md section 10 exists to catch,
but which nothing wired into CI actually caught before this script).
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HEADER = ROOT / "pdf_inspector.h"
FILTER = ROOT / "examples" / "java" / "filter.txt"

BLOCK_COMMENT = re.compile(r"/\*.*?\*/", re.DOTALL)
LINE_COMMENT = re.compile(r"//[^\n]*")
FUNCTION_NAME = re.compile(r"\b(pdf_inspector_[A-Za-z0-9_]+)\s*\(")
# Matches both `#define NAME VALUE` macros and `NAME = <value>,` enum
# variants once comments are stripped -- the only two shapes filter.txt's
# `--include-constant` lines need to cover. Requires a value after the name
# so the `#ifndef`/`#define` include guard (no value) is not mistaken for a
# constant worth exposing to Java.
MACRO_NAME = re.compile(r"^#define[ \t]+([A-Za-z0-9_]+)[ \t]+\S", re.MULTILINE)
ENUM_VARIANT_NAME = re.compile(r"^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=", re.MULTILINE)


def strip_comments(text: str) -> str:
    text = BLOCK_COMMENT.sub(" ", text)
    text = LINE_COMMENT.sub(" ", text)
    return text


def filter_entries(prefix: str) -> set[str]:
    needle = f"--include-{prefix} "
    names = set()
    for line in FILTER.read_text().splitlines():
        line = line.strip()
        if line.startswith(needle):
            names.add(line[len(needle) :].strip())
    return names


def main() -> int:
    header_text = strip_comments(HEADER.read_text())

    declared_functions = set(FUNCTION_NAME.findall(header_text))
    declared_constants = set(MACRO_NAME.findall(header_text)) | set(
        ENUM_VARIANT_NAME.findall(header_text)
    )

    filtered_functions = filter_entries("function")
    filtered_constants = filter_entries("constant")

    missing_functions = sorted(declared_functions - filtered_functions)
    missing_constants = sorted(declared_constants - filtered_constants)

    if not missing_functions and not missing_constants:
        print("examples/java/filter.txt covers every symbol in pdf_inspector.h.")
        return 0

    if missing_functions:
        print(
            "Functions declared in pdf_inspector.h but missing an "
            "'--include-function' line in examples/java/filter.txt:",
            file=sys.stderr,
        )
        for name in missing_functions:
            print(f"  {name}", file=sys.stderr)
    if missing_constants:
        print(
            "Constants declared in pdf_inspector.h but missing an "
            "'--include-constant' line in examples/java/filter.txt:",
            file=sys.stderr,
        )
        for name in missing_constants:
            print(f"  {name}", file=sys.stderr)
    print(
        "\nRegenerate with jextract's --dump-includes against pdf_inspector.h "
        "and fold in the new entries (see docs/java-bindings.md section 2).",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
