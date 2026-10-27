//! Matching source files on disk to the paths recorded in coverage data.
//!
//! Recorded paths are absolute (coverage.py's default), relative to the directory tests ran
//! in (`relative_files = true`, LCOV and JSON reports), or relative to a `<source>` directory
//! (Cobertura). Data produced on another machine, such as a CI runner, records paths that do
//! not exist locally. We therefore match exactly when we can, and otherwise by the longest
//! common path suffix.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use crate::{CoverageData, FileCoverage};

/// Resolves source files to their coverage data.
#[derive(Debug)]
pub struct PathResolver<'a> {
    data: &'a CoverageData,
    /// Normalized absolute paths to recorded paths.
    exact: HashMap<String, &'a str>,
    /// File names to the recorded paths ending with them.
    by_name: HashMap<String, Vec<Candidate<'a>>>,
}

#[derive(Debug)]
struct Candidate<'a> {
    key: &'a str,
    components: Vec<String>,
    absolute: bool,
}

impl<'a> PathResolver<'a> {
    /// Indexes `data`, resolving relative paths against its roots and then `project_root`.
    pub fn new(data: &'a CoverageData, project_root: &Path) -> Self {
        let mut exact = HashMap::new();
        let mut by_name: HashMap<String, Vec<Candidate<'a>>> = HashMap::new();

        for key in data.files.keys() {
            let key = key.as_str();
            let absolute = is_absolute(key);
            if absolute {
                exact.insert(normalize(key), key);
            } else {
                for root in data
                    .roots
                    .iter()
                    .map(PathBuf::as_path)
                    .chain([project_root])
                {
                    let joined = format!("{}/{key}", root.to_string_lossy());
                    exact.entry(normalize(&joined)).or_insert(key);
                }
            }
            let components = components(&normalize(key));
            if let Some(name) = components.last() {
                by_name.entry(name.clone()).or_default().push(Candidate {
                    key,
                    components,
                    absolute,
                });
            }
        }

        Self {
            data,
            exact,
            by_name,
        }
    }

    /// Finds the coverage data for the source file at `path`, an absolute path.
    pub fn resolve(&self, path: &Path) -> Option<(&'a str, &'a FileCoverage)> {
        let normalized = normalize(&path.to_string_lossy());
        if let Some(key) = self.exact.get(&normalized) {
            return Some((key, &self.data.files[*key]));
        }

        let wanted = components(&normalized);
        let candidates = self.by_name.get(wanted.last()?)?;
        let mut best: Option<(usize, &Candidate<'a>)> = None;
        let mut tied = false;
        for candidate in candidates {
            let common = wanted
                .iter()
                .rev()
                .zip(candidate.components.iter().rev())
                .take_while(|(a, b)| a == b)
                .count();
            // A relative path must match in full; an absolute path from another machine must
            // at least agree on the file's directory.
            let acceptable = if candidate.absolute {
                common >= 2
            } else {
                common == candidate.components.len()
            };
            if !acceptable {
                continue;
            }
            match best {
                Some((score, _)) if score > common => {}
                Some((score, _)) if score == common => tied = true,
                _ => {
                    best = Some((common, candidate));
                    tied = false;
                }
            }
        }
        if tied {
            tracing::debug!(
                "`{}` matches several files in the coverage data equally well",
                path.display()
            );
            return None;
        }
        best.map(|(_, candidate)| (candidate.key, &self.data.files[candidate.key]))
    }
}

fn is_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with('/')
        || path.starts_with('\\')
        || (bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
}

/// Normalizes a path lexically: forward slashes, no `.` or `..` components, and (on Windows,
/// where paths are case-insensitive) lowercase.
fn normalize(path: &str) -> String {
    let path = path.replace('\\', "/");
    let mut parts: Vec<&str> = Vec::new();
    for component in Path::new(&path).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if parts.last().is_some_and(|last| *last != "..") {
                    parts.pop();
                } else {
                    parts.push("..");
                }
            }
            Component::RootDir => parts.push(""),
            Component::Prefix(prefix) => parts.push(prefix.as_os_str().to_str().unwrap_or("")),
            Component::Normal(part) => parts.push(part.to_str().unwrap_or("")),
        }
    }
    let mut normalized = parts.join("/");
    if normalized.is_empty() && path.starts_with('/') {
        normalized.push('/');
    }
    if cfg!(windows) {
        normalized.make_ascii_lowercase();
    }
    normalized
}

