"""Check crap's per-function coverage against coverage.py's own function report.

Runs a project's tests under coverage.py, then compares, for every function, the statements
and covered statements that `crap report` computes from the raw `.coverage` data file with
those in `coverage json`'s function summaries.

Usage:

    python scripts/conformance.py path/to/crap path/to/project [--branch] -- pytest args...

Requires `coverage` and `pytest`. Exits with status 1 on any difference.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("crap")
    parser.add_argument("project")
    parser.add_argument("--branch", action="store_true")
    parser.add_argument("pytest_args", nargs="*")
    args = parser.parse_args()

    crap = os.path.abspath(args.crap)
    project = os.path.abspath(args.project)
    with tempfile.TemporaryDirectory() as tmp:
        data_file = os.path.join(tmp, ".coverage")
        env = {**os.environ, "COVERAGE_FILE": data_file}
        run = [sys.executable, "-m", "coverage", "run"]
        if args.branch:
            run.append("--branch")
        subprocess.run([*run, "-m", "pytest", "-q", "-p", "no:cacheprovider", *args.pytest_args],
                       cwd=project, env=env, check=False)
        report = os.path.join(tmp, "coverage.json")
        subprocess.run([sys.executable, "-m", "coverage", "json", "-q", "-o", report],
                       cwd=project, env=env, check=True)
        with open(report) as file:
            expected = {}
            for path, data in json.load(file)["files"].items():
                for name, function in data.get("functions", {}).items():
                    if name:
                        summary = function["summary"]
                        expected[(os.path.normpath(os.path.join(project, path)), name)] = (
                            summary["num_statements"], summary["covered_lines"])

        output = subprocess.run(
            [crap, "report", "--quiet", "--output-format", "json", "--unmeasured", "skip",
             "--coverage", data_file],
            cwd=project, capture_output=True, text=True, check=True,
        )
        actual = {
            (os.path.normpath(f["filename"]), f["function"]): (f["statements"], f["covered"])
            for f in json.loads(output.stdout)
        }

    # coverage.py also lists functions it excludes; crap leaves them out.
    differences = [
        (key, actual.get(key), value)
        for key, value in expected.items()
        if actual.get(key) != value and not (key not in actual and value == (0, 0))
    ]
    print(f"Compared {len(expected)} functions: {len(differences)} difference(s)")
    for (path, name), ours, theirs in differences[:20]:
        print(f"    {os.path.relpath(path, project)}::{name}: crap {ours}, coverage.py {theirs}")
    return 1 if differences else 0


if __name__ == "__main__":
    sys.exit(main())
