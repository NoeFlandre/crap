use std::path::Path;

use crap_coverage::{FileCoverage, LineSet};
use crap_python::AnalysisOptions;

use super::*;

#[test]
fn score() {
    assert_eq!(crap_score(1, 1.0), 1.0);
    assert_eq!(crap_score(1, 0.0), 2.0);
    assert_eq!(crap_score(5, 0.0), 30.0);
    assert_eq!(crap_score(6, 0.0), 42.0);
    assert_eq!(crap_score(10, 0.5), 22.5);
    assert_eq!(crap_score(30, 1.0), 30.0);
}

#[test]
fn required() {
    assert_eq!(required_coverage(5, 30.0), Some(0.0));
    assert_eq!(required_coverage(31, 30.0), None);
    assert_eq!(required_coverage(30, 30.0), Some(1.0));
    let needed = required_coverage(10, 30.0).unwrap();
    assert!((crap_score(10, needed) - 30.0).abs() < 1e-9);
    assert!((needed - 0.415_196).abs() < 1e-6, "{needed}");
}

#[test]
fn suppressions() {
    assert!(is_suppression("def f():  # crap: ignore"));
    assert!(is_suppression("def f():  # CRAP:ignore"));
    assert!(is_suppression("def f():  # noqa"));
    assert!(is_suppression("def f():  # noqa: CRAP001"));
    assert!(is_suppression("def f():  # noqa: E501, CRAP001"));
    assert!(is_suppression("def f():  # noqa:CRAP"));
    assert!(!is_suppression("def f():  # noqa: E501"));
    assert!(!is_suppression("def f():  # crappy"));
    assert!(!is_suppression("def f():"));
}

const SOURCE: &str = r#"
def simple(x):
    return x + 1


def branchy(a, b, c):
    """A docstring is not a statement."""
    if a:
        return 1
    elif b:
        return 2
    for item in c:
        if item and a or b:
            return item
    return 0


class Shape:
    def area(self):  # noqa: CRAP001
        if self.kind == "square":
            return self.side**2
        return 0

    def outer(self):
        def inner(y):
            return y if y else -y
        return inner


def excluded():  # pragma: no cover
    if True:
        pass
"#;

fn analyze_source(coverage: Option<&FileCoverage>) -> FileReport {
    analyze_file(
        Path::new("/project/mod.py"),
        SOURCE,
        coverage,
        &AnalysisOptions::default(),
    )
}

#[test]
fn unmeasured_file() {
    let report = analyze_source(None);
    assert!(!report.measured);
    let summary: Vec<_> = report
        .functions
        .iter()
        .map(|f| {
            (
                f.qualified_name.as_str(),
                f.complexity,
                f.statements,
                f.covered,
                f.score,
                f.suppressed,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("simple", 1, 1, 0, 2.0, false),
            ("branchy", 7, 8, 0, 56.0, false),
            ("Shape.area", 2, 3, 0, 6.0, true),
            ("Shape.outer", 1, 2, 0, 2.0, false),
            ("Shape.outer.inner", 2, 1, 0, 6.0, false),
        ]
    );
    assert_eq!(
        report.functions[1].missed_lines,
        [8, 9, 10, 11, 12, 13, 14, 15]
    );
}

#[test]
fn raw_coverage_data() {
    // Raw tracer lines: `def` lines run at import, plus some of `branchy`.
    let coverage = FileCoverage {
        executed: LineSet::from([2, 3, 6, 8, 10, 12, 15, 18, 19]),
        statements: None,
    };
    let report = analyze_source(Some(&coverage));
    let branchy = &report.functions[1];
    assert_eq!(branchy.statements, 8);
    assert_eq!(branchy.covered, 4);
    assert_eq!(branchy.missed_lines, [9, 11, 13, 14]);
    assert_eq!(branchy.coverage(), 0.5);
    assert_eq!(branchy.score, 49.0 * 0.125 + 7.0);
    assert!(branchy.is_crappy(13.0));
    assert!(!branchy.is_crappy(13.125));
    assert_eq!(report.functions[0].coverage(), 1.0);
}

#[test]
fn report_statements_win() {
    // A report that only counts two statements of `branchy`, both executed.
    let coverage = FileCoverage {
        executed: LineSet::from([8, 15]),
        statements: Some(LineSet::from([8, 15])),
    };
    let report = analyze_source(Some(&coverage));
    let branchy = &report.functions[1];
    assert_eq!((branchy.statements, branchy.covered), (2, 2));
    assert_eq!(branchy.score, 7.0);
    // Functions without statements in the report count as fully covered.
    assert_eq!(report.functions[0].statements, 0);
    assert_eq!(report.functions[0].score, 1.0);
}
