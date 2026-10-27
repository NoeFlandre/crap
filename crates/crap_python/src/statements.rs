//! Finding the lines that hold executable statements.
//!
//! coverage.py takes these from the line table of the compiled bytecode. We cannot compile
//! Python, so we reconstruct the same information from the syntax tree: a statement produces
//! code on its first line unless it compiles to nothing (`global`, a bare annotation in a
//! function, ...) or is unreachable (code after `return`, the body of `if False:`, ...).

use ruff_python_ast::token::TokenKind;
use ruff_python_ast::{BoolOp, ElifElseClause, ExceptHandler, Expr, Number, Stmt, UnaryOp};
use ruff_text_size::Ranged;

use crate::exclusion::is_irrefutable;
use crate::{LineSet, Locator};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Module,
    Class,
    Function,
}

/// Returns the lines on which executable statements start, before exclusions are applied
/// and before multi-line statements are normalized (coverage.py's `raw_statements`).
pub(crate) fn raw_statements(body: &[Stmt], locator: &Locator<'_>) -> LineSet {
    let mut collector = StatementCollector {
        locator,
        lines: LineSet::new(),
    };
    collector.block(body, Scope::Module);
    collector.lines
}

/// Returns the lines of module, class and function docstrings.
pub(crate) fn docstrings(body: &[Stmt], locator: &Locator<'_>) -> LineSet {
    fn visit(body: &[Stmt], locator: &Locator<'_>, lines: &mut LineSet) {
        if let Some(docstring) = docstring(body) {
            lines.extend(locator.start_line(docstring)..=locator.end_line(docstring));
        }
        for stmt in body {
            for child in child_bodies(stmt) {
                if matches!(stmt, Stmt::FunctionDef(_) | Stmt::ClassDef(_)) {
                    visit(child, locator, lines);
                } else {
                    // Only the first statement of a module, class or function is a docstring.
                    for nested in child {
                        for grandchild in child_bodies(nested) {
                            visit_nested(nested, grandchild, locator, lines);
                        }
                    }
                }
            }
        }
    }

    fn visit_nested(parent: &Stmt, body: &[Stmt], locator: &Locator<'_>, lines: &mut LineSet) {
        if matches!(parent, Stmt::FunctionDef(_) | Stmt::ClassDef(_)) {
            visit(body, locator, lines);
        } else {
            for stmt in body {
                for child in child_bodies(stmt) {
                    visit_nested(stmt, child, locator, lines);
                }
            }
        }
    }

    let mut lines = LineSet::new();
    visit(body, locator, &mut lines);
    lines
}

/// The docstring of a module, class or function body, if any.
pub(crate) fn docstring(body: &[Stmt]) -> Option<&Stmt> {
    let first = body.first()?;
    if let Stmt::Expr(expr) = first
        && expr.value.is_string_literal_expr()
    {
        return Some(first);
    }
    None
}

/// The statement lists directly nested in `stmt`.
pub(crate) fn child_bodies(stmt: &Stmt) -> Vec<&[Stmt]> {
    match stmt {
        Stmt::FunctionDef(function) => vec![&function.body],
        Stmt::ClassDef(class) => vec![&class.body],
        Stmt::If(if_stmt) => std::iter::once(&*if_stmt.body)
            .chain(if_stmt.elif_else_clauses.iter().map(|clause| &*clause.body))
            .collect(),
        Stmt::For(for_stmt) => vec![&for_stmt.body, &for_stmt.orelse],
        Stmt::While(while_stmt) => vec![&while_stmt.body, &while_stmt.orelse],
        Stmt::With(with_stmt) => vec![&with_stmt.body],
        Stmt::Try(try_stmt) => std::iter::once(&*try_stmt.body)
            .chain(try_stmt.handlers.iter().map(|handler| {
                let ExceptHandler::ExceptHandler(handler) = handler;
                &*handler.body
            }))
            .chain([&*try_stmt.orelse, &*try_stmt.finalbody])
            .collect(),
        Stmt::Match(match_stmt) => match_stmt.cases.iter().map(|case| &*case.body).collect(),
        _ => Vec::new(),
    }
}

/// The line Python's `ast` reports for a statement: its first line, except for decorated
/// definitions, which are reported on their `def` or `class` line.
pub(crate) fn python_lineno(stmt: &Stmt, locator: &Locator<'_>) -> u32 {
    match stmt {
        Stmt::FunctionDef(_) | Stmt::ClassDef(_) => def_keyword_line(stmt, locator),
        _ => locator.start_line(stmt),
    }
}

