//! Computing CRAP scores for Python functions.
//!
//! The CRAP (Change Risk Anti-Patterns) score of a function combines its cyclomatic
//! complexity `comp` with the fraction `cov` of its statements that tests execute:
//!
//! ```text
//! CRAP = comp² × (1 − cov)³ + comp
//! ```
//!
//! A simple, well-tested function scores close to its complexity. A complex, untested one
//! scores close to the square of its complexity. Functions scoring above a threshold
//! (30 by default) are "crappy": too complex for how well they are tested.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use rayon::prelude::*;
use regex::Regex;

use crap_coverage::{CoverageData, FileCoverage, LineSet, PathResolver};
use crap_python::{AnalysisOptions, ExclusionPatterns, SyntaxError, analyze};
use crap_workspace::Unmeasured;

pub use crap_python::FunctionKind;

/// The code under which crappy functions are reported, for suppression comments
/// (`# noqa: CRAP001`) and machine-readable output formats.
pub const CODE: &str = "CRAP001";

/// Computes the CRAP score of a function with the given cyclomatic `complexity` and
/// `coverage`, a fraction between 0 and 1.
pub fn crap_score(complexity: u32, coverage: f64) -> f64 {
    let complexity = f64::from(complexity);
    let uncovered = 1.0 - coverage.clamp(0.0, 1.0);
    complexity * complexity * uncovered.powi(3) + complexity
}

/// The minimum coverage (a fraction between 0 and 1) a function of the given `complexity`
/// needs to score at most `threshold`, or `None` when no amount of coverage is enough
/// because the complexity alone exceeds the threshold.
pub fn required_coverage(complexity: u32, threshold: f64) -> Option<f64> {
    let complexity = f64::from(complexity);
    if complexity > threshold {
        return None;
    }
    let uncovered = ((threshold - complexity) / (complexity * complexity)).cbrt();
    Some((1.0 - uncovered).clamp(0.0, 1.0))
}

/// The analysis of a single function.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionReport {
    pub name: String,
    /// The dotted name of the function, e.g. `Parser.parse`.
    pub qualified_name: String,
    pub kind: FunctionKind,
    /// One-based line and column of the function's name.
    pub line: u32,
    pub column: u32,
    /// The last line of the function's body.
    pub end_line: u32,
    /// The text of the line holding the function's name, for display.
    pub line_text: String,
    pub complexity: u32,
    /// The number of executable statements in the function (nested functions excluded).
    pub statements: u32,
    /// The number of those statements that were executed.
    pub covered: u32,
    /// The statements that were not executed.
    pub missed_lines: Vec<u32>,
    /// The missed statements as inclusive line ranges, merging runs of consecutive missed
    /// statements like coverage.py's "Missing" column.
    pub missed_ranges: Vec<(u32, u32)>,
    /// Whether the function's file appears in the coverage data.
    pub measured: bool,
    pub score: f64,
    /// Whether the function carries a `# crap: ignore` or `# noqa: CRAP001` comment.
    pub suppressed: bool,
}

impl FunctionReport {
    /// The fraction of the function's statements that were executed. A function without
    /// statements has nothing to cover, and counts as fully covered.
    pub fn coverage(&self) -> f64 {
        if self.statements == 0 {
            1.0
        } else {
            f64::from(self.covered) / f64::from(self.statements)
        }
    }

    /// Whether the function's score exceeds `threshold`.
    pub fn is_crappy(&self, threshold: f64) -> bool {
        self.score > threshold
    }
}

/// The analysis of a single file.
#[derive(Debug, Clone)]
pub struct FileReport {
    /// The absolute path of the file.
    pub path: PathBuf,
    pub functions: Vec<FunctionReport>,
    /// Whether the file appears in the coverage data.
    pub measured: bool,
    pub syntax_errors: Vec<SyntaxError>,
}

/// A file that could not be analyzed.
#[derive(Debug, Clone)]
pub struct FileError {
    pub path: PathBuf,
    pub message: String,
}

/// The analysis of a set of files.
#[derive(Debug, Default)]
pub struct Analysis {
    /// One report per analyzed file, in the order the files were given.
    pub files: Vec<FileReport>,
    pub errors: Vec<FileError>,
}

impl Analysis {
    /// Every analyzed function, file by file.
    pub fn functions(&self) -> impl Iterator<Item = (&FileReport, &FunctionReport)> {
        self.files
            .iter()
            .flat_map(|file| file.functions.iter().map(move |function| (file, function)))
    }
}

/// Settings for an analysis.
#[derive(Debug, Clone, Default)]
pub struct AnalysisSettings {
    pub exclude: ExclusionPatterns,
    pub unmeasured: Unmeasured,
}

