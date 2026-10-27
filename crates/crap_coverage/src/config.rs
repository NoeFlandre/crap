//! Reading the parts of coverage.py's configuration that affect measurement.
//!
//! Like coverage.py, the first of `.coveragerc`, `setup.cfg`, `tox.ini` and `pyproject.toml`
//! that holds coverage.py settings is used, unless `COVERAGE_RCFILE` names a file.

use std::path::{Path, PathBuf};

/// The coverage.py settings `crap` cares about.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CoveragePyConfig {
    /// The configuration file the settings were read from.
    pub path: Option<PathBuf>,
    /// `[run] data_file`: where the data file is written.
    pub data_file: Option<String>,
    /// `[report] exclude_lines`: replaces the default exclusion patterns.
    pub exclude_lines: Option<Vec<String>>,
    /// `[report] exclude_also`: extends the exclusion patterns.
    pub exclude_also: Vec<String>,
}

/// An error reading coverage.py's configuration.
#[derive(Debug, thiserror::Error)]
#[error("Failed to read coverage.py settings from `{path}`: {message}")]
pub struct ConfigError {
    pub path: PathBuf,
    pub message: String,
}

impl CoveragePyConfig {
    /// Finds and reads coverage.py's configuration for a project rooted at `root`.
    pub fn find(root: &Path) -> Result<Self, ConfigError> {
        if let Some(rcfile) = std::env::var_os("COVERAGE_RCFILE") {
            let path = root.join(rcfile);
            let config = if path.extension().is_some_and(|ext| ext == "toml") {
                Self::read_toml(&path)?
            } else {
                Self::read_ini(&path, true)?
            };
            return Ok(config.unwrap_or_default());
        }

        let rc = root.join(".coveragerc");
        if rc.is_file() {
            return Ok(Self::read_ini(&rc, true)?.unwrap_or_default());
        }
        for name in ["setup.cfg", "tox.ini"] {
            let path = root.join(name);
            if path.is_file()
                && let Some(config) = Self::read_ini(&path, false)?
            {
                return Ok(config);
            }
        }
        let pyproject = root.join("pyproject.toml");
        if pyproject.is_file()
            && let Some(config) = Self::read_toml(&pyproject)?
        {
            return Ok(config);
        }
        Ok(Self::default())
    }

    /// Reads an INI-style file. In `.coveragerc`, sections may be named `[run]` or
    /// `[coverage:run]`; elsewhere only the prefixed form is used.
    fn read_ini(path: &Path, unprefixed: bool) -> Result<Option<Self>, ConfigError> {
        let text = read(path)?;
        let sections = parse_ini(&text);
        let mut config = Self {
            path: Some(path.to_path_buf()),
            ..Self::default()
        };
        let mut found = unprefixed;
        for (section, key, value) in sections {
            let section = match section.strip_prefix("coverage:") {
                Some(section) => section,
                None if unprefixed => section.as_str(),
                None => continue,
            };
            found = true;
            match (section, key.as_str()) {
                ("run", "data_file") => config.data_file = Some(value.trim().to_string()),
                ("report", "exclude_lines") => config.exclude_lines = Some(split_lines(&value)),
                ("report", "exclude_also") => config.exclude_also = split_lines(&value),
                _ => {}
            }
        }
        Ok(found.then_some(config))
    }

    fn read_toml(path: &Path) -> Result<Option<Self>, ConfigError> {
        let text = read(path)?;
        let document: toml::Table = toml::from_str(&text).map_err(|err| ConfigError {
            path: path.to_path_buf(),
            message: err.to_string(),
        })?;
        let Some(coverage) = document
            .get("tool")
            .and_then(|tool| tool.get("coverage"))
            .and_then(toml::Value::as_table)
        else {
            return Ok(None);
        };
        let get = |section: &str, key: &str| coverage.get(section).and_then(|s| s.get(key));
        let strings = |value: Option<&toml::Value>| -> Option<Vec<String>> {
            match value? {
                toml::Value::String(string) => Some(split_lines(string)),
                toml::Value::Array(array) => Some(
                    array
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_string))
                        .collect(),
                ),
                _ => None,
            }
        };
        Ok(Some(Self {
            path: Some(path.to_path_buf()),
            data_file: get("run", "data_file")
                .and_then(toml::Value::as_str)
                .map(str::to_string),
            exclude_lines: strings(get("report", "exclude_lines")),
            exclude_also: strings(get("report", "exclude_also")).unwrap_or_default(),
        }))
    }
}

fn read(path: &Path) -> Result<String, ConfigError> {
    std::fs::read_to_string(path).map_err(|err| ConfigError {
        path: path.to_path_buf(),
        message: err.to_string(),
    })
}

/// Splits a multi-line INI value into its non-empty lines.
fn split_lines(value: &str) -> Vec<String> {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// A minimal parser for the INI dialect of Python's `configparser`: `[section]` headers,
/// `key = value` or `key: value` pairs, indented continuation lines, and `#`/`;` comments.
fn parse_ini(text: &str) -> Vec<(String, String, String)> {
    let mut entries: Vec<(String, String, String)> = Vec::new();
    let mut section = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let is_continuation = line.starts_with([' ', '\t']) && !trimmed.is_empty();
        if trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        if is_continuation {
            if let Some((entry_section, _, value)) = entries.last_mut()
                && *entry_section == section
            {
                value.push('\n');
                value.push_str(trimmed);
            }
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        if let Some(name) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            section = name.trim().to_string();
            continue;
        }
        let Some(separator) = trimmed.find(['=', ':']) else {
            continue;
        };
        let (key, value) = trimmed.split_at(separator);
        entries.push((
            section.clone(),
            key.trim().to_lowercase(),
            value[1..].trim().to_string(),
        ));
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coveragerc() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".coveragerc"),
            "[run]\ndata_file = build/.coverage\n\n[report]\n# comment\nexclude_also =\n    def __repr__\n    if TYPE_CHECKING:\n",
        )
        .unwrap();
        let config = CoveragePyConfig::find(dir.path()).unwrap();
        assert_eq!(config.data_file.as_deref(), Some("build/.coverage"));
        assert_eq!(config.exclude_lines, None);
        assert_eq!(config.exclude_also, ["def __repr__", "if TYPE_CHECKING:"]);
    }

    #[test]
    fn setup_cfg_without_coverage_falls_back_to_pyproject() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("setup.cfg"), "[metadata]\nname = x\n").unwrap();
        std::fs::write(
            dir.path().join("pyproject.toml"),
            "[tool.coverage.report]\nexclude_lines = [\"pragma: no cover\", \"raise NotImplementedError\"]\n",
        )
        .unwrap();
        let config = CoveragePyConfig::find(dir.path()).unwrap();
        assert_eq!(
            config.exclude_lines,
            Some(vec![
                "pragma: no cover".to_string(),
                "raise NotImplementedError".to_string()
            ])
        );
        assert_eq!(config.path, Some(dir.path().join("pyproject.toml")));
    }

    #[test]
    fn prefixed_sections_in_tox_ini() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("tox.ini"),
            "[tox]\nenvlist = py\n[coverage:run]\ndata_file: .cov\n",
        )
        .unwrap();
        let config = CoveragePyConfig::find(dir.path()).unwrap();
        assert_eq!(config.data_file.as_deref(), Some(".cov"));
    }

    #[test]
    fn no_configuration() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            CoveragePyConfig::find(dir.path()).unwrap(),
            CoveragePyConfig::default()
        );
    }
}