/// The line of the `def` or `class` keyword of a definition (after any decorators).
pub(crate) fn def_keyword_line(stmt: &Stmt, locator: &Locator<'_>) -> u32 {
    let decorators = match stmt {
        Stmt::FunctionDef(function) => &function.decorator_list,
        Stmt::ClassDef(class) => &class.decorator_list,
        _ => return locator.start_line(stmt),
    };
    let Some(last) = decorators.last() else {
        return locator.start_line(stmt);
    };
    locator
        .tokens
        .after(last.end())
        .iter()
        .find(|token| {
            matches!(
                token.kind(),
                TokenKind::Def | TokenKind::Class | TokenKind::Async
            )
        })
        .map_or_else(
            || locator.start_line(stmt),
            |token| locator.line(token.start()),
        )
}

struct StatementCollector<'a, 'b> {
    locator: &'a Locator<'b>,
    lines: LineSet,
}

impl StatementCollector<'_, '_> {
    fn record(&mut self, node: &impl Ranged) {
        self.lines.insert(self.locator.start_line(node));
    }

    /// Records the statements of a block, returning whether control can fall off its end.
    fn block(&mut self, body: &[Stmt], scope: Scope) -> bool {
        for stmt in body {
            if !self.stmt(stmt, scope) {
                // Everything after this statement is unreachable and generates no code.
                return false;
            }
        }
        true
    }

    /// Records the lines of a statement, returning whether control can continue after it.
    fn stmt(&mut self, stmt: &Stmt, scope: Scope) -> bool {
        match stmt {
            Stmt::FunctionDef(function) => {
                self.definition(stmt, &function.decorator_list);
                self.block(&function.body, Scope::Function);
                true
            }
            Stmt::ClassDef(class) => {
                self.definition(stmt, &class.decorator_list);
                self.block(&class.body, Scope::Class);
                true
            }
            Stmt::Return(_) | Stmt::Raise(_) | Stmt::Break(_) | Stmt::Continue(_) => {
                self.record(stmt);
                false
            }
            Stmt::Global(_) | Stmt::Nonlocal(_) | Stmt::IpyEscapeCommand(_) => true,
            Stmt::AnnAssign(assign) => {
                // A bare annotation of a name in a function compiles to nothing.
                if assign.value.is_some()
                    || !assign.target.is_name_expr()
                    || scope != Scope::Function
                {
                    self.record(stmt);
                }
                true
            }
            Stmt::If(if_stmt) => {
                self.record(stmt);
                self.if_chain(
                    &if_stmt.test,
                    &if_stmt.body,
                    &if_stmt.elif_else_clauses,
                    scope,
                )
            }
            Stmt::While(while_stmt) => {
                self.record(stmt);
                match truthiness(&while_stmt.test) {
                    Some(false) => self.block(&while_stmt.orelse, scope),
                    Some(true) => {
                        self.block(&while_stmt.body, scope);
                        has_break(&while_stmt.body)
                    }
                    None => {
                        self.block(&while_stmt.body, scope);
                        let orelse = self.block(&while_stmt.orelse, scope);
                        orelse || has_break(&while_stmt.body)
                    }
                }
            }
            Stmt::For(for_stmt) => {
                self.record(stmt);
                self.block(&for_stmt.body, scope);
                let orelse = self.block(&for_stmt.orelse, scope);
                orelse || has_break(&for_stmt.body)
            }
            Stmt::With(with_stmt) => {
                self.record(stmt);
                self.block(&with_stmt.body, scope);
                // A context manager can swallow the exception that ended its body.
                true
            }
            Stmt::Try(try_stmt) => {
                self.record(stmt);
                let body = self.block(&try_stmt.body, scope);
                let mut handlers = false;
                for handler in &try_stmt.handlers {
                    let ExceptHandler::ExceptHandler(handler) = handler;
                    self.record(handler);
                    handlers |= self.block(&handler.body, scope);
                }
                let orelse = body && self.block(&try_stmt.orelse, scope);
                let finally = self.block(&try_stmt.finalbody, scope);
                finally && (orelse || handlers)
            }
            Stmt::Match(match_stmt) => {
                self.record(stmt);
                let mut falls_through = false;
                for case in &match_stmt.cases {
                    self.record(&case.pattern);
                    // A guard the compiler folds to false makes the body unreachable.
                    if case.guard.as_deref().and_then(truthiness) == Some(false) {
                        continue;
                    }
                    falls_through |= self.block(&case.body, scope);
                    if is_irrefutable(case) {
                        // Later cases are a syntax error; this one catches everything.
                        return falls_through;
                    }
                }
                // No case may match.
                true
            }
            Stmt::Assert(assert) => {
                self.record(stmt);
                // `assert False` always raises.
                truthiness(&assert.test) != Some(false)
            }
            // `del ()` deletes nothing and compiles to nothing.
            Stmt::Delete(delete)
                if delete.targets.iter().all(|target| match target {
                    Expr::Tuple(tuple) => tuple.is_empty(),
                    Expr::List(list) => list.is_empty(),
                    _ => false,
                }) =>
            {
                true
            }
            Stmt::Delete(_)
            | Stmt::TypeAlias(_)
            | Stmt::Assign(_)
            | Stmt::AugAssign(_)
            | Stmt::Import(_)
            | Stmt::ImportFrom(_)
            | Stmt::Expr(_)
            | Stmt::Pass(_) => {
                self.record(stmt);
                true
            }
        }
    }

