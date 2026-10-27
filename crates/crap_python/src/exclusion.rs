use regex::{Regex, RegexBuilder};
use ruff_python_ast::token::TokenKind;
use ruff_python_ast::{MatchCase, Pattern, Stmt};
use ruff_text_size::{Ranged, TextSize};

use crate::lines::{MultilineMap, is_significant};
use crate::statements::{child_bodies, def_keyword_line, python_lineno};
use crate::{LineSet, Locator};

/// coverage.py's default exclusion pattern, matching `# pragma: no cover`.
pub const DEFAULT_EXCLUDE: &str = r"#\s*(pragma|PRAGMA)[:\s]?\s*(no|NO)\s*(cover|COVER)";

/// A compiled set of exclusion patterns.
#[derive(Debug, Clone)]
pub struct ExclusionPatterns {
    regex: Option<Regex>,
}

/// An exclusion pattern that could not be compiled.
#[derive(Debug, Clone, thiserror::Error)]
#[error("invalid exclusion pattern `{pattern}`: {message}")]
pub struct InvalidPattern {
    pub pattern: String,
    pub message: String,
}

impl Default for ExclusionPatterns {
    fn default() -> Self {
        Self::new([DEFAULT_EXCLUDE]).expect("the default exclusion pattern to be valid")
    }
}

impl ExclusionPatterns {
    /// Compiles `patterns` (regular expressions in coverage.py's syntax) into a single matcher.
    pub fn new<I, S>(patterns: I) -> Result<Self, InvalidPattern>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut alternatives = Vec::new();
        for pattern in patterns {
            let pattern = pattern.as_ref();
            // Validate each pattern separately for a precise error message.
            Regex::new(pattern).map_err(|err| InvalidPattern {
                pattern: pattern.to_string(),
                message: err.to_string(),
            })?;
            alternatives.push(format!("(?:{pattern})"));
        }
        if alternatives.is_empty() {
            return Ok(Self::none());
        }
        let joined = alternatives.join("|");
        let regex = RegexBuilder::new(&joined)
            .multi_line(true)
            .build()
            .map_err(|err| InvalidPattern {
                pattern: joined.clone(),
                message: err.to_string(),
            })?;
        Ok(Self { regex: Some(regex) })
    }

    /// A matcher that excludes nothing.
    pub fn none() -> Self {
        Self { regex: None }
    }
}

/// Computes the lines excluded from measurement, following coverage.py's rules:
///
/// * a line matching a pattern excludes its whole (logical) line;
/// * an excluded line in the header of a compound statement excludes the whole block;
/// * an excluded decorator or `def`/`class` line excludes the whole definition;
/// * an irrefutable `case` whose body is fully excluded is excluded too.
pub(crate) fn excluded_lines(
    body: &[Stmt],
    locator: &Locator<'_>,
    multiline: &MultilineMap,
    patterns: &ExclusionPatterns,
    raw_statements: &LineSet,
) -> LineSet {
    let Some(regex) = &patterns.regex else {
        return LineSet::new();
    };

    let mut excluded = LineSet::new();
    for found in regex.find_iter(locator.source) {
        let start = locator.line(TextSize::try_from(found.start()).unwrap_or_default());
        let end = locator.line(TextSize::try_from(found.end()).unwrap_or_default());
        excluded.extend((start..=end).map(|line| multiline.first_line(line)));
    }
    if excluded.is_empty() {
        return excluded;
    }

    exclude_suites(locator, &mut excluded);

    let mut excluded: LineSet = excluded
        .into_iter()
        .map(|line| multiline.first_line(line))
        .collect();

    exclude_definitions(body, locator, raw_statements, &mut excluded);
    excluded
}

/// Extends exclusions on block-opening lines to the blocks they open, by tracking indentation
/// in the token stream exactly like coverage.py's `PythonParser._raw_parse`.
fn exclude_suites(locator: &Locator<'_>, excluded: &mut LineSet) {
    let mut indent: i32 = 0;
    let mut exclude_indent: i32 = 0;
    let mut excluding = false;
    let mut first_line = 0;
    let mut nesting: i32 = 0;

    for token in &**locator.tokens {
        let kind = token.kind();
        let end_line = locator.end_line(token);
        match kind {
            TokenKind::Indent => indent += 1,
            TokenKind::Dedent => indent -= 1,
            TokenKind::Colon if nesting == 0 => {
                if !excluding && excluded.range(first_line..=end_line).next().is_some() {
                    excluded.insert(end_line);
                    exclude_indent = indent;
                    excluding = true;
                }
            }
            TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace => nesting += 1,
            TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace => nesting -= 1,
            TokenKind::Newline => first_line = 0,
            _ => {}
        }
        if is_significant(kind) && first_line == 0 {
            first_line = locator.line(token.start());
            if excluding && indent <= exclude_indent {
                excluding = false;
            }
            if excluding {
                excluded.insert(end_line);
            }
        }
    }
}

fn exclude_definitions(
    body: &[Stmt],
    locator: &Locator<'_>,
    raw_statements: &LineSet,
    excluded: &mut LineSet,
) {
    for stmt in body {
        let decorators = match stmt {
            Stmt::FunctionDef(function) => Some(&function.decorator_list),
            Stmt::ClassDef(class) => Some(&class.decorator_list),
            _ => None,
        };
        if let Some(decorators) = decorators {
            let def_line = def_keyword_line(stmt, locator);
            let first_line = decorators
                .first()
                .map_or(def_line, |decorator| locator.start_line(decorator));
            if excluded.range(first_line..=def_line).next().is_some() {
                excluded.extend(first_line..=locator.end_line(stmt));
            }
        }

        if let Stmt::Match(match_stmt) = stmt {
            for case in &match_stmt.cases {
                exclude_irrefutable_case(case, locator, raw_statements, excluded);
            }
        }

        for child in child_bodies(stmt) {
            exclude_definitions(child, locator, raw_statements, excluded);
        }
    }
}

/// An irrefutable `case` (like `case _:`) is the `match` equivalent of an `else` clause: if
/// its whole body is excluded, its `case` line is excluded too.
fn exclude_irrefutable_case(
    case: &MatchCase,
    locator: &Locator<'_>,
    raw_statements: &LineSet,
    excluded: &mut LineSet,
) {
    if !is_irrefutable(case) {
        return;
    }
    let (Some(first), Some(last)) = (case.body.first(), case.body.last()) else {
        return;
    };
    let mut body_statements = raw_statements
        .range(python_lineno(first, locator)..=locator.end_line(last))
        .peekable();
    if body_statements.peek().is_some() && body_statements.all(|line| excluded.contains(line)) {
        excluded.extend(locator.start_line(&case.pattern)..=locator.end_line(&case.pattern));
    }
}

/// Whether a `case` always matches: a wildcard or bare-capture pattern without a guard.
pub(crate) fn is_irrefutable(case: &MatchCase) -> bool {
    if case.guard.is_some() {
        return false;
    }
    let mut pattern = &case.pattern;
    while let Pattern::MatchOr(or) = pattern {
        match or.patterns.last() {
            Some(last) => pattern = last,
            None => return false,
        }
    }
    while let Pattern::MatchAs(as_pattern) = pattern {
        match &as_pattern.pattern {
            Some(inner) => pattern = inner,
            None => return true,
        }
    }
    false
}
