use crate::assert_snapshot;
use crate::common::TestProject;

#[test]
fn table() {
    let project = TestProject::shop();
    assert_snapshot!(project, project.command("report"));
}

#[test]
fn top_and_sort() {
    let project = TestProject::shop();
    assert_snapshot!(
        project,
        project
            .command("report")
            .args(["--top", "3", "--sort", "complexity"])
    );
}

#[test]
fn crappy_only() {
    let project = TestProject::shop();
    assert_snapshot!(project, project.command("report").arg("--crappy-only"));
}

#[test]
fn min_score_sorted_by_location() {
    let project = TestProject::shop();
    assert_snapshot!(
        project,
        project
            .command("report")
            .args(["--min-score", "2", "--sort", "location", "--quiet"])
    );
}

#[test]
fn markdown() {
    let project = TestProject::shop();
    assert_snapshot!(
        project,
        project
            .command("report")
            .args(["--output-format", "markdown"])
    );
}

#[test]
fn csv() {
    let project = TestProject::shop();
    assert_snapshot!(
        project,
        project
            .command("report")
            .args(["--output-format", "csv", "--sort", "name"])
    );
}

#[test]
fn json() {
    let project = TestProject::shop();
    assert_snapshot!(
        project,
        project
            .command("report")
            .args(["--output-format", "json", "--top", "1"])
    );
}

#[test]
fn nothing_to_report() {
    let project = TestProject::shop();
    assert_snapshot!(
        project,
        project.command("report").args(["--min-score", "1000"])
    );
}

#[test]
fn version() {
    let project = TestProject::new();
    let output = project.command("version").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout, format!("crap {}\n", env!("CARGO_PKG_VERSION")));

    let output = project
        .command("version")
        .args(["--output-format", "json"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
}
