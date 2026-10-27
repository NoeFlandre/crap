use std::fmt::Write as _;

use crate::assert_snapshot;
use crate::common::{PRICING_EXECUTED, PRICING_STATEMENTS, TestProject};

/// `crap report` as CSV, for comparing coverage formats.
fn report(project: &TestProject, coverage: &str) -> String {
    let output = project
        .command("report")
        .args([
            "--output-format",
            "csv",
            "--sort",
            "location",
            "--coverage",
            coverage,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn missing() -> Vec<u32> {
    PRICING_STATEMENTS
        .iter()
        .filter(|line| !PRICING_EXECUTED.contains(line))
        .copied()
        .collect()
}

/// Every coverage format yields the same report as coverage.py's data file.
#[test]
fn all_formats_agree() {
    let project = TestProject::shop();
    let expected = report(&project, ".coverage");

    let json = serde_json::json!({
        "meta": { "format": 3, "version": "7.17.0" },
        "files": {
            "src/shop/__init__.py": { "executed_lines": [], "missing_lines": [], "excluded_lines": [] },
            "src/shop/pricing.py": {
                "executed_lines": PRICING_EXECUTED,
                "missing_lines": missing(),
                "excluded_lines": [],
            },
        },
    });
    project.write("coverage.json", &json.to_string());
    assert_eq!(report(&project, "coverage.json"), expected);

    let mut lines = String::new();
    for line in PRICING_STATEMENTS {
        let hits = u32::from(PRICING_EXECUTED.contains(line));
        let _ = writeln!(lines, r#"<line number="{line}" hits="{hits}"/>"#);
    }
    let root = project.root();
    let xml = format!(
        r#"<?xml version="1.0" ?>
<coverage version="7.17.0">
<sources><source>{}</source></sources>
<packages><package name="shop"><classes>
<class name="pricing.py" filename="shop/pricing.py"><methods/><lines>
{lines}</lines></class>
</classes></package></packages>
</coverage>"#,
        root.join("src").display()
    );
    project.write("coverage.xml", &xml);
    assert_eq!(report(&project, "coverage.xml"), expected);

    let mut lcov = String::from("TN:\nSF:src/shop/pricing.py\n");
    for line in PRICING_STATEMENTS {
        let hits = u32::from(PRICING_EXECUTED.contains(line));
        let _ = writeln!(lcov, "DA:{line},{hits}");
    }
    lcov.push_str("end_of_record\n");
    project.write("coverage.lcov", &lcov);
    assert_eq!(report(&project, "coverage.lcov"), expected);
}

#[test]
fn newest_coverage_file_is_discovered() {
    let project = TestProject::shop();
    // A report written after the data file, where everything ran.
    let json = serde_json::json!({
        "files": {
            "src/shop/pricing.py": {
                "executed_lines": PRICING_STATEMENTS,
                "missing_lines": [],
            },
        },
    });
    std::thread::sleep(std::time::Duration::from_millis(50));
    project.write("coverage.json", &json.to_string());
    assert_snapshot!(project, project.command("check").arg("--verbose"));
}

#[test]
fn data_recorded_on_another_machine() {
    let project = TestProject::shop();
    let json = serde_json::json!({
        "files": {
            "/home/runner/work/shop/shop/src/shop/pricing.py": {
                "executed_lines": PRICING_EXECUTED,
                "missing_lines": missing(),
            },
        },
    });
    project.write("ci/coverage.json", &json.to_string());
    assert_snapshot!(
        project,
        project.command("check").args([
            "--output-format",
            "concise",
            "--coverage",
            "ci/coverage.json"
        ])
    );
}

#[test]
fn parallel_data_files_are_merged() {
    let project = TestProject::shop();
    std::fs::remove_file(project.root().join(".coverage")).unwrap();
    let (first, second) = PRICING_EXECUTED.split_at(8);
    project.coverage_db(".coverage.host.1.abc", &[("src/shop/pricing.py", first)]);
    project.coverage_db(".coverage.host.2.def", &[("src/shop/pricing.py", second)]);
    let merged = report(&project, ".coverage.host.1.abc");
    assert_ne!(merged, "");
    assert_snapshot!(
        project,
        project
            .command("check")
            .args(["--output-format", "concise"])
    );
}

#[test]
fn branch_coverage_data() {
    let project = TestProject::new();
    project.write(
        "app.py",
        "def f(a):\n    if a:\n        return 1\n    return 2\n",
    );
    let path = project.root().join(".coverage");
    project.coverage_db(".coverage", &[]);
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch(&format!(
            "INSERT INTO file VALUES (1, '{}');
             INSERT INTO arc VALUES (1, 1, -1, 1), (1, 1, 1, -1), (1, 1, -1, 2), (1, 1, 2, 4), (1, 1, 4, -1);",
            project.root().join("app.py").display()
        ))
        .unwrap();
    assert_snapshot!(
        project,
        project.command("report").args(["--output-format", "csv"])
    );
}

#[test]
fn coverage_environment_variable() {
    let project = TestProject::shop();
    std::fs::rename(
        project.root().join(".coverage"),
        project.root().join(".coverage.custom"),
    )
    .unwrap();
    let mut command = project.command("check");
    command
        .env("COVERAGE_FILE", ".coverage.custom")
        .args(["--output-format", "concise"]);
    assert_snapshot!(project, command);
}

#[test]
fn unreadable_coverage_file() {
    let project = TestProject::shop();
    project.write("coverage.txt", "not coverage data\n");
    assert_snapshot!(
        project,
        project
            .command("check")
            .args(["--coverage", "coverage.txt"])
    );
    assert_snapshot!(
        project,
        project.command("check").args(["--coverage", "missing.xml"])
    );
}

#[test]
fn malformed_report() {
    let project = TestProject::shop();
    project.write("coverage.json", "{\"files\": 3}");
    assert_snapshot!(
        project,
        project
            .command("check")
            .args(["--coverage", "coverage.json"])
    );
}
