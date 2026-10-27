//! LCOV tracefiles (`coverage lcov`, and most JavaScript and native coverage tools).
//!
//! Only `SF` (source file) and `DA` (line data) records are used.

use std::collections::BTreeMap;

use crate::FileCoverage;

pub(crate) fn read(contents: &[u8]) -> Result<BTreeMap<String, FileCoverage>, String> {
    let text = std::str::from_utf8(contents).map_err(|err| err.to_string())?;
    let mut files: BTreeMap<String, FileCoverage> = BTreeMap::new();
    let mut current: Option<(String, FileCoverage)> = None;

    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if let Some(path) = line.strip_prefix("SF:") {
            current = Some((
                path.to_string(),
                FileCoverage {
                    statements: Some(crate::LineSet::new()),
                    ..FileCoverage::default()
                },
            ));
        } else if let Some(data) = line.strip_prefix("DA:") {
            let Some((_, file)) = &mut current else {
                return Err(format!("line {}: `DA` record outside of a file", index + 1));
            };
            let mut fields = data.split(',');
            let parse = |field: Option<&str>| {
                field
                    .and_then(|field| field.trim().parse::<u64>().ok())
                    .ok_or_else(|| format!("line {}: invalid `DA` record `{line}`", index + 1))
            };
            let number = u32::try_from(parse(fields.next())?)
                .map_err(|_| format!("line {}: line number out of range", index + 1))?;
            let hits = parse(fields.next())?;
            if let Some(statements) = &mut file.statements {
                statements.insert(number);
            }
            if hits > 0 {
                file.executed.insert(number);
            }
        } else if line == "end_of_record"
            && let Some((path, file)) = current.take()
        {
            files.entry(path).or_default().merge(file);
        }
    }
    if let Some((path, file)) = current {
        files.entry(path).or_default().merge(file);
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LineSet;

    #[test]
    fn tracefile() {
        let contents = b"TN:\nSF:pkg/mod.py\nFN:1,f\nDA:1,1\nDA:2,0\nDA:3,4,abc\nLF:3\nLH:2\nend_of_record\nSF:other.py\nDA:1,0\nend_of_record\n";
        let files = read(contents).unwrap();
        assert_eq!(files["pkg/mod.py"].executed, LineSet::from([1, 3]));
        assert_eq!(
            files["pkg/mod.py"].statements,
            Some(LineSet::from([1, 2, 3]))
        );
        assert_eq!(files["other.py"].executed, LineSet::new());
    }

    #[test]
    fn invalid_record() {
        assert!(read(b"SF:a.py\nDA:x,1\n").is_err());
        assert!(read(b"DA:1,1\n").is_err());
    }
}
