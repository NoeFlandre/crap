use std::path::PathBuf;

use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Args, Parser, Subcommand, ValueEnum};

use crap_workspace::{OutputFormat, SettingsOverrides, Unmeasured};

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Cyan.on_default());

/// An extremely fast CRAP score checker for Python.
///
/// CRAP (Change Risk Anti-Patterns) combines a function's cyclomatic complexity with its test
/// coverage: complex code that is poorly tested is risky to change. `crap` reads the coverage
/// data your test suite already produces (coverage.py, pytest-cov, Cobertura XML, LCOV).
#[derive(Debug, Parser)]
#[command(
    name = "crap",
    author,
    version,
    about,
    long_about = None,
    styles = STYLES,
    arg_required_else_help = true,
    after_help = "For help with a specific command, see: `crap help <command>`."
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,

    #[command(flatten)]
    pub(crate) global: GlobalArgs,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Report functions whose CRAP score exceeds the threshold.
    Check(CheckArgs),
    /// Show the CRAP score of every function, worst first.
    Report(ReportArgs),
    /// Display crap's version.
    Version {
        #[arg(long, value_enum, default_value_t = VersionFormat::Text)]
        output_format: VersionFormat,
    },
}

#[derive(Debug, Args)]
pub(crate) struct GlobalArgs {
    /// Enable verbose logging.
    #[arg(short, long, global = true, conflicts_with_all = ["quiet", "silent"], help_heading = "Global options")]
    pub(crate) verbose: bool,
    /// Print diagnostics, but nothing else.
    #[arg(
        short,
        long,
        global = true,
        conflicts_with = "silent",
        help_heading = "Global options"
    )]
    pub(crate) quiet: bool,
    /// Disable all output (the exit status still reports the result).
    #[arg(short, long, global = true, help_heading = "Global options")]
    pub(crate) silent: bool,
    /// Control when colored output is used.
    #[arg(
        long,
        value_enum,
        global = true,
        value_name = "WHEN",
        default_value_t = ColorChoice::Auto,
        help_heading = "Global options"
    )]
    pub(crate) color: ColorChoice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum ColorChoice {
    /// Use colors when writing to a terminal, unless `NO_COLOR` is set.
    Auto,
    /// Always use colors.
    Always,
    /// Never use colors.
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum VersionFormat {
    Text,
    Json,
}

/// Arguments shared by `check` and `report`.
#[derive(Debug, Args)]
pub(crate) struct SharedArgs {
    /// Files or directories to analyze [default: .]
    pub(crate) paths: Vec<PathBuf>,

    /// Coverage file to read: a coverage.py data file (`.coverage`), or a JSON, Cobertura XML
    /// or LCOV report. May be repeated to merge several files. Discovered automatically when
    /// omitted.
    #[arg(long, value_name = "FILE", help_heading = "Coverage")]
    pub(crate) coverage: Vec<PathBuf>,

    /// How to treat files missing from the coverage data: count their functions as `uncovered`
    /// [default], or `skip` them.
    #[arg(long, value_name = "MODE", value_parser = parse_unmeasured, help_heading = "Coverage")]
    pub(crate) unmeasured: Option<Unmeasured>,

    /// The CRAP score above which a function is considered crappy [default: 30]
    #[arg(long, value_name = "SCORE", help_heading = "Scoring")]
    pub(crate) threshold: Option<f64>,

    /// Path to a `pyproject.toml`, `crap.toml` or `.crap.toml` file to use for configuration.
    #[arg(long, value_name = "FILE", help_heading = "Configuration")]
    pub(crate) config: Option<PathBuf>,

    /// Glob patterns of files and directories to skip, replacing the defaults.
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "PATTERN",
        help_heading = "File selection"
    )]
    pub(crate) exclude: Option<Vec<String>>,

    /// Glob patterns of files and directories to skip, in addition to the defaults.
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "PATTERN",
        help_heading = "File selection"
    )]
    pub(crate) extend_exclude: Vec<String>,

    /// Respect `.gitignore` and similar files when finding files.
    #[arg(
        long,
        overrides_with = "no_respect_gitignore",
        help_heading = "File selection"
    )]
    respect_gitignore: bool,
    #[arg(long, overrides_with = "respect_gitignore", hide = true)]
    no_respect_gitignore: bool,

    /// Write the output to a file instead of stdout.
    #[arg(short, long, value_name = "FILE")]
    pub(crate) output_file: Option<PathBuf>,
}

impl SharedArgs {
    pub(crate) fn overrides(&self) -> SettingsOverrides {
        SettingsOverrides {
            threshold: self.threshold,
            coverage: (!self.coverage.is_empty()).then(|| self.coverage.clone()),
            exclude: self.exclude.clone(),
            extend_exclude: self.extend_exclude.clone(),
            respect_gitignore: match (self.respect_gitignore, self.no_respect_gitignore) {
                (true, _) => Some(true),
                (_, true) => Some(false),
                _ => None,
            },
            unmeasured: self.unmeasured,
            output_format: None,
        }
    }
}

#[derive(Debug, Args)]
pub(crate) struct CheckArgs {
    #[command(flatten)]
    pub(crate) shared: SharedArgs,

    /// Output format [default: full] [possible values: full, concise, json, json-lines, github, gitlab, sarif, junit]
    #[arg(long, value_name = "FORMAT", value_parser = parse_output_format, hide_possible_values = true)]
    pub(crate) output_format: Option<OutputFormat>,

    /// Exit with status 0, even when crappy functions are found.
    #[arg(short, long)]
    pub(crate) exit_zero: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ReportArgs {
    #[command(flatten)]
    pub(crate) shared: SharedArgs,

    /// Output format.
    #[arg(long, value_enum, value_name = "FORMAT", default_value_t = ReportFormat::Table)]
    pub(crate) output_format: ReportFormat,

    /// Sort order.
    #[arg(long, value_enum, default_value_t = SortKey::Score)]
    pub(crate) sort: SortKey,

    /// Show only the first N functions.
    #[arg(long, value_name = "N")]
    pub(crate) top: Option<usize>,

    /// Show only functions scoring at least SCORE.
    #[arg(long, value_name = "SCORE")]
    pub(crate) min_score: Option<f64>,

    /// Show only functions scoring above the threshold.
    #[arg(long, conflicts_with = "min_score")]
    pub(crate) crappy_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum ReportFormat {
    /// An aligned table.
    Table,
    /// A Markdown table, e.g. for pull request comments.
    Markdown,
    /// Comma-separated values.
    Csv,
    /// A JSON array.
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum SortKey {
    /// Highest CRAP score first.
    Score,
    /// Highest complexity first.
    Complexity,
    /// Lowest coverage first.
    Coverage,
    /// By file, then line.
    Location,
    /// By qualified function name.
    Name,
}

fn parse_output_format(value: &str) -> Result<OutputFormat, String> {
    value.parse()
}

fn parse_unmeasured(value: &str) -> Result<Unmeasured, String> {
    value.parse()
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::Cli;

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert();
    }
}
