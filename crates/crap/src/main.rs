use std::process::ExitCode;

use clap::Parser;
use colored::Colorize;

use crate::args::{Cli, Command};

mod args;
mod commands;
mod logging;
mod printer;
mod report;

/// The exit status of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExitStatus {
    /// No crappy functions were found.
    Success,
    /// Crappy functions were found.
    Failure,
    /// `crap` could not complete the analysis.
    Error,
}

impl From<ExitStatus> for ExitCode {
    fn from(status: ExitStatus) -> Self {
        match status {
            ExitStatus::Success => ExitCode::from(0),
            ExitStatus::Failure => ExitCode::from(1),
            ExitStatus::Error => ExitCode::from(2),
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    logging::init(&cli.global);

    let result = match cli.command {
        Command::Check(args) => commands::check(&args),
        Command::Report(args) => commands::report(&args),
        Command::Version { output_format } => Ok(commands::version(output_format)),
    };

    match result {
        Ok(status) => status.into(),
        Err(err) => {
            #[expect(clippy::print_stderr)]
            {
                if !logging::is_silent() {
                    eprintln!("{}{} {err}", "error".red().bold(), ":".bold());
                    for cause in err.chain().skip(1) {
                        eprintln!("  {} {cause}", "Cause:".bold());
                    }
                }
            }
            ExitStatus::Error.into()
        }
    }
}
