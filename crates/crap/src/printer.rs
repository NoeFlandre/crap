//! Output formats of `crap check`.

use std::fmt::Write as _;
use std::io::{self, Write};

use colored::Colorize;
use serde_json::{Value, json};

use crap_linter::{CODE, FileReport, FunctionReport, required_coverage};
use crap_workspace::OutputFormat;

use crate::commands::Run;
use crate::logging::{LogLevel, level};

const TOOL_URL: &str = "https://github.com/noeflandre/crap";

pub(crate) struct Printer<'a> {
    run: &'a Run,
    threshold: f64,
}

impl<'a> Printer<'a> {
    pub(crate) fn new(run: &'a Run) -> Self {
        Self {
            run,
            threshold: run.settings.threshold,
        }
    }

    /// The crappy functions that are not suppressed, in file and line order.
    fn crappy(&self) -> Vec<(&'a FileReport, &'a FunctionReport)> {
        self.run
            .analysis
            .functions()
            .filter(|(_, function)| function.is_crappy(self.threshold) && !function.suppressed)
            .collect()
    }

    /// Writes the report in `format`, returning the number of crappy functions.
    pub(crate) fn write(&self, writer: &mut dyn Write, format: OutputFormat) -> io::Result<usize> {
        let crappy = self.crappy();
        match format {
            OutputFormat::Full => self.write_text(writer, &crappy, true)?,
            OutputFormat::Concise => self.write_text(writer, &crappy, false)?,
            OutputFormat::Json => {
                let values: Vec<Value> = crappy
                    .iter()
                    .map(|(file, function)| function_json(self.run, file, function))
                    .collect();
                serde_json::to_writer_pretty(&mut *writer, &values)?;
                writeln!(writer)?;
            }
            OutputFormat::JsonLines => {
                for (file, function) in &crappy {
                    serde_json::to_writer(&mut *writer, &function_json(self.run, file, function))?;
                    writeln!(writer)?;
                }
            }
            OutputFormat::Github => self.write_github(writer, &crappy)?,
            OutputFormat::Gitlab => self.write_gitlab(writer, &crappy)?,
            OutputFormat::Sarif => self.write_sarif(writer, &crappy)?,
            OutputFormat::Junit => self.write_junit(writer)?,
        }
        Ok(crappy.len())
    }

    fn message(&self, function: &FunctionReport) -> String {
        format!(
            "`{}` has a CRAP score of {} (threshold: {})",
            function.qualified_name,
            score(function.score),
            number(self.threshold)
        )
    }

    /// The message with complexity and coverage, for formats without an annotated snippet.
    fn detailed_message(&self, function: &FunctionReport) -> String {
        format!(
            "`{}` has a CRAP score of {} (complexity {}, coverage {}, threshold {})",
            function.qualified_name,
            score(function.score),
            function.complexity,
            coverage(function),
            number(self.threshold)
        )
    }

    fn write_text(
        &self,
        writer: &mut dyn Write,
        crappy: &[(&FileReport, &FunctionReport)],
        full: bool,
    ) -> io::Result<()> {
        for (file, function) in crappy {
            let path = self.run.display_path(&file.path);
            write!(
                writer,
                "{}{}{}{}{}{} {} ",
                path.bold(),
                ":".cyan(),
                function.line,
                ":".cyan(),
                function.column,
                ":".cyan(),
                CODE.red().bold(),
            )?;
            if full {
                writeln!(writer, "{}", self.message(function))?;
                self.write_snippet(writer, function)?;
                writeln!(writer)?;
            } else {
                writeln!(writer, "{}", self.detailed_message(function))?;
            }
        }

        if level() < LogLevel::Default {
            return Ok(());
        }
        let total = self.run.analysis.functions().count();
        if crappy.is_empty() {
            writeln!(writer, "{}", "All checks passed!".green().bold())?;
        } else {
            let noun = if crappy.len() == 1 {
                "function"
            } else {
                "functions"
            };
            writeln!(
                writer,
                "Found {} crappy {noun} out of {total} (threshold: {}).",
                crappy.len().to_string().bold(),
                number(self.threshold)
            )?;
        }
        Ok(())
    }

