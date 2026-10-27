//! Readers for line coverage data.
//!
//! Supported formats:
//!
//! * coverage.py data files (`.coverage`, SQLite), as written by `coverage run` and
//!   `pytest --cov`;
//! * coverage.py JSON reports (`coverage json`);
//! * Cobertura XML reports (`coverage xml`, and most other coverage tools);
//! * LCOV tracefiles (`coverage lcov`).
//!
//! The format of a file is detected from its contents, not its name.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};

pub use crate::config::{ConfigError, CoveragePyConfig};
pub use crate::discovery::discover;
pub use crate::paths::PathResolver;

mod config;
mod discovery;
mod json;
mod lcov;
mod paths;
mod sqlite;
mod xml;

/// A set of one-based line numbers.
pub type LineSet = BTreeSet<u32>;

/// The format of a coverage file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// A coverage.py SQLite data file.
    CoveragePy,
    /// A coverage.py JSON report.
    Json,
    /// A Cobertura XML report.
    Cobertura,
    /// An LCOV tracefile.
    Lcov,
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Format::CoveragePy => "coverage.py data file",
            Format::Json => "coverage.py JSON report",
            Format::Cobertura => "Cobertura XML report",
            Format::Lcov => "LCOV tracefile",
        })
    }
}

/// Coverage measured for a single source file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileCoverage {
    /// Lines that were executed. Lines from a coverage.py data file are raw tracer lines,
    /// which may need to be normalized to the first line of their statement.
    pub executed: LineSet,
    /// The executable lines, when the format records them. coverage.py data files only
    /// record executed lines, so the executable lines must be found by analyzing the source.
    pub statements: Option<LineSet>,
}

impl FileCoverage {
    fn merge(&mut self, other: FileCoverage) {
        self.executed.extend(other.executed);
        self.statements = match (self.statements.take(), other.statements) {
            (Some(mut ours), Some(theirs)) => {
                ours.extend(theirs);
                Some(ours)
            }
            (ours, theirs) => ours.or(theirs),
        };
    }
}

/// Coverage data loaded from one or more files.
#[derive(Debug, Clone, Default)]
pub struct CoverageData {
    /// Measured files, keyed by the path recorded in the data. Recorded paths may be absolute
    /// or relative, and may come from another machine.
    pub files: BTreeMap<String, FileCoverage>,
    /// Directories relative paths may be relative to, in order of preference.
    pub roots: Vec<PathBuf>,
    /// The files the data was read from.
    pub sources: Vec<(PathBuf, Format)>,
}

impl CoverageData {
    /// Reads coverage data from `path`, detecting its format.
    pub fn read(path: &Path) -> Result<Self, Error> {
        let contents = std::fs::read(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let format = detect_format(&contents).ok_or_else(|| Error::UnknownFormat {
            path: path.to_path_buf(),
        })?;
        tracing::debug!("Reading {format} from `{}`", path.display());

        let parse_error = |message: String| Error::Parse {
            path: path.to_path_buf(),
            format,
            message,
        };
        let mut roots = Vec::new();
        let files = match format {
            Format::CoveragePy => sqlite::read(path).map_err(parse_error)?,
            Format::Json => json::read(&contents).map_err(parse_error)?,
            Format::Lcov => lcov::read(&contents).map_err(parse_error)?,
            Format::Cobertura => {
                let report = xml::read(&contents).map_err(parse_error)?;
                roots.extend(report.sources.into_iter().map(PathBuf::from));
                report.files
            }
        };

        // Relative paths are most often relative to the directory the tests ran in, which is
        // usually where the data file was written.
        if let Some(parent) = std::path::absolute(path)
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
        {
            roots.push(parent);
        }

        Ok(Self {
            files,
            roots,
            sources: vec![(path.to_path_buf(), format)],
        })
    }

    /// Reads and merges coverage data from several files.
    pub fn read_all<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Result<Self, Error> {
        let mut data = Self::default();
        for path in paths {
            data.merge(Self::read(path)?);
        }
        Ok(data)
    }

    /// Merges `other` into `self`, taking the union of executed lines.
    pub fn merge(&mut self, other: CoverageData) {
        for (path, coverage) in other.files {
            self.files.entry(path).or_default().merge(coverage);
        }
        for root in other.roots {
            if !self.roots.contains(&root) {
                self.roots.push(root);
            }
        }
        self.sources.extend(other.sources);
    }
}

/// Detects the format of a coverage file from its first bytes.
pub fn detect_format(contents: &[u8]) -> Option<Format> {
    if contents.starts_with(b"SQLite format 3\0") {
        return Some(Format::CoveragePy);
    }
    let text = contents.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(contents);
    let start = text.iter().position(|byte| !byte.is_ascii_whitespace())?;
    match text[start] {
        b'{' => Some(Format::Json),
        b'<' => Some(Format::Cobertura),
        _ if text[start..]
            .split(|&byte| byte == b'\n')
            .any(|line| line.starts_with(b"SF:")) =>
        {
            Some(Format::Lcov)
        }
        _ => None,
    }
}

/// An error reading coverage data.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to read `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "`{path}` is not a supported coverage file (expected a coverage.py data file, or a JSON, Cobertura XML or LCOV report)"
    )]
    UnknownFormat { path: PathBuf },
    #[error("Failed to parse `{path}` as a {format}: {message}")]
    Parse {
        path: PathBuf,
        format: Format,
        message: String,
    },
}
