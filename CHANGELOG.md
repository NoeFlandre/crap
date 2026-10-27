# Changelog

## 0.1.0

Initial release.

- `crap check` reports functions whose CRAP score exceeds a threshold (30 by default), with
  `full`, `concise`, `json`, `json-lines`, `github`, `gitlab`, `sarif` and `junit` output.
- `crap report` lists the score of every function as a table, Markdown, CSV or JSON, with
  sorting and filtering.
- Reads coverage.py data files (line and branch data, parallel files), coverage.py JSON
  reports, Cobertura XML and LCOV, discovering the newest automatically.
- Per-function coverage matches coverage.py's function report, and honors coverage.py's
  exclusion settings and pragmas.
- Cyclomatic complexity compatible with radon.
- Configuration in `[tool.crap]` of `pyproject.toml`, `crap.toml` or `.crap.toml`.
- Suppression comments: `# crap: ignore` and `# noqa: CRAP001`.
