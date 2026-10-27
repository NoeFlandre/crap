//! coverage.py's JSON report (`coverage json`).

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::{FileCoverage, LineSet};

#[derive(Deserialize)]
struct Report {
    files: BTreeMap<String, File>,
}

#[derive(Deserialize)]
struct File {
    executed_lines: LineSet,
    missing_lines: LineSet,
}

pub(crate) fn read(contents: &[u8]) -> Result<BTreeMap<String, FileCoverage>, String> {
    let report: Report = serde_json::from_slice(contents).map_err(|err| err.to_string())?;
    Ok(report
        .files
        .into_iter()
        .map(|(path, file)| {
            let mut statements = file.missing_lines;
            statements.extend(&file.executed_lines);
            let coverage = FileCoverage {
                executed: file.executed_lines,
                statements: Some(statements),
            };
            (path, coverage)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report() {
        let contents = br#"{
            "meta": {"format": 3, "version": "7.10.0"},
            "files": {
                "pkg/mod.py": {
                    "executed_lines": [1, 2, 4],
                    "missing_lines": [5],
                    "excluded_lines": [7],
                    "summary": {}
                }
            },
            "totals": {}
        }"#;
        let files = read(contents).unwrap();
        let file = &files["pkg/mod.py"];
        assert_eq!(file.executed, LineSet::from([1, 2, 4]));
        assert_eq!(file.statements, Some(LineSet::from([1, 2, 4, 5])));
    }

    #[test]
    fn invalid() {
        assert!(read(b"{\"meta\": {}}").is_err());
    }
}
