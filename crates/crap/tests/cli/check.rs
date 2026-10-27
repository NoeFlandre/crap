use crate::assert_snapshot;
use crate::common::TestProject;

#[test]
fn full() {
    let project = TestProject::shop();
    assert_snapshot!(project, project.command("check"));
}

#[test]
fn concise() {
    let project = TestProject::shop();
    assert_snapshot!(
        project,
        project
            .command("check")
            .args(["--output-format", "concise"])
    );
}

#[test]
fn all_checks_passed() {
    let project = TestProject::shop();
    assert_snapshot!(
        project,
        project.command("check").args(["--threshold", "50"])
    );
}

#[test]
fn exit_zero() {
    let project = TestProject::shop();
    assert_snapshot!(
        project,
        project
            .command("check")
            .args(["--exit-zero", "--quiet", "--output-format", "concise"])
    );
}

#[test]
fn silent() {
    let project = TestProject::shop();
    assert_snapshot!(project, project.command("check").arg("--silent"));
}

#[test]
fn json() {
    let project = TestProject::shop();
    assert_snapshot!(
        project,
        project.command("check").args(["--output-format", "json"])
    );
}

#[test]
fn json_lines() {
    let project = TestProject::shop();
    assert_snapshot!(
        project,
        project
            .command("check")
            .args(["--output-format", "json-lines"])
    );
}

#[test]
fn github() {
    let project = TestProject::shop();
    assert_snapshot!(
        project,
        project.command("check").args(["--output-format", "github"])
    );
}

#[test]
fn gitlab() {
    let project = TestProject::shop();
    assert_snapshot!(
        project,
        project.command("check").args(["--output-format", "gitlab"])
    );
}

#[test]
fn sarif() {
    let project = TestProject::shop();
    assert_snapshot!(
        project,
        project.command("check").args(["--output-format", "sarif"])
    );
}

#[test]
fn junit() {
    let project = TestProject::shop();
    assert_snapshot!(
        project,
        project.command("check").args(["--output-format", "junit"])
    );
}

#[test]
fn output_file() {
    let project = TestProject::shop();
    assert_snapshot!(
        project,
        project.command("check").args([
            "--output-format",
            "concise",
            "--output-file",
            "reports/crap.txt"
        ])
    );
    let written = std::fs::read_to_string(project.root().join("reports/crap.txt")).unwrap();
    assert!(
        written.contains("`discount` has a CRAP score of 46.9"),
        "{written}"
    );
}

#[test]
fn explicit_paths() {
    let project = TestProject::shop();
    project.write("src/shop/other.py", "def untested(a):\n    return a or 1\n");
    assert_snapshot!(
        project,
        project
            .command("check")
            .args(["--output-format", "concise", "src/shop/other.py"])
    );
}

#[test]
fn unmeasured_files() {
    let project = TestProject::shop();
    project.write(
        "tests/test_pricing.py",
        "def test_all():\n    for x in range(3):\n        if x and x > 1 or x < 0:\n            assert x\n",
    );
    assert_snapshot!(
        project,
        project
            .command("check")
            .args(["--output-format", "concise", "--threshold", "15"])
    );
    assert_snapshot!(
        project,
        project.command("check").args([
            "--output-format",
            "concise",
            "--threshold",
            "15",
            "--unmeasured",
            "skip"
        ])
    );
}

#[test]
fn syntax_error() {
    let project = TestProject::shop();
    project.write("src/shop/broken.py", "def broken(:\n    pass\n");
    assert_snapshot!(
        project,
        project
            .command("check")
            .args(["--output-format", "concise"])
    );
}

#[test]
fn no_python_files() {
    let project = TestProject::new();
    project.write("README.md", "# Nothing here\n");
    assert_snapshot!(project, project.command("check"));
}

#[test]
fn no_coverage_data() {
    let project = TestProject::new();
    project.write("app.py", "def f():\n    pass\n");
    assert_snapshot!(project, project.command("check"));
}

#[test]
fn missing_path() {
    let project = TestProject::shop();
    assert_snapshot!(
        project,
        project
            .command("check")
            .args(["--output-format", "concise", "src", "nope"])
    );
}

#[test]
fn excluded_functions_and_suppressions() {
    let project = TestProject::new();
    project.write(
        "app.py",
        r"
def excluded(a):  # pragma: no cover
    if a and a > 1 or a < 0:
        return 1
    return 2 if a else 3


def suppressed(a):  # noqa: CRAP001
    if a and a > 1 or a < 0:
        return 1
    return 2 if a else 3


def reported(a):  # noqa: E501
    if a and a > 1 or a < 0:
        return 1
    return 2 if a else 3
",
    );
    project.coverage_db(".coverage", &[("app.py", &[2, 8, 14])]);
    assert_snapshot!(
        project,
        project
            .command("check")
            .args(["--output-format", "concise", "--threshold", "10"])
    );
}