    /// Writes the source line of a function's name with an annotation, in the style of
    /// Ruff's diagnostics.
    fn write_snippet(&self, writer: &mut dyn Write, function: &FunctionReport) -> io::Result<()> {
        let line_number = function.line.to_string();
        let gutter = " ".repeat(line_number.len());
        let bar = "|".blue().bold();

        let text = function.line_text.replace('\t', "    ");
        let prefix: String = function
            .line_text
            .chars()
            .take(function.column.saturating_sub(1) as usize)
            .collect();
        let offset = prefix.replace('\t', "    ").chars().count();
        let width = function.name.chars().count().max(1);

        let mut label = format!(
            "complexity {}, coverage {}",
            function.complexity,
            coverage(function)
        );
        if !function.measured {
            label.push_str(" (not in the coverage data)");
        } else if !function.missed_ranges.is_empty() {
            let _ = write!(label, " (missed lines {})", ranges(&function.missed_ranges));
        }

        writeln!(writer, "{gutter} {bar}")?;
        writeln!(
            writer,
            "{} {bar} {}",
            line_number.blue().bold(),
            text.trim_end()
        )?;
        writeln!(
            writer,
            "{gutter} {bar} {}{} {}",
            " ".repeat(offset),
            "^".repeat(width).red().bold(),
            label.red().bold()
        )?;
        writeln!(writer, "{gutter} {bar}")?;
        writeln!(
            writer,
            "{gutter} {} {}: {}",
            "=".blue().bold(),
            "help".bold(),
            help(function, self.threshold)
        )?;
        Ok(())
    }

    fn write_github(
        &self,
        writer: &mut dyn Write,
        crappy: &[(&FileReport, &FunctionReport)],
    ) -> io::Result<()> {
        for (file, function) in crappy {
            let path = self.run.display_path(&file.path);
            writeln!(
                writer,
                "::error title={},file={},line={},col={},endLine={}::{}",
                escape_property(&format!("crap ({CODE})")),
                escape_property(&path),
                function.line,
                function.column,
                function.end_line,
                escape_data(&format!(
                    "{path}:{}:{}: {CODE} {}",
                    function.line,
                    function.column,
                    self.detailed_message(function),
                )),
            )?;
        }
        Ok(())
    }

    fn write_gitlab(
        &self,
        writer: &mut dyn Write,
        crappy: &[(&FileReport, &FunctionReport)],
    ) -> io::Result<()> {
        let issues: Vec<Value> = crappy
            .iter()
            .map(|(file, function)| {
                let path = self.run.display_path(&file.path).replace('\\', "/");
                json!({
                    "check_name": CODE,
                    "description": self.detailed_message(function),
                    "categories": ["Complexity"],
                    "severity": "major",
                    "fingerprint": fingerprint(&path, &function.qualified_name),
                    "location": {
                        "path": path,
                        "lines": { "begin": function.line, "end": function.end_line },
                    },
                })
            })
            .collect();
        serde_json::to_writer_pretty(&mut *writer, &issues)?;
        writeln!(writer)
    }

    fn write_sarif(
        &self,
        writer: &mut dyn Write,
        crappy: &[(&FileReport, &FunctionReport)],
    ) -> io::Result<()> {
        let results: Vec<Value> = crappy
            .iter()
            .map(|(file, function)| {
                let uri = self.run.display_path(&file.path).replace('\\', "/");
                json!({
                    "ruleId": CODE,
                    "level": "error",
                    "message": { "text": self.detailed_message(function) },
                    "locations": [{
                        "physicalLocation": {
                            "artifactLocation": { "uri": uri },
                            "region": {
                                "startLine": function.line,
                                "startColumn": function.column,
                                "endLine": function.end_line,
                            },
                        },
                    }],
                    "properties": {
                        "function": function.qualified_name,
                        "complexity": function.complexity,
                        "coverage": function.coverage(),
                        "score": function.score,
                    },
                })
            })
            .collect();
        let log = json!({
            "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
            "version": "2.1.0",
            "runs": [{
                "tool": {
                    "driver": {
                        "name": "crap",
                        "version": env!("CARGO_PKG_VERSION"),
                        "informationUri": TOOL_URL,
                        "rules": [{
                            "id": CODE,
                            "name": "crappy-function",
                            "shortDescription": {
                                "text": "Function is too complex for how well it is tested",
                            },
                            "fullDescription": {
                                "text": "The CRAP score combines cyclomatic complexity (comp) and test coverage (cov): comp² × (1 − cov)³ + comp. Functions scoring above the threshold should be tested more thoroughly or simplified.",
                            },
                            "helpUri": TOOL_URL,
                            "defaultConfiguration": { "level": "error" },
                        }],
                    },
                },
                "results": results,
            }],
        });
        serde_json::to_writer_pretty(&mut *writer, &log)?;
        writeln!(writer)
    }