    fn definition(&mut self, stmt: &Stmt, decorators: &[ruff_python_ast::Decorator]) {
        for decorator in decorators {
            self.record(decorator);
        }
        self.lines.insert(def_keyword_line(stmt, self.locator));
    }

    fn if_chain(
        &mut self,
        test: &Expr,
        body: &[Stmt],
        clauses: &[ElifElseClause],
        scope: Scope,
    ) -> bool {
        match truthiness(test) {
            // The body always runs; the other branches are dropped by the compiler.
            Some(true) => self.block(body, scope),
            // The body never runs.
            Some(false) => self.clauses(clauses, scope),
            None => {
                let body = self.block(body, scope);
                let rest = self.clauses(clauses, scope);
                body || rest
            }
        }
    }

    fn clauses(&mut self, clauses: &[ElifElseClause], scope: Scope) -> bool {
        let Some((clause, rest)) = clauses.split_first() else {
            // No `else`: control falls through when every test is false.
            return true;
        };
        match &clause.test {
            Some(test) => {
                self.record(clause);
                self.if_chain(test, &clause.body, rest, scope)
            }
            None => self.block(&clause.body, scope),
        }
    }
}

/// Whether `body` contains a `break` that exits the loop owning `body`.
fn has_break(body: &[Stmt]) -> bool {
    body.iter().any(|stmt| match stmt {
        Stmt::Break(_) => true,
        // A `break` in a nested loop's body exits that loop, but one in its `else` clause
        // exits ours.
        Stmt::For(for_stmt) => has_break(&for_stmt.orelse),
        Stmt::While(while_stmt) => has_break(&while_stmt.orelse),
        Stmt::FunctionDef(_) | Stmt::ClassDef(_) => false,
        _ => child_bodies(stmt).into_iter().any(has_break),
    })
}

/// The truth value of a test the compiler folds to a constant, if any.
fn truthiness(expr: &Expr) -> Option<bool> {
    match expr {
        Expr::BooleanLiteral(literal) => Some(literal.value),
        Expr::NoneLiteral(_) => Some(false),
        Expr::EllipsisLiteral(_) => Some(true),
        Expr::NumberLiteral(number) => Some(match &number.value {
            Number::Int(int) => *int != ruff_python_ast::Int::ZERO,
            Number::Float(float) => *float != 0.0,
            Number::Complex { real, imag } => *real != 0.0 || *imag != 0.0,
        }),
        Expr::StringLiteral(string) => Some(!string.value.is_empty()),
        Expr::BytesLiteral(bytes) => Some(!bytes.value.is_empty()),
        Expr::Name(name) if name.id.as_str() == "__debug__" => Some(true),
        Expr::UnaryOp(unary) if unary.op == UnaryOp::Not => truthiness(&unary.operand).map(|v| !v),
        // `0 and x` is folded to `0`, and `1 or x` to `1`.
        Expr::BoolOp(bool_op) => {
            let short_circuit = bool_op.op == BoolOp::Or;
            for value in &bool_op.values {
                match truthiness(value)? {
                    value if value == short_circuit => return Some(value),
                    _ => {}
                }
            }
            Some(!short_circuit)
        }
        _ => None,
    }
}
