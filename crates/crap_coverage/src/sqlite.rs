//! coverage.py's SQLite data file.
//!
//! See <https://coverage.readthedocs.io/en/latest/dbschema.html>. Executed lines are stored
//! per file and context, either as "numbits" blobs in `line_bits`, or, when branch coverage is
//! measured, as `(from, to)` pairs in `arc`.

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use crate::{FileCoverage, LineSet};

pub(crate) fn read(path: &Path) -> Result<BTreeMap<String, FileCoverage>, String> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|err| err.to_string())?;
    read_connection(&connection).map_err(|err| err.to_string())
}

fn read_connection(connection: &Connection) -> rusqlite::Result<BTreeMap<String, FileCoverage>> {
    let mut paths = BTreeMap::new();
    let mut statement = connection.prepare("SELECT id, path FROM file")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (id, path) = row?;
        paths.insert(id, path);
    }

    let mut lines: BTreeMap<i64, LineSet> = BTreeMap::new();

    if table_exists(connection, "line_bits")? {
        let mut statement = connection.prepare("SELECT file_id, numbits FROM line_bits")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;
        for row in rows {
            let (file_id, numbits) = row?;
            lines
                .entry(file_id)
                .or_default()
                .extend(numbits_to_lines(&numbits));
        }
    }

    if table_exists(connection, "arc")? {
        let mut statement = connection.prepare("SELECT file_id, fromno, tono FROM arc")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        for row in rows {
            let (file_id, from, to) = row?;
            let entry = lines.entry(file_id).or_default();
            // Negative line numbers mark entry to and exit from a code object.
            for line in [from, to] {
                if let Ok(line @ 1..) = u32::try_from(line) {
                    entry.insert(line);
                }
            }
        }
    }

    Ok(paths
        .into_iter()
        .map(|(id, path)| {
            let coverage = FileCoverage {
                executed: lines.remove(&id).unwrap_or_default(),
                statements: None,
            };
            (path, coverage)
        })
        .collect())
}

fn table_exists(connection: &Connection, name: &str) -> rusqlite::Result<bool> {
    connection
        .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")?
        .exists([name])
}

/// Decodes coverage.py's "numbits": bit `n` of the blob is set when line `n` was executed.
pub(crate) fn numbits_to_lines(numbits: &[u8]) -> impl Iterator<Item = u32> + '_ {
    numbits.iter().enumerate().flat_map(|(index, &byte)| {
        (0..8u32)
            .filter(move |bit| byte & (1 << bit) != 0)
            .map(move |bit| index as u32 * 8 + bit)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbits() {
        // coverage.numbits.nums_to_numbits([1, 2, 3, 9, 17])
        let numbits = [0b0000_1110, 0b0000_0010, 0b0000_0010];
        assert_eq!(
            numbits_to_lines(&numbits).collect::<Vec<_>>(),
            [1, 2, 3, 9, 17]
        );
    }

    #[test]
    fn line_and_arc_data() -> rusqlite::Result<()> {
        let connection = Connection::open_in_memory()?;
        connection.execute_batch(
            "CREATE TABLE file (id INTEGER PRIMARY KEY, path TEXT);
             CREATE TABLE line_bits (file_id INTEGER, context_id INTEGER, numbits BLOB);
             CREATE TABLE arc (file_id INTEGER, context_id INTEGER, fromno INTEGER, tono INTEGER);
             INSERT INTO file VALUES (1, '/src/a.py'), (2, 'b.py'), (3, 'unrun.py');
             INSERT INTO line_bits VALUES (1, 1, X'06'), (1, 2, X'0001');
             INSERT INTO arc VALUES (2, 1, -1, 3), (2, 1, 3, 4), (2, 1, 4, -1);",
        )?;
        let files = read_connection(&connection)?;
        assert_eq!(files["/src/a.py"].executed, LineSet::from([1, 2, 8]));
        assert_eq!(files["b.py"].executed, LineSet::from([3, 4]));
        assert_eq!(files["unrun.py"].executed, LineSet::new());
        assert_eq!(files["b.py"].statements, None);
        Ok(())
    }
}
