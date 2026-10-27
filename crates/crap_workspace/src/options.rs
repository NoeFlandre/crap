use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use serde::Deserialize;

/// The options of a `crap` configuration file, as written.
///
/// ```toml
/// [tool.crap]
/// threshold = 30
/// coverage = ["coverage.xml"]
/// extend-exclude = ["tests/"]
/// ```
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    /// The CRAP score above which a function is reported. Defaults to 30.
    pub threshold: Option<f64>,
    /// Coverage files to read. Defaults to the most recent of coverage.py's data file,
    /// `coverage.json`, `coverage.xml`, `coverage.lcov` and `lcov.info`.
    pub coverage: Option<Vec<PathBuf>>,
    /// Glob patterns of files and directories to skip, replacing the defaults.
    pub exclude: Option<Vec<String>>,
    /// Glob patterns of files and directories to skip, in addition to `exclude`.
    pub extend_exclude: Option<Vec<String>>,
    /// Whether to skip files ignored by `.gitignore` and similar files. Defaults to `true`.
    pub respect_gitignore: Option<bool>,
    /// How to treat files that are missing from the coverage data. Defaults to `uncovered`.
    pub unmeasured: Option<Unmeasured>,
    /// The output format of `crap check`. Defaults to `full`.
    pub output_format: Option<OutputFormat>,
}

/// How to treat files that do not appear in the coverage data.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Unmeasured {
    /// Treat their functions as having no coverage: code that no test imports is untested.
    #[default]
    Uncovered,
    /// Leave them out of the analysis.
    Skip,
}

impl Unmeasured {
    pub const ALL: &[&str] = &["uncovered", "skip"];
}

impl FromStr for Unmeasured {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "uncovered" => Ok(Self::Uncovered),
            "skip" => Ok(Self::Skip),
            _ => Err(format!("expected one of: {}", Self::ALL.join(", "))),
        }
    }
}

/// The format in which `crap check` reports crappy functions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputFormat {
    /// One diagnostic per function, with the source line and a hint.
    #[default]
    Full,
    /// One line per function.
    Concise,
    /// A JSON array.
    Json,
    /// One JSON object per line.
    JsonLines,
    /// GitHub Actions workflow commands, shown as annotations on pull requests.
    Github,
    /// A GitLab Code Quality report.
    Gitlab,
    /// A SARIF 2.1.0 log, for GitHub code scanning and other tools.
    Sarif,
    /// A JUnit XML report.
    Junit,
}

impl OutputFormat {
    pub const ALL: &[&str] = &[
        "full",
        "concise",
        "json",
        "json-lines",
        "github",
        "gitlab",
        "sarif",
        "junit",
    ];
}

impl FromStr for OutputFormat {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(match value {
            "full" => Self::Full,
            "concise" => Self::Concise,
            "json" => Self::Json,
            "json-lines" => Self::JsonLines,
            "github" => Self::Github,
            "gitlab" => Self::Gitlab,
            "sarif" => Self::Sarif,
            "junit" => Self::Junit,
            _ => return Err(format!("expected one of: {}", Self::ALL.join(", "))),
        })
    }
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let index = *self as usize;
        f.write_str(Self::ALL[index])
    }
}
