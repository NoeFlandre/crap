use std::path::{Path, PathBuf};
use std::process::Command;

use insta_cmd::get_cargo_bin;

/// A sample module: four functions with branches, one suppressed, one fully covered.
pub(crate) const PRICING: &str = r#""""Pricing rules."""


def total(items):
    return sum(item.price for item in items)


def discount(customer, amount, coupon=None):
    """Compute a discount."""
    if customer.is_vip and amount > 100:
        rate = 0.2
    elif customer.is_vip:
        rate = 0.1
    elif coupon and coupon.startswith("SAVE"):
        rate = 0.05
    else:
        rate = 0
    for tier in customer.tiers:
        if tier == "gold" or tier == "platinum":
            rate += 0.05
        elif tier == "silver":
            rate += 0.02
    if rate > 0.3:
        rate = 0.3
    return amount * rate


class Cart:
    def __init__(self):
        self.items = []

    def add(self, item, quantity=1):
        if quantity <= 0:
            raise ValueError("quantity must be positive")
        self.items.extend([item] * quantity)

    def shipping(self, country):  # crap: ignore
        if country == "FR":
            return 0
        elif country in ("BE", "DE", "ES", "IT"):
            return 5
        elif country == "US":
            return 15
        elif country == "JP":
            return 20
        return 25

    def checkout(self, payment):
        match payment.method:
            case "card" if payment.valid:
                return "ok"
            case "card":
                return "declined"
            case "paypal":
                return "redirect" if payment.token else "error"
            case "cash" | "check":
                return "pending"
            case _:
                return "unsupported"
"#;

/// The lines coverage.py records for `PRICING` when running a small test suite.
pub(crate) const PRICING_EXECUTED: &[u32] = &[
    1, 4, 5, 8, 10, 11, 18, 23, 25, 28, 29, 30, 32, 33, 35, 37, 48,
];

/// The executable statements of `PRICING`, as coverage.py reports them.
pub(crate) const PRICING_STATEMENTS: &[u32] = &[
    1, 4, 5, 8, 10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 22, 23, 24, 25, 28, 29, 30, 32, 33, 34,
    35, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59,
];

pub(crate) struct TestProject {
    dir: tempfile::TempDir,
}

impl TestProject {
    pub(crate) fn new() -> Self {
        let dir = tempfile::tempdir().expect("a temporary directory");
        // Resolve symlinks (`/tmp` on macOS) so paths printed by `crap` match.
        let project = Self { dir };
        assert!(project.root().is_dir());
        project
    }

    /// A project with `src/shop/pricing.py` and coverage.py data for it.
    pub(crate) fn shop() -> Self {
        let project = Self::new();
        project.write("pyproject.toml", "[project]\nname = \"shop\"\n");
        project.write("src/shop/__init__.py", "");
        project.write("src/shop/pricing.py", PRICING);
        project.coverage_db(
            ".coverage",
            &[
                ("src/shop/__init__.py", &[]),
                ("src/shop/pricing.py", PRICING_EXECUTED),
            ],
        );
        project
    }

    pub(crate) fn root(&self) -> PathBuf {
        dunce_canonicalize(self.dir.path())
    }

    pub(crate) fn write(&self, path: &str, contents: &str) {
        let path = self.root().join(path);
        std::fs::create_dir_all(path.parent().expect("a parent directory"))
            .expect("to create directories");
        std::fs::write(path, contents).expect("to write a file");
    }

    /// Writes a coverage.py data file recording `files` (paths relative to the project root,
    /// stored as absolute paths like coverage.py does) with their executed lines.
    pub(crate) fn coverage_db(&self, name: &str, files: &[(&str, &[u32])]) {
        let path = self.root().join(name);
        let _ = std::fs::remove_file(&path);
        let connection = rusqlite::Connection::open(&path).expect("to create a database");
        connection
            .execute_batch(
                "CREATE TABLE coverage_schema (version integer);
                 INSERT INTO coverage_schema VALUES (7);
                 CREATE TABLE meta (key text, value text, unique (key));
                 INSERT INTO meta VALUES ('version', '7.17.0'), ('has_arcs', '0');
                 CREATE TABLE file (id integer primary key, path text, unique (path));
                 CREATE TABLE context (id integer primary key, context text, unique (context));
                 INSERT INTO context VALUES (1, '');
                 CREATE TABLE line_bits (file_id integer, context_id integer, numbits blob,
                    unique (file_id, context_id));
                 CREATE TABLE arc (file_id integer, context_id integer, fromno integer,
                    tono integer, unique (file_id, context_id, fromno, tono));
                 CREATE TABLE tracer (file_id integer primary key, tracer text);",
            )
            .expect("to create the schema");
        for (id, (file, lines)) in files.iter().enumerate() {
            let absolute = self.root().join(file);
            connection
                .execute(
                    "INSERT INTO file VALUES (?1, ?2)",
                    (id as i64 + 1, absolute.to_string_lossy().as_ref()),
                )
                .expect("to insert a file");
            connection
                .execute(
                    "INSERT INTO line_bits VALUES (?1, 1, ?2)",
                    (id as i64 + 1, numbits(lines)),
                )
                .expect("to insert lines");
        }
    }

    pub(crate) fn command(&self, subcommand: &str) -> Command {
        let mut command = Command::new(get_cargo_bin("crap"));
        command
            .arg(subcommand)
            .current_dir(self.root())
            .env("NO_COLOR", "1")
            .env_remove("FORCE_COLOR")
            .env_remove("COVERAGE_FILE")
            .env_remove("COVERAGE_RCFILE")
            .env_remove("CRAP_LOG");
        command
    }

    /// Snapshot settings replacing the project's path with `[ROOT]`.
    pub(crate) fn settings(&self) -> insta::Settings {
        let mut settings = insta::Settings::clone_current();
        let root = self.root().to_string_lossy().into_owned();
        // Paths as printed, and as escaped in JSON on Windows.
        settings.add_filter(&regex_escape(&root.replace('\\', "\\\\")), "[ROOT]");
        settings.add_filter(&regex_escape(&root), "[ROOT]");
        settings.add_filter(&regex_escape(&root.replace('\\', "/")), "[ROOT]");
        if cfg!(windows) {
            settings.add_filter(r"\\\\?([\w\[\]])", "/$1");
            settings.add_filter(
                r"The system cannot find the (file|path) specified\.",
                "No such file or directory",
            );
        }
        settings.add_filter(r#""version": "\d+\.\d+\.\d+""#, r#""version": "[VERSION]""#);
        settings
    }
}

pub(crate) fn numbits(lines: &[u32]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for &line in lines {
        let index = (line / 8) as usize;
        if bytes.len() <= index {
            bytes.resize(index + 1, 0);
        }
        bytes[index] |= 1 << (line % 8);
    }
    bytes
}

fn regex_escape(value: &str) -> String {
    let mut escaped = String::new();
    for c in value.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(c) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

fn dunce_canonicalize(path: &Path) -> PathBuf {
    let canonical = path.canonicalize().expect("a canonical path");
    // Strip Windows' verbatim prefix, which most tools do not print.
    let text = canonical.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(stripped) => PathBuf::from(stripped),
        None => canonical,
    }
}

/// Runs a command and asserts on its output, with the project's paths filtered out.
#[macro_export]
macro_rules! assert_snapshot {
    ($project:expr, $command:expr) => {{
        $project.settings().bind(|| {
            insta_cmd::assert_cmd_snapshot!($command);
        });
    }};
}
