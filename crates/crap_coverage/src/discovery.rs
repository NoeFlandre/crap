//! Finding the coverage data of a project.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::CoveragePyConfig;

/// Report files looked for in the project root, in addition to coverage.py's data file.
const REPORTS: &[&str] = &[
    "coverage.json",
    "coverage.xml",
    "coverage.lcov",
    "lcov.info",
];

/// Finds the coverage data for the project rooted at `root`.
///
/// The candidates are coverage.py's data file (honoring `COVERAGE_FILE` and the `data_file`
/// setting), its unmerged parallel data files (`.coverage.*`), and the reports in
/// [`REPORTS`]. When several exist, the most recently modified wins, so a stale report never
/// shadows fresh data.
pub fn discover(root: &Path, config: &CoveragePyConfig) -> Vec<PathBuf> {
    let data_file = std::env::var_os("COVERAGE_FILE")
        .map(PathBuf::from)
        .or_else(|| config.data_file.as_ref().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(".coverage"));
    let data_file = root.join(data_file);

    let mut candidates: Vec<Vec<PathBuf>> = Vec::new();
    if data_file.is_file() {
        candidates.push(vec![data_file]);
    } else {
        let parallel = parallel_data_files(&data_file);
        if !parallel.is_empty() {
            candidates.push(parallel);
        }
    }
    for report in REPORTS {
        let path = root.join(report);
        if path.is_file() {
            candidates.push(vec![path]);
        }
    }

    candidates
        .into_iter()
        .max_by_key(|group| group.iter().filter_map(|path| modified(path)).max())
        .unwrap_or_default()
}

/// Data files written with `parallel = true`, named `<data_file>.<suffix>`.
fn parallel_data_files(data_file: &Path) -> Vec<PathBuf> {
    let (Some(directory), Some(name)) = (data_file.parent(), data_file.file_name()) else {
        return Vec::new();
    };
    let prefix = format!("{}.", name.to_string_lossy());
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with(&prefix) && !name.ends_with("-journal")
        })
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    files.sort();
    files
}

fn modified(path: &Path) -> Option<SystemTime> {
    path.metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
}

#[cfg(test)]
mod tests {
    use std::fs::{File, FileTimes};
    use std::time::Duration;

    use super::*;

    fn touch(path: &Path, age: u64) {
        let file = File::create(path).unwrap();
        let time = SystemTime::now() - Duration::from_secs(age);
        file.set_times(FileTimes::new().set_modified(time)).unwrap();
    }

    #[test]
    fn newest_wins() {
        let dir = tempfile::tempdir().unwrap();
        let config = CoveragePyConfig::default();
        assert!(discover(dir.path(), &config).is_empty());

        touch(&dir.path().join(".coverage"), 100);
        touch(&dir.path().join("coverage.xml"), 200);
        assert_eq!(
            discover(dir.path(), &config),
            [dir.path().join(".coverage")]
        );

        touch(&dir.path().join("coverage.json"), 10);
        assert_eq!(
            discover(dir.path(), &config),
            [dir.path().join("coverage.json")]
        );
    }

    #[test]
    fn parallel_files() {
        let dir = tempfile::tempdir().unwrap();
        touch(&dir.path().join(".coverage.host.1.abc"), 0);
        touch(&dir.path().join(".coverage.host.2.def"), 0);
        touch(&dir.path().join(".coveragerc"), 0);
        assert_eq!(
            discover(dir.path(), &CoveragePyConfig::default()),
            [
                dir.path().join(".coverage.host.1.abc"),
                dir.path().join(".coverage.host.2.def")
            ]
        );
    }

    #[test]
    fn configured_data_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("build")).unwrap();
        touch(&dir.path().join("build/.cov"), 0);
        let config = CoveragePyConfig {
            data_file: Some("build/.cov".to_string()),
            ..CoveragePyConfig::default()
        };
        assert_eq!(
            discover(dir.path(), &config),
            [dir.path().join("build/.cov")]
        );
    }
}
