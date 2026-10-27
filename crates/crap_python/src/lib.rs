//! Static analysis of Python source code for `crap`.
//!
//! Given the text of a Python module, this crate finds every function and method, computes
//! its cyclomatic complexity, and determines which lines of its body hold executable
//! statements. The line model mirrors [coverage.py](https://coverage.readthedocs.io) so that
//! per-function coverage computed from raw coverage data agrees with coverage.py's own
//! reports.
//!
//! Parsing is done with Ruff's Python parser, so every syntax supported by Ruff (Python 3.14
//! included) is supported here.

use std::collections::BTreeSet;

use ruff_python_ast::token::Tokens;
use ruff_python_ast::{ModModule, PySourceType};
use ruff_python_parser::{Parsed, parse_unchecked_source};
use ruff_source_file::LineIndex;
use ruff_text_size::{Ranged, TextSize};

pub use crate::complexity::complexity;
pub use crate::exclusion::{DEFAULT_EXCLUDE, ExclusionPatterns, InvalidPattern};
pub use crate::functions::{Function, FunctionKind};

mod complexity;
mod exclusion;
mod functions;
mod lines;
mod statements;

/// A set of one-based line numbers.
pub type LineSet = BTreeSet<u32>;

/// Options controlling how a module is analyzed.
#[derive(Debug, Clone, Default)]
pub struct AnalysisOptions {
    /// Patterns marking lines as excluded from coverage measurement, like coverage.py's
    /// `exclude_lines` setting. A matching line excludes its whole statement, and when the
    /// statement opens a block (`def`, `if`, ...), the whole block.
    pub exclude: ExclusionPatterns,
}

/// A syntax error found while parsing a module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub message: String,
    pub line: u32,
    pub column: u32,
}

/// The result of analyzing a single Python module.
#[derive(Debug)]
pub struct ModuleAnalysis {
    /// Every function and method in the module, in source order.
    pub functions: Vec<Function>,
    /// Lines holding executable statements, after exclusions, normalized to the first line
    /// of multi-line statements (coverage.py's `statements`).
    pub statements: LineSet,
    /// Lines excluded from measurement by an exclusion pattern.
    pub excluded: LineSet,
    /// Syntax errors in the module. When non-empty, the rest of the analysis is best-effort.
    pub syntax_errors: Vec<SyntaxError>,
    multiline: lines::MultilineMap,
}

impl ModuleAnalysis {
    /// Maps a line to the first line of the logical line (statement) that contains it.
    pub fn first_line(&self, line: u32) -> u32 {
        self.multiline.first_line(line)
    }

    /// Normalizes raw line numbers, as recorded by a tracer, to the first lines of their
    /// statements.
    pub fn translate_lines(&self, lines: impl IntoIterator<Item = u32>) -> LineSet {
        lines
            .into_iter()
            .map(|line| self.first_line(line))
            .collect()
    }
}

/// Analyzes the Python module in `source`.
pub fn analyze(source: &str, options: &AnalysisOptions) -> ModuleAnalysis {
    let parsed = parse_unchecked_source(source, PySourceType::Python);
    let index = LineIndex::from_source_text(source);
    let locator = Locator {
        source,
        index: &index,
        tokens: parsed.tokens(),
    };

    let syntax_errors = parsed
        .errors()
        .iter()
        .map(|error| {
            let location = locator.line_column(error.location.start());
            SyntaxError {
                message: error.error.to_string(),
                line: location.0,
                column: location.1,
            }
        })
        .collect();

    analyze_parsed(&parsed, &locator, options, syntax_errors)
}

fn analyze_parsed(
    parsed: &Parsed<ModModule>,
    locator: &Locator<'_>,
    options: &AnalysisOptions,
    syntax_errors: Vec<SyntaxError>,
) -> ModuleAnalysis {
    let module = parsed.syntax();
    let multiline = lines::MultilineMap::from_tokens(locator);

    let raw_statements = statements::raw_statements(&module.body, locator);
    let docstrings = statements::docstrings(&module.body, locator);
    let excluded = exclusion::excluded_lines(
        &module.body,
        locator,
        &multiline,
        &options.exclude,
        &raw_statements,
    );

    let statements: LineSet = raw_statements
        .iter()
        .filter(|line| !excluded.contains(line) && !docstrings.contains(line))
        .map(|&line| multiline.first_line(line))
        .filter(|line| !excluded.contains(line) && !docstrings.contains(line))
        .collect();

    let functions = functions::find_functions(&module.body, locator, &excluded);

    ModuleAnalysis {
        functions,
        statements,
        excluded,
        syntax_errors,
        multiline,
    }
}

/// Source text together with the index needed to convert offsets to lines.
pub(crate) struct Locator<'a> {
    pub(crate) source: &'a str,
    pub(crate) index: &'a LineIndex,
    pub(crate) tokens: &'a Tokens,
}

impl Locator<'_> {
    /// The one-based line containing `offset`.
    pub(crate) fn line(&self, offset: TextSize) -> u32 {
        self.index.line_index(offset).get() as u32
    }

    /// The one-based line and (character) column of `offset`.
    pub(crate) fn line_column(&self, offset: TextSize) -> (u32, u32) {
        let location = self.index.line_column(offset, self.source);
        (location.line.get() as u32, location.column.get() as u32)
    }

    /// The one-based line on which `node` starts.
    pub(crate) fn start_line(&self, node: &impl Ranged) -> u32 {
        self.line(node.start())
    }

    /// The one-based line on which `node` ends.
    pub(crate) fn end_line(&self, node: &impl Ranged) -> u32 {
        // A node's range is exclusive; step back so a trailing newline is not counted.
        let end = node.end();
        if end > node.start() {
            self.line(end - TextSize::from(1))
        } else {
            self.line(end)
        }
    }
}

#[cfg(test)]
mod tests;
