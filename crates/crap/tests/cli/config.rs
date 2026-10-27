use crate::assert_snapshot;
use crate::common::TestProject;

#[test]
fn pyproject_settings() {
    let project = TestProject::shop();
    project.write(
        "pyproject.toml",
        "[project]\nname = \"shop\"\n\n[tool.crap]\nthreshold = 40\noutput-format = \"concise\"\n",
    );
    assert_snapshot!(project, project.command("check"));
}

#[test]
fn command_line_overrides_configuration() {
    let project = TestProject::shop();
    project.write("crap.toml", "threshold = 40\noutput-format = \"concise\"\n");
    assert_snapshot!(
        project,
        project.command("check").args(["--threshold", "45"])
    );
}

#[test]
fn run_from_a_subdirectory() {
    let project = TestProject::shop();
    project.write(
        "pyproject.toml",
        "[tool.crap]\noutput-format = \"concise\"\n",
    );
    let mut command = project.command("check");
    command.current_dir(project.root().join("src/shop"));
    assert_snapshot!(project, command);
}

#[test]
fn exclude() {
    let project = TestProject::shop();
    project.write("crap.toml", "extend-exclude = [\"pricing.py\"]\n");
    assert_snapshot!(project, project.command("check"));
}

#[test]
fn explicit_config_file() {
    let project = TestProject::shop();
    project.write(
        "configs/strict.toml",
        "threshold = 5\noutput-format = \"concise\"\n",
    );
    assert_snapshot!(
        project,
        project
            .command("check")
            .args(["--config", "configs/strict.toml"])
    );
}

#[test]
fn unknown_option() {
    let project = TestProject::shop();
    project.write("pyproject.toml", "[tool.crap]\nthreshhold = 40\n");
    assert_snapshot!(project, project.command("check"));
}

#[test]
fn coverage_exclusions_are_honored() {
    let project = TestProject::shop();
    project.write(
        "pyproject.toml",
        "[tool.coverage.report]\nexclude_also = [\"def checkout\"]\n",
    );
    assert_snapshot!(
        project,
        project
            .command("check")
            .args(["--output-format", "concise"])
    );
}

#[test]
fn invalid_coverage_exclusion() {
    let project = TestProject::shop();
    project.write(
        "pyproject.toml",
        "[tool.coverage.report]\nexclude_also = [\"(unclosed\"]\n",
    );
    assert_snapshot!(
        project,
        project
            .command("check")
            .args(["--output-format", "concise"])
    );
}
