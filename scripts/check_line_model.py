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

from coverage.parser import PythonParser, _case_is_irrefutable
from coverage.regions import code_regions
from radon.complexity import cc_visit

DEFAULT_EXCLUDE = r"#\s*(pragma|PRAGMA)[:\s]?\s*(no|NO)\s*(cover|COVER)"

# Functions where CPython's optimizer goes further than crap's model, in its own compiler
# regression tests.
KNOWN_STATEMENT_DIFFERENCES = {("test/test_compile.py", "TestSpecifics.test_regression_gh_120225.name_4")}


def radon_disagrees(tree: ast.AST) -> set[int]:
    """The `def` lines of functions where crap deliberately differs from radon.

    radon ignores `except*` handlers, and only recognizes `case _:` and `case name:` as
    catch-all cases, not `case _ as name:`, or-patterns, or guarded wildcards. crap counts
    `except*` like `except`, and uses coverage.py's definition of an irrefutable case.
    """
    def differs(function: ast.AST) -> bool:
        for node in ast.walk(function):
            if type(node).__name__ == "TryStar":
                return True
            if isinstance(node, ast.Match):
                radon = any(getattr(case.pattern, "pattern", False) is None for case in node.cases)
                ours = any(_case_is_irrefutable(case) for case in node.cases)
                if radon != ours:
                    return True
        return False

    return {
        node.lineno
        for node in ast.walk(tree)
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and differs(node)
    }


def lazy_annotations(tree: ast.AST) -> set[int]:
    """Lines of bare annotations in class bodies, which compile to nothing from Python 3.14
    (PEP 649) but to code before. crap follows the older behavior."""
    if sys.version_info < (3, 14):
        return set()
    return {
        statement.lineno
        for node in ast.walk(tree)
        if isinstance(node, ast.ClassDef)
        for statement in node.body
        if isinstance(statement, ast.AnnAssign) and statement.value is None
    }


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
                tree = ast.parse(text)
                skip_complexity = radon_disagrees(tree)
                version_dependent = lazy_annotations(tree)
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

        statements = set(ours["statements"]) - version_dependent
        for region in regions:
            stats["functions"] += 1
            a = statements & region.lines
            b = (parser.statements - version_dependent) & region.lines
            known = any(path.endswith(f) and region.name == n for f, n in KNOWN_STATEMENT_DIFFERENCES)
            # From Python 3.14, coverage.py ignores code objects named `__annotate__` (to hide
            # the ones the compiler generates for PEP 649), user-defined ones included.
            known |= sys.version_info >= (3, 14) and region.name.endswith("__annotate__")
            if a != b and not known:
                failures["statements"].append(
                    (path, region.name, "only crap:", sorted(a - b), "only coverage.py:", sorted(b - a))
                )

        try:
            radon = radon_complexities(text)
        except Exception:
            continue
        for function in ours["functions"]:
            if function["line"] not in radon or function["line"] in skip_complexity:
                continue
            stats["complexities"] += 1
            if radon[function["line"]] != function["complexity"]:
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
