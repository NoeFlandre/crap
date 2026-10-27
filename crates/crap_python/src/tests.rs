use crate::{AnalysisOptions, ExclusionPatterns, FunctionKind, LineSet, analyze};

fn complexities(source: &str) -> Vec<(String, u32)> {
    analyze(source, &AnalysisOptions::default())
        .functions
        .into_iter()
        .map(|function| (function.qualified_name, function.complexity))
        .collect()
}

fn complexity(body: &str) -> u32 {
    let source = format!("def f():\n{}", indent(body));
    let functions = complexities(&source);
    assert_eq!(functions.len(), 1, "{functions:?}");
    functions[0].1
}

fn indent(body: &str) -> String {
    body.lines().fold(String::new(), |mut indented, line| {
        indented.push_str("    ");
        indented.push_str(line);
        indented.push('\n');
        indented
    })
}

fn statements(source: &str) -> Vec<u32> {
    analyze(source, &AnalysisOptions::default())
        .statements
        .into_iter()
        .collect()
}

#[test]
fn complexity_of_simple_constructs() {
    assert_eq!(complexity("return 1"), 1);
    assert_eq!(complexity("if a:\n    pass"), 2);
    assert_eq!(
        complexity("if a:\n    pass\nelif b:\n    pass\nelse:\n    pass"),
        3
    );
    assert_eq!(complexity("return a if b else c"), 2);
    assert_eq!(complexity("for x in y:\n    pass"), 2);
    assert_eq!(complexity("for x in y:\n    pass\nelse:\n    pass"), 3);
    assert_eq!(complexity("while x:\n    pass"), 2);
    assert_eq!(complexity("async for x in y:\n    pass"), 2);
    assert_eq!(complexity("with a:\n    pass"), 1);
}

#[test]
fn complexity_of_exceptions() {
    assert_eq!(complexity("try:\n    a()\nfinally:\n    b()"), 1);
    assert_eq!(
        complexity("try:\n    a()\nexcept A:\n    pass\nexcept B:\n    pass"),
        3
    );
    assert_eq!(
        complexity("try:\n    a()\nexcept A:\n    pass\nelse:\n    pass"),
        3
    );
    assert_eq!(complexity("try:\n    a()\nexcept* A:\n    pass"), 2);
}

#[test]
fn complexity_of_boolean_operators() {
    assert_eq!(complexity("return a and b"), 2);
    assert_eq!(complexity("return a and b and c"), 3);
    assert_eq!(complexity("return a and b or c"), 3);
    assert_eq!(complexity("if a or b:\n    pass"), 3);
}

#[test]
fn complexity_of_comprehensions() {
    assert_eq!(complexity("return [x for x in y]"), 2);
    assert_eq!(complexity("return [x for x in y if x if not x]"), 4);
    assert_eq!(complexity("return {x: z for x in y for z in x}"), 3);
    assert_eq!(complexity("return any(x for x in y)"), 2);
}

#[test]
fn complexity_of_match() {
    let source =
        "match x:\n    case 1:\n        pass\n    case 2:\n        pass\n    case _:\n        pass";
    assert_eq!(complexity(source), 3);
    let source = "match x:\n    case 1:\n        pass\n    case other:\n        pass";
    assert_eq!(complexity(source), 2);
    let source = "match x:\n    case 1:\n        pass\n    case _ if y:\n        pass";
    assert_eq!(complexity(source), 3);
}

#[test]
fn complexity_of_assertions_and_lambdas() {
    // Like radon, the condition of an assertion is not inspected.
    assert_eq!(complexity("assert a and b"), 2);
    assert_eq!(complexity("return lambda x: x if x else 0"), 2);
}

#[test]
fn nested_definitions_are_scored_separately() {
    let source = r"
def outer(a):
    if a:
        pass

    def inner(b):
        return b or a

    class Local:
        def method(self):
            for x in self:
                if x:
                    return x

    return inner
";
    assert_eq!(
        complexities(source),
        [
            ("outer".to_string(), 2),
            ("outer.inner".to_string(), 2),
            ("outer.Local.method".to_string(), 3),
        ]
    );
}

