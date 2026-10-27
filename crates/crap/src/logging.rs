use std::io::IsTerminal;
use std::sync::OnceLock;

use crate::args::{ColorChoice, GlobalArgs};

/// How much `crap` prints besides its main output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LogLevel {
    /// Print nothing.
    Silent,
    /// Print the main output only.
    Quiet,
    /// Print the main output, warnings and summaries.
    Default,
    /// Print debugging information too.
    Verbose,
}

static LEVEL: OnceLock<LogLevel> = OnceLock::new();

pub(crate) fn init(args: &GlobalArgs) {
    let level = if args.silent {
        LogLevel::Silent
    } else if args.quiet {
        LogLevel::Quiet
    } else if args.verbose {
        LogLevel::Verbose
    } else {
        LogLevel::Default
    };
    let _ = LEVEL.set(level);

    let colorize = match args.color {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => {
            if std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()) {
                false
            } else if std::env::var_os("FORCE_COLOR").is_some_and(|value| !value.is_empty()) {
                true
            } else {
                std::io::stdout().is_terminal()
            }
        }
    };
    colored::control::set_override(colorize);

    if level == LogLevel::Verbose {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::new(
                "crap=debug,crap_coverage=debug,crap_workspace=debug,crap_linter=debug",
            ))
            .with_writer(std::io::stderr)
            .with_target(false)
            .without_time()
            .init();
    } else if let Ok(filter) = std::env::var("CRAP_LOG") {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
            .with_writer(std::io::stderr)
            .init();
    }
}

pub(crate) fn level() -> LogLevel {
    LEVEL.get().copied().unwrap_or(LogLevel::Default)
}

pub(crate) fn is_silent() -> bool {
    level() == LogLevel::Silent
}

/// Prints a warning to stderr, unless running with `--quiet` or `--silent`.
macro_rules! warn_user {
    ($($arg:tt)*) => {{
        use colored::Colorize;
        if $crate::logging::level() >= $crate::logging::LogLevel::Default {
            let message = format!($($arg)*);
            #[allow(clippy::print_stderr)]
            {
                eprintln!("{}{} {}", "warning".yellow().bold(), ":".bold(), message.bold());
            }
        }
    }};
}

pub(crate) use warn_user;