    /// Writes a JUnit report with one test case per function.
    fn write_junit(&self, writer: &mut dyn Write) -> io::Result<()> {
        let files = &self.run.analysis.files;
        let total = self.run.analysis.functions().count();
        let failures = self.crappy().len();
        writeln!(writer, r#"<?xml version="1.0" encoding="UTF-8"?>"#)?;
        writeln!(
            writer,
            r#"<testsuites name="crap" tests="{total}" failures="{failures}" errors="0">"#
        )?;
        for file in files.iter().filter(|file| !file.functions.is_empty()) {
            let path = xml_escape(&self.run.display_path(&file.path));
            let file_failures = file
                .functions
                .iter()
                .filter(|function| function.is_crappy(self.threshold) && !function.suppressed)
                .count();
            writeln!(
                writer,
                r#"  <testsuite name="{path}" tests="{}" failures="{file_failures}" errors="0" skipped="0">"#,
                file.functions.len()
            )?;
            for function in &file.functions {
                let name = xml_escape(&function.qualified_name);
                let crappy = function.is_crappy(self.threshold) && !function.suppressed;
                if crappy {
                    writeln!(
                        writer,
                        r#"    <testcase name="{name}" classname="{path}" file="{path}" line="{}">"#,
                        function.line
                    )?;
                    writeln!(
                        writer,
                        r#"      <failure message="{}">line {}, col {}: complexity {}, coverage {}</failure>"#,
                        xml_escape(&self.message(function)),
                        function.line,
                        function.column,
                        function.complexity,
                        coverage(function)
                    )?;
                    writeln!(writer, "    </testcase>")?;
                } else if function.suppressed {
                    writeln!(
                        writer,
                        r#"    <testcase name="{name}" classname="{path}" file="{path}" line="{}"><skipped message="suppressed"/></testcase>"#,
                        function.line
                    )?;
                } else {
                    writeln!(
                        writer,
                        r#"    <testcase name="{name}" classname="{path}" file="{path}" line="{}"/>"#,
                        function.line
                    )?;
                }
            }
            writeln!(writer, "  </testsuite>")?;
        }
        writeln!(writer, "</testsuites>")
    }
}

/// The JSON representation of a function, shared by `check` and `report`.
pub(crate) fn function_json(run: &Run, file: &FileReport, function: &FunctionReport) -> Value {
    let threshold = run.settings.threshold;
    json!({
        "code": CODE,
        "filename": file.path,
        "location": { "row": function.line, "column": function.column },
        "end_location": { "row": function.end_line },
        "function": function.qualified_name,
        "complexity": function.complexity,
        "coverage": function.coverage(),
        "statements": function.statements,
        "covered": function.covered,
        "missed_lines": function.missed_lines,
        "measured": function.measured,
        "score": function.score,
        "threshold": threshold,
        "crappy": function.is_crappy(threshold),
        "suppressed": function.suppressed,
    })
}

fn help(function: &FunctionReport, threshold: f64) -> String {
    match required_coverage(function.complexity, threshold) {
        None => format!(
            "Its complexity alone ({}) exceeds the threshold: split it into smaller functions",
            function.complexity
        ),
        Some(required) => {
            let percent = (required * 100.0).ceil();
            if percent >= 100.0 {
                "Cover all of it with tests, or reduce its complexity".to_string()
            } else {
                format!("Cover at least {percent:.0}% of it with tests, or reduce its complexity")
            }
        }
    }
}

/// Formats a function's coverage as a percentage, e.g. `14.3%`.
pub(crate) fn coverage(function: &FunctionReport) -> String {
    format!("{:.1}%", function.coverage() * 100.0)
}

/// Formats a CRAP score with one decimal, e.g. `56.0`.
pub(crate) fn score(score: f64) -> String {
    format!("{score:.1}")
}

/// Formats a number without a trailing `.0`, e.g. `30` or `12.5`.
pub(crate) fn number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

/// Formats line ranges like coverage.py: `3-5, 9, 12-14`, abbreviated after a few ranges.
fn ranges(ranges: &[(u32, u32)]) -> String {
    const MAX: usize = 5;
    let mut parts: Vec<String> = ranges
        .iter()
        .take(MAX)
        .map(|&(start, end)| {
            if start == end {
                start.to_string()
            } else {
                format!("{start}-{end}")
            }
        })
        .collect();
    if ranges.len() > MAX {
        parts.push("...".to_string());
    }
    parts.join(", ")
}

/// Escapes the data of a GitHub workflow command.
fn escape_data(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// Escapes a property of a GitHub workflow command.
fn escape_property(value: &str) -> String {
    escape_data(value).replace(':', "%3A").replace(',', "%2C")
}

fn xml_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            c => escaped.push(c),
        }
    }
    escaped
}

/// A stable fingerprint for GitLab, identifying a function across runs (FNV-1a).
fn fingerprint(path: &str, function: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in path.bytes().chain([0]).chain(function.bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting() {
        assert_eq!(number(30.0), "30");
        assert_eq!(number(12.5), "12.5");
        assert_eq!(score(56.0), "56.0");
        assert_eq!(ranges(&[(3, 5), (9, 9)]), "3-5, 9");
        assert_eq!(
            ranges(&[(1, 1), (3, 3), (5, 5), (7, 7), (9, 9), (11, 11)]),
            "1, 3, 5, 7, 9, ..."
        );
        assert_eq!(escape_property("a:b,c%"), "a%3Ab%2Cc%25");
        assert_eq!(xml_escape("<a & 'b'>"), "&lt;a &amp; &apos;b&apos;&gt;");
        assert_eq!(fingerprint("a.py", "f"), fingerprint("a.py", "f"));
        assert_ne!(fingerprint("a.py", "f"), fingerprint("a.py", "g"));
    }
}
