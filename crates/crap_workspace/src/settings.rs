use std::path::{Path, PathBuf};

use crate::files::FilePatternSet;
use crate::options::{Options, OutputFormat, Unmeasured};

/// The default `threshold`: Alberto Savoia's original cut-off for crappy code.
pub const DEFAULT_THRESHOLD: f64 = 30.0;

/// Files and directories skipped by default.
pub const DEFAULT_EXCLUDE: &[&str] = &[
    ".bzr",
    ".direnv",
    ".eggs",
    ".git",
    ".git-rewrite",
    ".hg",
    ".ipynb_checkpoints",
    ".mypy_cache",
    ".nox",
    ".pants.d",
    ".pyenv",
    ".pytest_cache",
    ".pytype",
    ".ruff_cache",
    ".svn",
    ".tox",
    ".venv",
    ".vscode",
    "__pypackages__",
    "_build",
    "buck-out",
    "build",
    "dist",
    "node_modules",
    "site-packages",
    "venv",
];

/// Settings given on the command line, which take precedence over the configuration file.
#[derive(Debug, Clone, Default)]
pub struct SettingsOverrides {
    pub threshold: Option<f64>,
    pub coverage: Option<Vec<PathBuf>>,
    pub exclude: Option<Vec<String>>,
    pub extend_exclude: Vec<String>,
    pub respect_gitignore: Option<bool>,
    pub unmeasured: Option<Unmeasured>,
    pub output_format: Option<OutputFormat>,
}

/// Fully resolved settings.
#[derive(Debug, Clone)]
pub struct Settings {
    /// The project root: the directory of the configuration file, else of the nearest
    /// `pyproject.toml`, else the current directory.
    pub project_root: PathBuf,
    /// The configuration file the settings were read from, if any.
    pub config_file: Option<PathBuf>,
    pub threshold: f64,
    /// Absolute paths of the coverage files to read. Empty to discover them.
    pub coverage: Vec<PathBuf>,
    pub exclude: FilePatternSet,
    pub respect_gitignore: bool,
    pub unmeasured: Unmeasured,
    pub output_format: OutputFormat,
}

/// An error loading the configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Failed to read `{path}`: {err}")]
    Io { path: PathBuf, err: std::io::Error },
    #[error("Failed to parse `{path}`: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("Invalid setting in `{path}`: {message}")]
    Invalid { path: PathBuf, message: String },
}

impl Settings {
    /// Resolves the settings for a run from `cwd`, reading `config_file` if given and
    /// otherwise discovering a configuration file.
    pub fn resolve(
        cwd: &Path,
        config_file: Option<&Path>,
        overrides: SettingsOverrides,
    ) -> Result<Self, ConfigError> {
        // Without a discovered configuration file, the project is rooted at the nearest
        // `pyproject.toml`, or else the current directory.
        let default_root = || {
            cwd.ancestors()
                .find(|dir| dir.join("pyproject.toml").is_file())
                .unwrap_or(cwd)
                .to_path_buf()
        };
        let (config_file, options, project_root) = if let Some(path) = config_file {
            // An explicit configuration file (say, a shared `ci/crap.toml`) does not move the
            // project root.
            let path = cwd.join(path);
            let options = read_options(&path)?.unwrap_or_default();
            (Some(path), options, default_root())
        } else if let Some((path, options)) = find_config(cwd)? {
            let root = path.parent().unwrap_or(cwd).to_path_buf();
            (Some(path), options, root)
        } else {
            (None, Options::default(), default_root())
        };

        let threshold = overrides
            .threshold
            .or(options.threshold)
            .unwrap_or(DEFAULT_THRESHOLD);
        if !threshold.is_finite() || threshold < 0.0 {
            return Err(ConfigError::Invalid {
                path: config_file.clone().unwrap_or_default(),
                message: format!("`threshold` must be a non-negative number, got {threshold}"),
            });
        }

        // Paths from the command line are relative to the current directory; paths from the
        // configuration file are relative to the project root.
        let coverage = match overrides.coverage {
            Some(paths) => paths.iter().map(|path| cwd.join(path)).collect(),
            None => options
                .coverage
                .unwrap_or_default()
                .iter()
                .map(|path| project_root.join(path))
                .collect(),
        };

        let mut exclude: Vec<String> = overrides
            .exclude
            .or(options.exclude)
            .unwrap_or_else(|| DEFAULT_EXCLUDE.iter().map(ToString::to_string).collect());
        exclude.extend(options.extend_exclude.unwrap_or_default());
        exclude.extend(overrides.extend_exclude);
        let exclude = FilePatternSet::new(&project_root, &exclude).map_err(|message| {
            ConfigError::Invalid {
                path: config_file.clone().unwrap_or_default(),
                message,
            }
        })?;

        Ok(Self {
            project_root,
            config_file,
            threshold,
            coverage,
            exclude,
            respect_gitignore: overrides
                .respect_gitignore
                .or(options.respect_gitignore)
                .unwrap_or(true),
            unmeasured: overrides
                .unmeasured
                .or(options.unmeasured)
                .unwrap_or_default(),
            output_format: overrides
                .output_format
                .or(options.output_format)
                .unwrap_or_default(),
        })
    }
}

