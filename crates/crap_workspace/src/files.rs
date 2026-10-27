use std::path::{Path, PathBuf};
use std::sync::Mutex;

use globset::{Glob, GlobBuilder, GlobSet, GlobSetBuilder};
use ignore::{WalkBuilder, WalkState};

use crate::Settings;

/// Glob patterns matching files and directories, in the style of Ruff's `exclude`:
///
/// * a pattern without a slash (`build`, `*_pb2.py`) matches a file or directory name anywhere;
/// * a pattern with a slash (`tests/`, `src/legacy/*.py`) matches a path relative to the
///   project root, and everything below it.
#[derive(Debug, Clone)]
pub struct FilePatternSet {
    names: GlobSet,
    paths: GlobSet,
    root: PathBuf,
}

impl FilePatternSet {
    pub fn new(root: &Path, patterns: &[String]) -> Result<Self, String> {
        let mut names = GlobSetBuilder::new();
        let mut paths = GlobSetBuilder::new();
        for pattern in patterns {
            let invalid =
                |err: globset::Error| format!("invalid exclude pattern `{pattern}`: {err}");
            let trimmed = pattern.trim_start_matches("./").trim_end_matches('/');
            if trimmed.contains('/') || pattern.ends_with('/') && trimmed.contains('*') {
                let glob = |pattern: &str| {
                    GlobBuilder::new(pattern)
                        .literal_separator(true)
                        .build()
                        .map_err(invalid)
                };
                paths.add(glob(trimmed)?);
                paths.add(glob(&format!("{trimmed}/**"))?);
            } else if pattern.ends_with('/') {
                // `tests/` means the directory at the root, not any directory named `tests`.
                paths.add(Glob::new(trimmed).map_err(invalid)?);
                paths.add(Glob::new(&format!("{trimmed}/**")).map_err(invalid)?);
            } else {
                names.add(Glob::new(trimmed).map_err(invalid)?);
            }
        }
        Ok(Self {
            names: names.build().map_err(|err| err.to_string())?,
            paths: paths.build().map_err(|err| err.to_string())?,
            root: root.to_path_buf(),
        })
    }

    /// Whether `path` (absolute) is matched by a pattern.
    pub fn is_match(&self, path: &Path) -> bool {
        if path
            .file_name()
            .is_some_and(|name| self.names.is_match(Path::new(name)))
        {
            return true;
        }
        path.strip_prefix(&self.root)
            .is_ok_and(|relative| self.paths.is_match(relative))
    }
}

/// The Python files to check.
#[derive(Debug, Default)]
pub struct PythonFiles {
    /// Absolute paths, sorted.
    pub files: Vec<PathBuf>,
    /// Errors met while walking directories.
    pub errors: Vec<String>,
}

/// Finds the Python files under `paths` (absolute), honoring the exclusion settings.
///
/// Files given explicitly are always included; files found by walking a directory are
/// included when they have a `.py` or `.pyw` extension and are not excluded.
pub fn find_python_files(paths: &[PathBuf], settings: &Settings) -> PythonFiles {
    let mut result = PythonFiles::default();
    let mut directories = Vec::new();
    for path in paths {
        if path.is_file() {
            result.files.push(path.clone());
        } else if path.is_dir() {
            directories.push(path.clone());
        } else {
            result
                .errors
                .push(format!("{}: No such file or directory", path.display()));
        }
    }

    if let Some((first, rest)) = directories.split_first() {
        let mut builder = WalkBuilder::new(first);
        for directory in rest {
            builder.add(directory);
        }
        let exclude = settings.exclude.clone();
        builder
            .standard_filters(settings.respect_gitignore)
            .hidden(false)
            .require_git(false)
            .filter_entry(move |entry| entry.depth() == 0 || !exclude.is_match(entry.path()));

        let found = Mutex::new(Vec::new());
        let errors = Mutex::new(Vec::new());
        builder.build_parallel().run(|| {
            Box::new(|entry| {
                match entry {
                    Ok(entry) => {
                        let path = entry.path();
                        if entry.file_type().is_some_and(|kind| kind.is_file())
                            && path
                                .extension()
                                .is_some_and(|extension| extension == "py" || extension == "pyw")
                        {
                            found
                                .lock()
                                .expect("no panics while holding the lock")
                                .push(path.to_path_buf());
                        }
                    }
                    Err(err) => errors
                        .lock()
                        .expect("no panics while holding the lock")
                        .push(err.to_string()),
                }
                WalkState::Continue
            })
        });
        result.files.extend(found.into_inner().unwrap_or_default());
        result
            .errors
            .extend(errors.into_inner().unwrap_or_default());
    }

    result.files.sort();
    result.files.dedup();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SettingsOverrides;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
    }

    fn relative(root: &Path, files: &PythonFiles) -> Vec<String> {
        files
            .files
            .iter()
            .map(|file| {
                file.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    }

    #[test]
    fn discovery() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for file in [
            "src/pkg/__init__.py",
            "src/pkg/mod.py",
            "src/pkg/stub.pyi",
            "src/pkg/data.txt",
            "src/pkg/gen_pb2.py",
            ".venv/lib/x.py",
            "build/lib/y.py",
            "tests/test_mod.py",
            "docs/tests/conf.py",
            "ignored/z.py",
        ] {
            touch(&root.join(file));
        }
        std::fs::write(root.join(".gitignore"), "ignored/\n").unwrap();
        std::fs::write(
            root.join("crap.toml"),
            "extend-exclude = [\"tests/\", \"*_pb2.py\"]\n",
        )
        .unwrap();

        let settings = Settings::resolve(root, None, SettingsOverrides::default()).unwrap();
        let files = find_python_files(&[root.to_path_buf()], &settings);
        assert!(files.errors.is_empty(), "{:?}", files.errors);
        assert_eq!(
            relative(root, &files),
            [
                "docs/tests/conf.py",
                "src/pkg/__init__.py",
                "src/pkg/mod.py"
            ]
        );

        // Explicit files are always checked.
        let explicit = root.join("tests/test_mod.py");
        let files = find_python_files(std::slice::from_ref(&explicit), &settings);
        assert_eq!(files.files, [explicit]);

        let overrides = SettingsOverrides {
            respect_gitignore: Some(false),
            ..SettingsOverrides::default()
        };
        let settings = Settings::resolve(root, None, overrides).unwrap();
        let files = find_python_files(&[root.join("ignored")], &settings);
        assert_eq!(relative(root, &files), ["ignored/z.py"]);
    }

    #[test]
    fn missing_path() {
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings::resolve(dir.path(), None, SettingsOverrides::default()).unwrap();
        let files = find_python_files(&[dir.path().join("nope")], &settings);
        assert_eq!(files.errors.len(), 1);
    }

    #[test]
    fn patterns() {
        let root = Path::new("/project");
        let set = FilePatternSet::new(
            root,
            &[
                "build".to_string(),
                "tests/".to_string(),
                "src/legacy/*.py".to_string(),
            ],
        )
        .unwrap();
        assert!(set.is_match(Path::new("/project/build")));
        assert!(set.is_match(Path::new("/project/src/build")));
        assert!(set.is_match(Path::new("/project/tests")));
        assert!(set.is_match(Path::new("/project/tests/unit/test_x.py")));
        assert!(!set.is_match(Path::new("/project/src/tests")));
        assert!(set.is_match(Path::new("/project/src/legacy/old.py")));
        assert!(!set.is_match(Path::new("/project/src/legacy/sub/old.py")));
        assert!(!set.is_match(Path::new("/project/src/pkg/mod.py")));
    }
}
