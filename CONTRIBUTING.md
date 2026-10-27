# Contributing to crap

Thanks for your interest in improving `crap`!

## Getting started

You need [Rust](https://rustup.rs/) (the toolchain version is pinned in
`rust-toolchain.toml` and installed automatically) and, for the conformance checks, Python
with [uv](https://docs.astral.sh/uv/).

```console
$ cargo build
$ cargo run -- check path/to/project
```

## Layout

| Crate            | Purpose                                                                      |
| ---------------- | ---------------------------------------------------------------------------- |
| `crap`           | The command-line interface and output formats.                               |
| `crap_linter`    | Combines complexity and coverage into CRAP scores, file by file.             |
| `crap_python`    | Parses Python (with Ruff's parser): functions, complexity, executable lines. |
| `crap_coverage`  | Reads coverage data and coverage.py's configuration, and matches paths.      |
| `crap_workspace` | Configuration files, settings and file discovery.                            |

The Python package in `python/` only locates and runs the binary; it is built with
[maturin](https://www.maturin.rs/).

## Testing

```console
$ cargo test --workspace
$ cargo clippy --workspace --all-targets -- -D warnings
$ cargo fmt --all
```

The command-line tests in `crates/crap/tests/cli` use [insta](https://insta.rs/) snapshots.
After changing output, review the new snapshots with
[`cargo insta`](https://insta.rs/docs/cli/):

```console
$ cargo insta test --review
```

### Conformance with coverage.py and radon

`crap` must agree with coverage.py on which lines are executable and which belong to each
function, and with radon on complexity. Two scripts check this against the real tools, and
run in CI:

```console
$ uv venv && uv pip install coverage pytest radon
$ cargo build --release --bin crap --example dump

# Per-function coverage from a coverage.py data file, compared with `coverage json`.
$ .venv/bin/python scripts/conformance.py target/release/crap scripts/conformance/project
$ .venv/bin/python scripts/conformance.py target/release/crap scripts/conformance/project --branch

# Statements, regions, exclusions and complexity over a whole code base.
$ .venv/bin/python scripts/check_line_model.py target/release/examples/dump $(find /usr/lib/python3.13 -name '*.py')
```

When adding support for a construct, extend `scripts/conformance/project` to exercise it.

## Releasing

1. Update the version in `Cargo.toml` and `pyproject.toml`, and add a section to
   `CHANGELOG.md`.
2. Commit, then tag the commit `vX.Y.Z` and push the tag.

The `Release` workflow builds wheels and binaries for every platform, publishes the wheels to
PyPI with trusted publishing (from the `pypi` environment), and creates a GitHub release with
the binaries and the changelog section.
