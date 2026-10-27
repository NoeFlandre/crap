use ruff_python_ast::token::TokenKind;
use ruff_python_ast::{Stmt, StmtFunctionDef};
use ruff_text_size::Ranged;

use crate::complexity::complexity;
use crate::statements::{child_bodies, def_keyword_line, python_lineno};
use crate::{LineSet, Locator};

/// Whether a function is a plain function or a method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FunctionKind {
    /// A function defined at module level or inside another function.
    Function,
    /// A function defined directly in a class body.
    Method,
}

/// A function or method found in a module.
#[derive(Debug, Clone)]
pub struct Function {
    /// The function's name, e.g. `parse`.
    pub name: String,
    /// The dotted path of enclosing classes and functions, e.g. `Parser.parse`. This is the
    /// name coverage.py uses in its function-level reports.
    pub qualified_name: String,
    pub kind: FunctionKind,
    pub is_async: bool,
    /// The one-based line of the function's name.
    pub line: u32,
    /// The one-based (character) column of the function's name.
    pub column: u32,
    /// The line of the first decorator, or of the `def` keyword.
    pub start_line: u32,
    /// The line of the colon ending the signature.
    pub signature_end_line: u32,
    /// The last line of the function's body.
    pub end_line: u32,
    /// The lines that belong to this function and to no nested function, following
    /// coverage.py's region rules: the lines of the body, minus the bodies of nested
    /// functions. Includes blank and comment lines.
    pub lines: LineSet,
    /// Cyclomatic complexity of the function body.
    pub complexity: u32,
    /// Whether the function is excluded from measurement, e.g. by `# pragma: no cover`.
    pub excluded: bool,
}

pub(crate) fn find_functions(
    body: &[Stmt],
    locator: &Locator<'_>,
    excluded: &LineSet,
) -> Vec<Function> {
    let mut finder = FunctionFinder {
        locator,
        excluded,
        functions: Vec::new(),
        scopes: Vec::new(),
    };
    finder.visit_body(body);
    finder.functions
}

enum Scope {
    Class(String),
    /// A function, by index in `FunctionFinder::functions`.
    Function(usize),
}

struct FunctionFinder<'a, 'b> {
    locator: &'a Locator<'b>,
    excluded: &'a LineSet,
    functions: Vec<Function>,
    scopes: Vec<Scope>,
}

impl FunctionFinder<'_, '_> {
    fn visit_body(&mut self, body: &[Stmt]) {
        for stmt in body {
            match stmt {
                Stmt::FunctionDef(function) => self.visit_function(stmt, function),
                Stmt::ClassDef(class) => {
                    self.scopes.push(Scope::Class(class.name.to_string()));
                    self.visit_body(&class.body);
                    self.scopes.pop();
                }
                _ => {
                    for child in child_bodies(stmt) {
                        self.visit_body(child);
                    }
                }
            }
        }
    }

    fn visit_function(&mut self, stmt: &Stmt, function: &StmtFunctionDef) {
        let (Some(first), Some(last)) = (function.body.first(), function.body.last()) else {
            // Only possible with syntax errors.
            return;
        };
        let locator = self.locator;
        let body_start = python_lineno(first, locator);
        let end_line = locator.end_line(last);

        // Nested function bodies are excluded from the nearest enclosing function.
        if let Some(&enclosing) = self.scopes.iter().rev().find_map(|scope| match scope {
            Scope::Function(index) => Some(index),
            Scope::Class(_) => None,
        }) {
            self.functions[enclosing]
                .lines
                .retain(|line| !(body_start..=end_line).contains(line));
        }

        let name = function.name.to_string();
        let mut qualified_name = String::new();
        for scope in &self.scopes {
            match scope {
                Scope::Class(name) => qualified_name.push_str(name),
                Scope::Function(index) => qualified_name.push_str(&self.functions[*index].name),
            }
            qualified_name.push('.');
        }
        qualified_name.push_str(&name);

        let kind = if matches!(self.scopes.last(), Some(Scope::Class(_))) {
            FunctionKind::Method
        } else {
            FunctionKind::Function
        };

        let def_line = def_keyword_line(stmt, locator);
        let start_line = function
            .decorator_list
            .first()
            .map_or(def_line, |decorator| locator.start_line(decorator));
        let signature_end_line = locator
            .tokens
            .before(first.start())
            .iter()
            .rev()
            .find(|token| token.kind() == TokenKind::Colon)
            .map_or(def_line, |token| locator.line(token.start()));
        let (line, column) = locator.line_column(function.name.start());

        self.functions.push(Function {
            name,
            qualified_name,
            kind,
            is_async: function.is_async,
            line,
            column,
            start_line,
            signature_end_line,
            end_line,
            lines: (body_start..=end_line).collect(),
            complexity: complexity(&function.body),
            excluded: self.excluded.contains(&def_line),
        });

        self.scopes.push(Scope::Function(self.functions.len() - 1));
        self.visit_body(&function.body);
        self.scopes.pop();
    }
}
