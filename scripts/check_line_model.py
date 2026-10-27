"""Check crap's model of Python source against coverage.py and radon.

For every file given, compare what `crap_python` finds (dumped by its `dump` example) with:

* coverage.py's executable statements, multi-line statement map and exclusions;
* coverage.py's function regions (the lines attributed to each function);
* radon's cyclomatic complexity of each function.

Usage:

    cargo build --release -p crap_python --example dump
    python scripts/check_line_model.py target/release/examples/dump $(find /usr/lib/python3.13 -name '*.py')

Requires `coverage` and `radon`. Exits with status 1 on any difference.
"""

from __future__ import annotations

import ast
import collections
import json
import subprocess
import sys
import warnings

from coverage.parser import PythonParser
from coverage.regions import code_regions
from radon.complexity import cc_visit

DEFAULT_EXCLUDE = r"#\s*(pragma|PRAGMA)[:\s]?\s*(no|NO)\s*(cover|COVER)"

# Known, deliberate differences from radon: crap treats `case _ as name:` as a catch-all.
RADON_EXCEPTIONS = {("sqlite3/__main__.py", "SqliteInteractiveConsole.runsource")}


def radon_complexities(text: str) -> dict[int, int]:
    """Map the line of each function's `def` to radon's complexity."""
    complexities = {}

    def add(function):
        complexities[function.lineno] = function.complexity
        for closure in getattr(function, "closures", []):
            add(closure)

    for block in cc_visit(text):
        for function in getattr(block, "methods", [block]):
            add(function)
    return complexities


def main() -> int:
    dump, *paths = sys.argv[1:]
    stats = collections.Counter()
    failures = collections.defaultdict(list)

    # Run the dump in chunks to keep command lines short.
    lines = []
    for start in range(0, len(paths), 500):
        output = subprocess.run(
            [dump, *paths[start : start + 500]], capture_output=True, text=True, check=True
        )
        lines.extend(output.stdout.splitlines())

    for line in lines:
        ours = json.loads(line)
        path = ours["path"]
        with open(path, encoding="utf-8") as file:
            text = file.read()
        try:
            with warnings.catch_warnings():
                warnings.simplefilter("ignore")
                parser = PythonParser(text=text, exclude=DEFAULT_EXCLUDE)
                parser.parse_source()
                regions = [r for r in code_regions(text) if r.kind == "function"]
                ast.parse(text)
        except Exception:  # Not valid Python for this interpreter.
            stats["skipped"] += 1
            continue
        if ours["syntax_errors"]:
            failures["syntax"].append(path)
            continue
        stats["files"] += 1

        multiline = {line: first for line, first in ours["multiline"]}
        expected = {line: first for line, first in parser.multiline_map.items() if line != first}
        if multiline != expected:
            failures["multiline"].append((path, sorted(set(multiline.items()) ^ set(expected.items()))[:5]))

        if set(ours["excluded"]) != parser.excluded:
            failures["excluded"].append((path, sorted(set(ours["excluded"]) ^ parser.excluded)[:5]))

        our_regions = {(f["name"], tuple(f["lines"])) for f in ours["functions"]}
        their_regions = {(r.name, tuple(sorted(r.lines))) for r in regions}
        if our_regions != their_regions:
            failures["regions"].append((path, sorted(our_regions ^ their_regions, key=str)[:2]))

        statements = set(ours["statements"])
        for region in regions:
            stats["functions"] += 1
            a, b = statements & region.lines, parser.statements & region.lines
            if a != b:
                failures["statements"].append(
                    (path, region.name, "only crap:", sorted(a - b), "only coverage.py:", sorted(b - a))
                )

        try:
            radon = radon_complexities(text)
        except Exception:
            continue
        for function in ours["functions"]:
            if function["line"] not in radon:
                continue
            stats["complexities"] += 1
            if radon[function["line"]] != function["complexity"]:
                if any(path.endswith(f) and function["name"] == n for f, n in RADON_EXCEPTIONS):
                    continue
                failures["complexity"].append(
                    (path, function["name"], function["complexity"], radon[function["line"]])
                )

    print(dict(stats))
    for kind, items in failures.items():
        print(f"{kind}: {len(items)} difference(s)")
        for item in items[:10]:
            print("   ", item)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