fn components(normalized: &str) -> Vec<String> {
    normalized
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(all(test, unix))]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn data(paths: &[&str], roots: &[&str]) -> CoverageData {
        CoverageData {
            files: paths
                .iter()
                .map(|path| ((*path).to_string(), FileCoverage::default()))
                .collect(),
            roots: roots.iter().map(PathBuf::from).collect(),
            sources: Vec::new(),
        }
    }

    fn resolve<'a>(resolver: &PathResolver<'a>, path: &str) -> Option<&'a str> {
        resolver.resolve(Path::new(path)).map(|(key, _)| key)
    }

    #[test]
    fn exact_absolute() {
        let data = data(&["/project/src/pkg/mod.py"], &[]);
        let resolver = PathResolver::new(&data, Path::new("/project"));
        assert_eq!(
            resolve(&resolver, "/project/src/pkg/./mod.py"),
            Some("/project/src/pkg/mod.py")
        );
    }

    #[test]
    fn relative_to_roots() {
        let data = data(&["pkg/mod.py", "src/other.py"], &["/project/src"]);
        let resolver = PathResolver::new(&data, Path::new("/project"));
        assert_eq!(
            resolve(&resolver, "/project/src/pkg/mod.py"),
            Some("pkg/mod.py")
        );
        assert_eq!(
            resolve(&resolver, "/project/src/other.py"),
            Some("src/other.py")
        );
    }

    #[test]
    fn data_from_another_machine() {
        let data = data(
            &[
                "/home/runner/work/app/app/src/app/__init__.py",
                "/home/runner/work/app/app/src/app/cli/__init__.py",
                "C:\\a\\app\\app\\src\\app\\core.py",
            ],
            &[],
        );
        let resolver = PathResolver::new(&data, Path::new("/Users/me/code/app"));
        assert_eq!(
            resolve(&resolver, "/Users/me/code/app/src/app/__init__.py"),
            Some("/home/runner/work/app/app/src/app/__init__.py")
        );
        assert_eq!(
            resolve(&resolver, "/Users/me/code/app/src/app/cli/__init__.py"),
            Some("/home/runner/work/app/app/src/app/cli/__init__.py")
        );
        assert_eq!(
            resolve(&resolver, "/Users/me/code/app/src/app/core.py"),
            Some("C:\\a\\app\\app\\src\\app\\core.py")
        );
    }

    #[test]
    fn file_name_alone_is_not_enough() {
        let data = data(&["/ci/src/pkg/setup.py", "tests/conftest.py"], &[]);
        let resolver = PathResolver::new(&data, Path::new("/local"));
        assert_eq!(resolve(&resolver, "/local/setup.py"), None);
        assert_eq!(resolve(&resolver, "/local/src/conftest.py"), None);
    }

    #[test]
    fn ambiguous_matches_are_rejected() {
        let data = data(&["/ci/a/pkg/mod.py", "/ci/b/pkg/mod.py"], &[]);
        let resolver = PathResolver::new(&data, Path::new("/local"));
        assert_eq!(resolve(&resolver, "/local/c/pkg/mod.py"), None);
        assert_eq!(
            resolve(&resolver, "/local/a/pkg/mod.py"),
            Some("/ci/a/pkg/mod.py")
        );
    }

    #[test]
    fn normalization() {
        assert_eq!(normalize("/a/./b/../c"), "/a/c");
        assert_eq!(normalize("a\\b\\c.py"), "a/b/c.py");
        assert_eq!(normalize("../x.py"), "../x.py");
    }
}
