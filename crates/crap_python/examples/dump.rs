//! Dumps the analysis of Python files as JSON lines, for comparison with coverage.py and radon.
#![allow(clippy::print_stdout)]

use crap_python::{AnalysisOptions, analyze};

fn main() {
    let options = AnalysisOptions::default();
    for path in std::env::args().skip(1) {
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let analysis = analyze(&source, &options);
        let functions: Vec<_> = analysis
            .functions
            .iter()
            .map(|function| {
                serde_json::json!({
                    "name": function.qualified_name,
                    "line": function.line,
                    "complexity": function.complexity,
                    "lines": function.lines,
                    "excluded": function.excluded,
                })
            })
            .collect();
        let translated: Vec<(u32, u32)> = (1..=source.lines().count() as u32 + 1)
            .map(|line| (line, analysis.first_line(line)))
            .filter(|(line, first)| line != first)
            .collect();
        println!(
            "{}",
            serde_json::json!({
                "path": path,
                "statements": analysis.statements,
                "excluded": analysis.excluded,
                "multiline": translated,
                "syntax_errors": analysis.syntax_errors.len(),
                "functions": functions,
            })
        );
    }
}
