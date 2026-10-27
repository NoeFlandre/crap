//! Configuration and file discovery for `crap`.
//!
//! Settings are read from the `[tool.crap]` table of a `pyproject.toml`, or from the top level
//! of a `crap.toml` or `.crap.toml`, found by walking up from the current directory.

pub use crate::files::{FilePatternSet, PythonFiles, find_python_files};
pub use crate::options::{Options, OutputFormat, Unmeasured};
pub use crate::settings::{
    ConfigError, DEFAULT_EXCLUDE, DEFAULT_THRESHOLD, Settings, SettingsOverrides, find_config,
};

mod files;
mod options;
mod settings;
