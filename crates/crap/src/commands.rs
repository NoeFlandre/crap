use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result, bail};

use crap_coverage::{CoverageData, CoveragePyConfig};
use crap_linter::{Analysis, AnalysisSettings, analyze_files};
use crap_python::{DEFAULT_EXCLUDE, ExclusionPatterns};
use crap_workspace::{Settings, Unmeasured, find_python_files};

use crate::ExitStatus;
use crate::args::{CheckArgs, ReportArgs, SharedArgs, VersionFormat};
use crate::logging::{LogLevel, level, warn_user};
use crate::printer::Printer;
use crate::report::Reporter;

/// The result of analyzing a project.
pub(crate) struct Run {
    pub(crate) settings: Settings,
    pub(crate) analysis: Analysis,
    /// The directory paths are displayed relative to.
    pub(crate) cwd: PathBuf,
}

impl Run {
    /// Formats `path` for display: relative to the current directory when inside it.
    pub(crate) fn display_path(&self, path: &Path) -> String {
        relativize(path, &self.cwd)
    }
}

pub(crate) fn relativize(path: &Path, cwd: &Path) -> String {
    path.strip_prefix(cwd)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

pub(crate) fn check(args: &CheckArgs) -> Result<ExitStatus> {
    let mut overrides = args.shared.overrides();
    overrides.output_format = args.output_format;
    let Some(run) = analyze(&args.shared, overrides)? else {
        return Ok(ExitStatus::Success);
    };

    let mut writer = output_writer(args.shared.output_file.as_deref())?;
    let crappy = Printer::new(&run).write(&mut writer, run.settings.output_format)?;
    writer.flush()?;

    if crappy > 0 && !args.exit_zero {
        Ok(ExitStatus::Failure)
    } else {
        Ok(ExitStatus::Success)
    }
}

pub(crate) fn report(args: &ReportArgs) -> Result<ExitStatus> {
    let Some(run) = analyze(&args.shared, args.shared.overrides())? else {
        return Ok(ExitStatus::Success);
    };

    let mut writer = output_writer(args.shared.output_file.as_deref())?;
    Reporter::new(&run, args).write(&mut writer)?;
    writer.flush()?;
    Ok(ExitStatus::Success)
}

#[expect(clippy::print_stdout)]
pub(crate) fn version(format: VersionFormat) -> ExitStatus {
    let version = env!("CARGO_PKG_VERSION");
    match format {
        VersionFormat::Text => println!("crap {version}"),
        VersionFormat::Json => println!(
            "{}",
            serde_json::json!({ "name": "crap", "version": version })
        ),
    }
    ExitStatus::Success
}

/// Resolves settings, finds files and coverage data, and analyzes every file. Returns `None`
/// when there is nothing to analyze.
fn analyze(args: &SharedArgs, overrides: crap_workspace::SettingsOverrides) -> Result<Option<Run>> {
    let cwd = std::env::current_dir().context("Failed to determine the current directory")?;
    let settings = Settings::resolve(&cwd, args.config.as_deref(), overrides)?;
    tracing::debug!("Project root: `{}`", settings.project_root.display());

    let paths: Vec<PathBuf> = if args.paths.is_empty() {
        vec![cwd.clone()]
    } else {
        args.paths
            .iter()
            .map(|path| std::path::absolute(cwd.join(path)))
            .collect::<io::Result<_>>()?
    };
    let files = find_python_files(&paths, &settings);
    for error in &files.errors {
        warn_user!("{error}");
    }
    tracing::debug!("Found {} Python files", files.files.len());
    if files.files.is_empty() {
        warn_user!("No Python files found under the given path(s)");
        return Ok(None);
    }

    let coverage_config = CoveragePyConfig::find(&settings.project_root)?;
    let coverage_files = if settings.coverage.is_empty() {
        crap_coverage::discover(&settings.project_root, &coverage_config)
    } else {
        settings.coverage.clone()
    };
    if coverage_files.is_empty() {
        bail!(
            "No coverage data found in `{}`.\n\n  Run your tests with coverage first, for example with `coverage run -m pytest` or\n  `pytest --cov`, or point to a coverage file with `--coverage <FILE>`.",
            settings.project_root.display()
        );
    }
    let coverage = CoverageData::read_all(coverage_files.iter().map(PathBuf::as_path))?;
    for (path, format) in &coverage.sources {
        tracing::debug!("Read {format} `{}`", path.display());
    }

    let exclude = exclusion_patterns(&coverage_config);
    let analysis_settings = AnalysisSettings {
        exclude,
        unmeasured: settings.unmeasured,
    };
    let analysis = analyze_files(
        &files.files,
        &coverage,
        &settings.project_root,
        &analysis_settings,
    );

    let run = Run {
        settings,
        analysis,
        cwd,
    };
    warn_about_analysis(&run, &coverage_files);
    Ok(Some(run))
}

/// The exclusion patterns from coverage.py's configuration, falling back to its default.
fn exclusion_patterns(config: &CoveragePyConfig) -> ExclusionPatterns {
    let mut patterns: Vec<String> = config
        .exclude_lines
        .clone()
        .unwrap_or_else(|| vec![DEFAULT_EXCLUDE.to_string()]);
    patterns.extend(config.exclude_also.iter().cloned());
    match ExclusionPatterns::new(&patterns) {
        Ok(patterns) => patterns,
        Err(err) => {
            let origin = config
                .path
                .as_ref()
                .map_or_else(String::new, |path| format!(" in `{}`", path.display()));
            warn_user!("Ignoring coverage.py exclusion patterns{origin}: {err}");
            ExclusionPatterns::default()
        }
    }
}

fn warn_about_analysis(run: &Run, coverage_files: &[PathBuf]) {
    let analysis = &run.analysis;
    for error in &analysis.errors {
        warn_user!(
            "Failed to read `{}`: {}",
            run.display_path(&error.path),
            error.message
        );
    }
    for file in &analysis.files {
        if let Some(error) = file.syntax_errors.first() {
            warn_user!(
                "Failed to parse `{}:{}:{}`: {}",
                run.display_path(&file.path),
                error.line,
                error.column,
                error.message
            );
        }
    }

    if level() < LogLevel::Default {
        return;
    }

    if run.settings.unmeasured == Unmeasured::Uncovered {
        let unmeasured: Vec<_> = analysis
            .files
            .iter()
            .filter(|file| !file.measured && !file.functions.is_empty())
            .collect();
        if !unmeasured.is_empty() {
            let examples: Vec<_> = unmeasured
                .iter()
                .take(3)
                .map(|file| format!("`{}`", run.display_path(&file.path)))
                .collect();
            let more = if unmeasured.len() > examples.len() {
                format!(" and {} more", unmeasured.len() - examples.len())
            } else {
                String::new()
            };
            let (count, possessive, pronoun) = if unmeasured.len() == 1 {
                ("1 file is".to_string(), "its", "it")
            } else {
                (format!("{} files are", unmeasured.len()), "their", "them")
            };
            warn_user!(
                "{count} missing from the coverage data, so {possessive} functions count as untested: {}{more}. Exclude {pronoun}, or pass `--unmeasured skip` to ignore {pronoun}.",
                examples.join(", ")
            );
        }
    }

    // Coverage recorded before a file changed no longer matches its lines.
    let recorded = coverage_files
        .iter()
        .filter_map(|path| modified(path))
        .max();
    if let Some(recorded) = recorded {
        let stale = analysis
            .files
            .iter()
            .filter(|file| {
                file.measured && modified(&file.path).is_some_and(|time| time > recorded)
            })
            .count();
        if stale > 0 {
            let files = if stale == 1 {
                "1 file was".to_string()
            } else {
                format!("{stale} files were")
            };
            warn_user!(
                "{files} modified after the coverage data was recorded; re-run your tests for accurate results"
            );
        }
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    path.metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
}

fn output_writer(path: Option<&Path>) -> Result<Box<dyn Write>> {
    match path {
        Some(path) => {
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                std::fs::create_dir_all(parent)?;
            }
            let file = File::create(path)
                .with_context(|| format!("Failed to create `{}`", path.display()))?;
            colored::control::set_override(false);
            Ok(Box::new(BufWriter::new(file)))
        }
        None if crate::logging::is_silent() => Ok(Box::new(io::sink())),
        None => Ok(Box::new(BufWriter::new(io::stdout().lock()))),
    }
}