/// Finds the nearest configuration file at or above `start`: a `.crap.toml`, a `crap.toml`,
/// or a `pyproject.toml` with a `[tool.crap]` table.
pub fn find_config(start: &Path) -> Result<Option<(PathBuf, Options)>, ConfigError> {
    for directory in start.ancestors() {
        for name in [".crap.toml", "crap.toml", "pyproject.toml"] {
            let path = directory.join(name);
            if path.is_file()
                && let Some(options) = read_options(&path)?
            {
                tracing::debug!("Using configuration file `{}`", path.display());
                return Ok(Some((path, options)));
            }
        }
    }
    Ok(None)
}

/// Reads the options in a configuration file; `None` for a `pyproject.toml` without a
/// `[tool.crap]` table.
fn read_options(path: &Path) -> Result<Option<Options>, ConfigError> {
    let contents = std::fs::read_to_string(path).map_err(|err| ConfigError::Io {
        path: path.to_path_buf(),
        err,
    })?;
    let parse_error = |message: String| ConfigError::Parse {
        path: path.to_path_buf(),
        message,
    };

    if path
        .file_name()
        .is_some_and(|name| name == "pyproject.toml")
    {
        let document: toml::Table =
            toml::from_str(&contents).map_err(|err| parse_error(err.to_string()))?;
        let Some(table) = document.get("tool").and_then(|tool| tool.get("crap")) else {
            return Ok(None);
        };
        let options = table.clone().try_into().map_err(|err: toml::de::Error| {
            parse_error(format!("[tool.crap]: {}", err.message()))
        })?;
        Ok(Some(options))
    } else {
        toml::from_str(&contents)
            .map(Some)
            .map_err(|err| parse_error(err.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn defaults_without_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings::resolve(dir.path(), None, SettingsOverrides::default()).unwrap();
        assert_eq!(settings.threshold, DEFAULT_THRESHOLD);
        assert_eq!(settings.config_file, None);
        assert_eq!(settings.project_root, dir.path());
        assert!(settings.respect_gitignore);
        assert_eq!(settings.unmeasured, Unmeasured::Uncovered);
    }

    #[test]
    fn pyproject_in_a_parent_directory() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("pyproject.toml"),
            "[project]\nname = \"x\"\n\n[tool.crap]\nthreshold = 15\ncoverage = [\"reports/coverage.xml\"]\nunmeasured = \"skip\"\noutput-format = \"concise\"\n",
        );
        let nested = dir.path().join("src/pkg");
        std::fs::create_dir_all(&nested).unwrap();

        let settings = Settings::resolve(&nested, None, SettingsOverrides::default()).unwrap();
        assert_eq!(settings.threshold, 15.0);
        assert_eq!(settings.project_root, dir.path());
        assert_eq!(settings.coverage, [dir.path().join("reports/coverage.xml")]);
        assert_eq!(settings.unmeasured, Unmeasured::Skip);
        assert_eq!(settings.output_format, OutputFormat::Concise);
    }

    #[test]
    fn crap_toml_takes_precedence_over_pyproject() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("pyproject.toml"),
            "[tool.crap]\nthreshold = 15\n",
        );
        write(&dir.path().join("crap.toml"), "threshold = 20\n");
        let settings = Settings::resolve(dir.path(), None, SettingsOverrides::default()).unwrap();
        assert_eq!(settings.threshold, 20.0);
    }

    #[test]
    fn pyproject_without_table_marks_the_root() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("pyproject.toml"),
            "[project]\nname = \"x\"\n",
        );
        let nested = dir.path().join("src");
        std::fs::create_dir_all(&nested).unwrap();
        let settings = Settings::resolve(&nested, None, SettingsOverrides::default()).unwrap();
        assert_eq!(settings.config_file, None);
        assert_eq!(settings.project_root, dir.path());
    }

    #[test]
    fn overrides_win() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("crap.toml"), "threshold = 20\n");
        let overrides = SettingsOverrides {
            threshold: Some(8.0),
            ..SettingsOverrides::default()
        };
        let settings = Settings::resolve(dir.path(), None, overrides).unwrap();
        assert_eq!(settings.threshold, 8.0);
    }

    #[test]
    fn unknown_option() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("pyproject.toml"),
            "[tool.crap]\nthreshhold = 20\n",
        );
        let error = Settings::resolve(dir.path(), None, SettingsOverrides::default()).unwrap_err();
        assert!(
            error.to_string().contains("unknown field `threshhold`"),
            "{error}"
        );
    }

    #[test]
    fn negative_threshold() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("crap.toml"), "threshold = -1\n");
        assert!(Settings::resolve(dir.path(), None, SettingsOverrides::default()).is_err());
    }
}