/// Analyzes `files` (absolute paths) against `coverage`, in parallel.
pub fn analyze_files(
    files: &[PathBuf],
    coverage: &CoverageData,
    project_root: &Path,
    settings: &AnalysisSettings,
) -> Analysis {
    let resolver = PathResolver::new(coverage, project_root);
    let options = AnalysisOptions {
        exclude: settings.exclude.clone(),
    };

    let results: Vec<Result<Option<FileReport>, FileError>> = files
        .par_iter()
        .map(|path| {
            let source = std::fs::read(path).map_err(|err| FileError {
                path: path.clone(),
                message: err.to_string(),
            })?;
            let source = String::from_utf8(source).map_err(|_| FileError {
                path: path.clone(),
                message: "File is not valid UTF-8".to_string(),
            })?;
            let file_coverage = resolver.resolve(path).map(|(_, coverage)| coverage);
            if file_coverage.is_none() && settings.unmeasured == Unmeasured::Skip {
                return Ok(None);
            }
            Ok(Some(analyze_file(path, &source, file_coverage, &options)))
        })
        .collect();

    let mut analysis = Analysis::default();
    for result in results {
        match result {
            Ok(Some(report)) => analysis.files.push(report),
            Ok(None) => {}
            Err(error) => analysis.errors.push(error),
        }
    }
    analysis
}

/// Analyzes a single file. `coverage` is `None` when the file is missing from the coverage
/// data, in which case its functions are reported as uncovered.
pub fn analyze_file(
    path: &Path,
    source: &str,
    coverage: Option<&FileCoverage>,
    options: &AnalysisOptions,
) -> FileReport {
    let analysis = analyze(source, options);
    if !analysis.syntax_errors.is_empty() {
        // Code that does not parse cannot run, so neither its complexity nor its coverage
        // means anything.
        return FileReport {
            path: path.to_path_buf(),
            functions: Vec::new(),
            measured: coverage.is_some(),
            syntax_errors: analysis.syntax_errors,
        };
    }
    let lines: Vec<&str> = source.lines().collect();

    let statements = coverage
        .and_then(|coverage| coverage.statements.as_ref())
        .unwrap_or(&analysis.statements);
    let executed: LineSet = coverage
        .map(|coverage| analysis.translate_lines(coverage.executed.iter().copied()))
        .unwrap_or_default();

    let functions = analysis
        .functions
        .iter()
        .filter(|function| !function.excluded)
        .map(|function| {
            let own: Vec<u32> = function
                .lines
                .iter()
                .filter(|line| statements.contains(line))
                .copied()
                .collect();
            let missed_lines: Vec<u32> = own
                .iter()
                .filter(|line| !executed.contains(line))
                .copied()
                .collect();
            let missed_ranges = missed_ranges(&own, &executed);
            let total = own.len() as u32;
            let covered = total - missed_lines.len() as u32;
            let coverage_fraction = if total == 0 {
                1.0
            } else {
                f64::from(covered) / f64::from(total)
            };
            let header = lines
                .get(function.start_line as usize - 1..function.signature_end_line as usize)
                .unwrap_or_default();

            FunctionReport {
                name: function.name.clone(),
                qualified_name: function.qualified_name.clone(),
                kind: function.kind,
                line: function.line,
                column: function.column,
                end_line: function.end_line,
                line_text: lines
                    .get(function.line as usize - 1)
                    .map(|line| (*line).to_string())
                    .unwrap_or_default(),
                complexity: function.complexity,
                statements: total,
                covered,
                missed_lines,
                missed_ranges,
                measured: coverage.is_some(),
                score: crap_score(function.complexity, coverage_fraction),
                suppressed: header.iter().any(|line| is_suppression(line)),
            }
        })
        .collect();

    FileReport {
        path: path.to_path_buf(),
        functions,
        measured: coverage.is_some(),
        syntax_errors: analysis.syntax_errors,
    }
}

/// Groups the unexecuted lines among `statements` (sorted) into ranges of consecutive
/// statements.
fn missed_ranges(statements: &[u32], executed: &LineSet) -> Vec<(u32, u32)> {
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    let mut in_run = false;
    for &line in statements {
        if executed.contains(&line) {
            in_run = false;
        } else {
            match ranges.last_mut() {
                Some((_, end)) if in_run => *end = line,
                _ => ranges.push((line, line)),
            }
            in_run = true;
        }
    }
    ranges
}

/// Whether a line carries a comment suppressing the CRAP check: `# crap: ignore`, a bare
/// `# noqa`, or a `# noqa` naming `CRAP001` (or the `CRAP` prefix).
fn is_suppression(line: &str) -> bool {
    static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)#\s*(?:crap\s*:\s*ignore\b|noqa(?P<codes>\s*:\s*[A-Z]+[0-9]*(?:[\s,]+[A-Z]+[0-9]*)*)?)")
            .expect("valid regex")
    });
    PATTERN.captures_iter(line).any(|captures| {
        let Some(codes) = captures.name("codes") else {
            return true;
        };
        codes
            .as_str()
            .trim_start_matches(|c: char| c.is_whitespace() || c == ':')
            .split(|c: char| c.is_whitespace() || c == ',')
            .any(|code| code.eq_ignore_ascii_case(CODE) || code.eq_ignore_ascii_case("CRAP"))
    })
}

#[cfg(test)]
mod tests;
