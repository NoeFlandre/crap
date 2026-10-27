# crap

[![CI](https://github.com/noeflandre/crap/actions/workflows/ci.yml/badge.svg)](https://github.com/noeflandre/crap/actions/workflows/ci.yml)
[![PyPI](https://img.shields.io/pypi/v/crap-cli.svg)](https://pypi.org/project/crap-cli/)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

An extremely fast CRAP score checker for Python, written in Rust.

`crap` finds the functions in your Python code that are **too complex for how well they are
tested**. It combines the cyclomatic complexity of every function with the coverage data your
test suite already produces, and flags the ones that are risky to change.

```console
$ pytest --cov
$ crap check
src/shop/pricing.py:8:5: CRAP001 `discount` has a CRAP score of 46.9 (threshold: 30)
  |
8 | def discount(customer, amount, coupon=None):
  |     ^^^^^^^^ complexity 11, coverage 33.3% (missed lines 12-17, 19-22, 24)
  |
  = help: Cover at least 47% of it with tests, or reduce its complexity

Found 1 crappy function out of 214 (threshold: 30).
```

## Highlights

- ⚡️ **Fast.** Analyzes CPython's standard library (~300,000 lines) in under 0.1 seconds,
  about 25× faster than `radon cc` computes complexity alone.
- 🎯 **Exact.** Per-function coverage matches coverage.py's own function report, line for
  line, whichever format you feed it.
- 🔌 **Zero setup.** Reads the `.coverage` file that `pytest --cov` and `coverage run` leave
  behind. No `coverage xml` step needed.
- 📦 **Every coverage format.** coverage.py data files, coverage.py JSON, Cobertura XML and
  LCOV, detected automatically, merged when you pass several.
- 🐍 **Modern Python.** Parses Python 3.14 syntax with [Ruff's parser](https://github.com/astral-sh/ruff).
- 🤖 **Made for CI.** Output for GitHub annotations, GitLab Code Quality, SARIF, JUnit and
  JSON, and a meaningful exit status.
- 🔧 **Configured where you expect.** `[tool.crap]` in `pyproject.toml`, and it honors
  coverage.py's own settings (`exclude_lines`, `exclude_also`, `data_file`).

## What is a CRAP score?

CRAP (Change Risk Anti-Patterns) was introduced by Alberto Savoia and Bob Evans to spot code
that is hard to change safely. For a function with cyclomatic complexity `comp` and test
coverage `cov` (between 0 and 1):

```text
CRAP = comp² × (1 − cov)³ + comp
```

A fully tested function scores its complexity. An untested one scores close to the square of
its complexity. The usual threshold is **30**: above it, a function is "crappy", and needs
either more tests or less complexity.

| Complexity | Coverage needed to score ≤ 30 |
| ---------: | ----------------------------: |
|        1–5 |                            0% |
|         10 |                           42% |
|         15 |                           60% |
|         20 |                           71% |
|         25 |                           80% |
|         30 |                          100% |
|        31+ |    impossible: split it up    |

`crap check` tells you exactly how much coverage each crappy function needs.

## Installation

`crap` is published on PyPI as `crap-cli`. With [uv](https://docs.astral.sh/uv/):

```console
$ uv tool install crap-cli    # install the `crap` command
$ uvx --from crap-cli crap check    # or run it without installing
$ uv add --dev crap-cli       # or add it to your project
```

With pip or pipx:

```console
$ pip install crap-cli
$ pipx install crap-cli
```

From source, with a Rust toolchain:

```console
$ cargo install --git https://github.com/noeflandre/crap crap
```

Prebuilt binaries for Linux, macOS and Windows are attached to every
[GitHub release](https://github.com/noeflandre/crap/releases).

## Usage

Run your tests with coverage, then run `crap`:

```console
$ pytest --cov            # or: coverage run -m pytest
$ crap check              # report crappy functions; exit status 1 if any
$ crap report             # list every function, worst first
```

`crap check` takes files and directories (the current directory by default) and exits with:

| Status | Meaning                                                           |
| -----: | ----------------------------------------------------------------- |
|    `0` | No crappy functions (or `--exit-zero` was passed).                |
|    `1` | Crappy functions were found.                                      |
|    `2` | `crap` could not run: no coverage data, invalid configuration, … |

`crap report` shows the score of every function:

```console
$ crap report --top 5
CRAP  Complexity  Coverage  Function       Location
46.9          11     33.3%  discount       src/shop/pricing.py:8
42.0           6      0.0%  Cart.checkout  src/shop/pricing.py:48
30.0           5      0.0%  Cart.shipping  src/shop/pricing.py:37
 2.1           2     66.7%  Cart.add       src/shop/pricing.py:32
 2.0           2    100.0%  total          src/shop/pricing.py:4

Showing 5 of 6 matching functions. 6 functions analyzed, 2 above the threshold of 30.
```

Sort with `--sort score|complexity|coverage|location|name`, filter with `--min-score` or
`--crappy-only`, and export with `--output-format table|markdown|csv|json`. The Markdown
table drops straight into a pull request comment.

Run `crap help check` and `crap help report` for every option.

### Output formats

`crap check --output-format <FORMAT>`:

| Format       | Use                                                               |
| ------------ | ----------------------------------------------------------------- |
| `full`       | The default: each function with its source line and a hint.       |
| `concise`    | One line per function.                                            |
| `json`       | A JSON array, with complexity, coverage and missed lines.         |
| `json-lines` | One JSON object per line.                                         |
| `github`     | GitHub Actions annotations, shown inline on pull requests.        |
| `gitlab`     | A GitLab Code Quality report.                                     |
| `sarif`      | SARIF 2.1.0, for GitHub code scanning and other SAST tools.       |
| `junit`      | A JUnit XML report, one test case per function.                   |

Write any of them to a file with `--output-file <FILE>`.

## Coverage data

Without `--coverage`, `crap` looks in the project root for coverage.py's data file (`.coverage`,
or the file named by `COVERAGE_FILE` or `[run] data_file`), unmerged parallel data files
(`.coverage.*`), `coverage.json`, `coverage.xml`, `coverage.lcov` and `lcov.info`, and uses
**the most recently modified**, so a stale report never hides fresh data.

Point it at specific files with `--coverage`, repeated to merge several runs:

```console
$ crap check --coverage coverage.xml
$ crap check --coverage unit/.coverage --coverage integration/.coverage
```

The format is detected from the contents, not the name:

| Format                      | Produced by                                         |
| --------------------------- | --------------------------------------------------- |
| coverage.py data file       | `coverage run`, `pytest --cov` (any core, any mode) |
| coverage.py JSON report     | `coverage json`, `pytest --cov-report=json`         |
| Cobertura XML               | `coverage xml`, `pytest --cov-report=xml`, others   |
| LCOV                        | `coverage lcov`, `pytest --cov-report=lcov`, others |

Paths are matched to your files even when the data was recorded somewhere else, such as
absolute paths from a CI runner.

### How coverage is attributed to functions

A function's coverage is the fraction of its executable statements that ran, exactly as
coverage.py's function-level report computes it: the lines of its body, excluding docstrings
and the bodies of nested functions (which are scored on their own).

coverage.py's data file records only which lines ran, so `crap` works out which lines are
executable from the source, reproducing coverage.py's rules: multi-line statements,
docstrings, unreachable code (after `return`, under `if False:`, ...), and exclusion pragmas.
`crap`'s test suite checks this model against coverage.py on tens of thousands of functions.

`crap` honors coverage.py's exclusions: `# pragma: no cover`, and your `exclude_lines` and
`exclude_also` settings from `.coveragerc`, `setup.cfg`, `tox.ini` or `pyproject.toml`.
Functions excluded from coverage are not scored.

### Files missing from the coverage data

A file that never appears in the coverage data was never imported by your tests, so by
default its functions count as untested, and `crap` warns about it. When the coverage data
deliberately covers only part of the project (for example `pytest --cov=src`, leaving your
test files unmeasured), either exclude the rest or skip unmeasured files:

```toml
[tool.crap]
extend-exclude = ["tests/"]
# or
unmeasured = "skip"
```

## Complexity

`crap` computes the cyclomatic complexity of each function the way
[radon](https://radon.readthedocs.io/) does, so the numbers are familiar. A function starts
at 1, and each decision point adds to it:

| Construct                                    | Adds                                  |
| -------------------------------------------- | ------------------------------------- |
| `if`, `elif`                                 | 1 each                                |
| conditional expression (`a if b else c`)     | 1                                     |
| `for`, `async for`, `while`                  | 1, plus 1 for an `else` clause        |
| `try`                                        | 1 per `except`, plus 1 for `else`     |
| `match`                                      | 1 per `case`, minus a final catch-all |
| boolean operators (`a and b or c`)           | 1 per extra operand                   |
| comprehensions (`[x for x in y if z]`)       | 1 per `for`, plus 1 per `if`          |
| `assert`                                     | 1                                     |

Nested functions and classes are scored separately and do not add to their parent. Lambdas
count toward the function that contains them.

Two deliberate departures from radon: `except*` handlers count like `except` handlers (radon
ignores them), and any irrefutable `case` (`case _:`, `case name:`, `case _ as name:`, or an
or-pattern ending in one) is recognized as the catch-all, without a guard.

## Configuration

`crap` reads its settings from the nearest `pyproject.toml` with a `[tool.crap]` table, or a
`crap.toml` or `.crap.toml` (same keys, without the table header), searching from the current
directory upwards. Command-line flags take precedence. Use `--config <FILE>` to pick a file.

```toml
[tool.crap]
# Report functions scoring above this.
threshold = 30
# Coverage files to read; discovered automatically when omitted.
coverage = ["coverage.xml"]
# Skip these, in addition to the defaults (.venv, build, dist, node_modules, ...).
extend-exclude = ["tests/", "docs/", "*_pb2.py"]
# Replace the default exclusions entirely.
# exclude = [".venv", "build"]
# Skip files ignored by .gitignore.
respect-gitignore = true
# Files missing from the coverage data: "uncovered" (count as untested) or "skip".
unmeasured = "uncovered"
# Default output format of `crap check`.
output-format = "full"
```

| Option              | Default                     | Description                                                     |
| ------------------- | --------------------------- | --------------------------------------------------------------- |
| `threshold`         | `30`                        | The CRAP score above which a function is reported.              |
| `coverage`          | discovered                  | Coverage files to read, relative to the project root.           |
| `exclude`           | `.venv`, `build`, `dist`, … | Glob patterns of files and directories to skip.                 |
| `extend-exclude`    | `[]`                        | Patterns to skip in addition to `exclude`.                      |
| `respect-gitignore` | `true`                      | Skip files ignored by `.gitignore`, `.ignore` and git excludes. |
| `unmeasured`        | `"uncovered"`               | How to treat files missing from the coverage data.              |
| `output-format`     | `"full"`                    | The default output format of `crap check`.                      |

Patterns without a slash (`build`, `*_pb2.py`) match a file or directory name anywhere.
Patterns with a slash, or ending with one (`tests/`, `src/legacy/*.py`), match paths from the
project root, and everything below them.

The project root is the directory of the configuration file, else of the nearest
`pyproject.toml`, else the current directory. Coverage data is looked for there.

### Suppressing a function

Add `# crap: ignore` or `# noqa: CRAP001` to the line of the `def` (or anywhere in its
signature or decorators):

```python
def legacy_dispatch(event):  # crap: ignore
    ...
```

If you also run Ruff, declare the code so it does not flag the `noqa` as unknown:

```toml
[tool.ruff.lint]
external = ["CRAP"]
```

## Continuous integration

### GitHub Actions

```yaml
- uses: astral-sh/setup-uv@v6
- run: uv run pytest --cov
- run: uvx --from crap-cli crap check --output-format github
```

With `--output-format github`, crappy functions are annotated inline on the pull request.
For GitHub code scanning, upload `--output-format sarif` with
`github/codeql-action/upload-sarif`.

### GitLab CI

```yaml
crap:
  script:
    - pytest --cov
    - crap check --output-format gitlab --output-file gl-code-quality-report.json
  artifacts:
    reports:
      codequality: gl-code-quality-report.json
```

### pre-commit

```yaml
- repo: https://github.com/noeflandre/crap
  rev: v0.1.0
  hooks:
    - id: crap
```

The hook checks the whole project against your latest coverage data, so run your tests with
coverage first.

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) to get started.

## Acknowledgements

The CRAP metric is due to Alberto Savoia and Bob Evans
([crap4j](https://www.artima.com/weblogs/viewpost.jsp?thread=210575)). `crap` parses Python
with [Ruff](https://github.com/astral-sh/ruff)'s parser, follows
[coverage.py](https://github.com/nedbat/coveragepy)'s model of executable lines and
[radon](https://github.com/rubik/radon)'s definition of complexity, and takes its look and
feel from [Astral](https://astral.sh)'s tools.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