#[test]
fn functions() {
    let source = r#"
import functools


class Parser:
    @functools.cache
    @staticmethod
    def parse(
        text: str,
    ) -> int:
        """Parse."""
        return len(text)

    async def fetch(self):
        def helper():
            return 1
        return helper()


def free():  # pragma: no cover
    pass
"#;
    let analysis = analyze(source, &AnalysisOptions::default());
    let summary: Vec<_> = analysis
        .functions
        .iter()
        .map(|f| {
            (
                f.qualified_name.as_str(),
                f.kind,
                f.is_async,
                (f.line, f.column),
                (f.start_line, f.signature_end_line, f.end_line),
                f.lines.iter().copied().collect::<Vec<_>>(),
                f.excluded,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (
                "Parser.parse",
                FunctionKind::Method,
                false,
                (8, 9),
                (6, 10, 12),
                vec![11, 12],
                false
            ),
            (
                "Parser.fetch",
                FunctionKind::Method,
                true,
                (14, 15),
                (14, 14, 17),
                vec![15, 17],
                false
            ),
            (
                "Parser.fetch.helper",
                FunctionKind::Function,
                false,
                (15, 13),
                (15, 15, 16),
                vec![16],
                false
            ),
            (
                "free",
                FunctionKind::Function,
                false,
                (20, 5),
                (20, 20, 21),
                vec![21],
                true
            ),
        ]
    );
}

#[test]
fn statements_skip_docstrings_and_declarations() {
    let source = r#""""Module docstring."""
import os


def f(a):
    """Docstring."""
    global G
    x: int
    y: int = 1
    self_attribute: int
    return (
        a
    )
"#;
    assert_eq!(statements(source), [2, 5, 9, 11]);
}

#[test]
fn unreachable_statements() {
    let source = r"def f(a):
    if a:
        return 1
        dead = 1
    else:
        raise ValueError
    also_dead = 1


def g():
    if False:
        never = 1
    while True:
        if g:
            break
    reachable = 1
    try:
        return 1
    finally:
        cleanup = 1
    after = 1


def h():
    with ctx:
        return 1
    maybe = 1
";
    assert_eq!(
        statements(source),
        [
            1, 2, 3, 6, 10, 11, 13, 14, 15, 16, 17, 18, 20, 24, 25, 26, 27
        ]
    );
}

#[test]
fn multiline_statements_map_to_their_first_line() {
    let source = "def f():\n    x = [\n        1,\n        2,\n    ]\n    y = '''\n    text\n    '''\n    return x\n";
    let analysis = analyze(source, &AnalysisOptions::default());
    assert_eq!(analysis.statements, LineSet::from([1, 2, 6, 9]));
    assert_eq!(analysis.first_line(4), 2);
    assert_eq!(analysis.first_line(7), 6);
    assert_eq!(analysis.first_line(9), 9);
    assert_eq!(
        analysis.translate_lines([1, 3, 4, 8, 9]),
        LineSet::from([1, 2, 6, 9])
    );
}

#[test]
fn exclusions() {
    let source = r"def f(a):
    if a:  # pragma: no cover
        x = 1
        y = 2
    z = 3
    if a:
        pass
    else:  # pragma: no cover
        w = 1
    return z  # pragma: no cover
";
    let analysis = analyze(source, &AnalysisOptions::default());
    assert_eq!(analysis.excluded, LineSet::from([2, 3, 4, 8, 9, 10]));
    assert_eq!(analysis.statements, LineSet::from([1, 5, 6, 7]));
}

#[test]
fn custom_exclusion_patterns() {
    let source = "def f():\n    raise NotImplementedError\n\ndef g():\n    if TYPE_CHECKING:\n        import x\n    return 1\n";
    let options = AnalysisOptions {
        exclude: ExclusionPatterns::new(["raise NotImplementedError", "if TYPE_CHECKING:"])
            .unwrap(),
    };
    let analysis = analyze(source, &options);
    assert_eq!(analysis.statements, LineSet::from([1, 4, 7]));

    let options = AnalysisOptions {
        exclude: ExclusionPatterns::none(),
    };
    assert_eq!(
        analyze(source, &options).statements,
        LineSet::from([1, 2, 4, 5, 6, 7])
    );
}

#[test]
fn invalid_exclusion_pattern() {
    let error = ExclusionPatterns::new(["(unclosed"]).unwrap_err();
    assert_eq!(error.pattern, "(unclosed");
}

#[test]
fn syntax_errors() {
    let analysis = analyze("def f(:\n    pass\n", &AnalysisOptions::default());
    assert!(!analysis.syntax_errors.is_empty());
    assert_eq!(analysis.syntax_errors[0].line, 1);
}

#[test]
fn modern_syntax() {
    let source = r#"
type Alias[T] = list[T]


def generic[T](items: list[T]) -> T:
    match items:
        case [first, *_] if first:
            return first
        case _:
            raise ValueError(f"{items!r:>{10}}")


async def tasks():
    try:
        async with group() as g:
            g.create_task(x)
    except* ValueError:
        pass
"#;
    let analysis = analyze(source, &AnalysisOptions::default());
    assert!(analysis.syntax_errors.is_empty());
    assert_eq!(
        complexities(source),
        [("generic".to_string(), 2), ("tasks".to_string(), 2)]
    );
}
