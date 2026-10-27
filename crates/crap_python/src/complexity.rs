//! Cyclomatic complexity, computed the way [radon](https://radon.readthedocs.io) does.
//!
//! A function starts with a complexity of 1, and every decision point adds to it:
//!
//! | Construct                                    | Increment                          |
//! |----------------------------------------------|------------------------------------|
//! | `if`, `elif`                                 | +1                                 |
//! | conditional expression (`a if b else c`)     | +1                                 |
//! | `for`, `async for`, `while`                  | +1, and +1 for an `else` clause    |
//! | `try`                                        | +1 per `except`, +1 for `else`     |
//! | `match`                                      | +1 per `case`, minus a final `case _` |
//! | boolean operator (`a and b or c`)            | +1 per extra operand               |
//! | comprehension (`[x for x in y if z]`)        | +1 per `for`, +1 per `if`          |
//! | `assert`                                     | +1 (its condition is not inspected) |
//!
//! Nested functions and classes are scored on their own and do not add to the complexity of
//! the enclosing function. Lambdas are part of the function that contains them.

use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Comprehension, Expr, Stmt};

use crate::exclusion::is_irrefutable;

/// Computes the cyclomatic complexity of a function body.
pub fn complexity(body: &[Stmt]) -> u32 {
    let mut visitor = ComplexityVisitor { complexity: 1 };
    visitor.visit_body(body);
    visitor.complexity
}

struct ComplexityVisitor {
    complexity: u32,
}

impl<'a> Visitor<'a> for ComplexityVisitor {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            // Nested definitions are measured separately.
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => return,
            Stmt::If(if_stmt) => {
                let elifs = if_stmt
                    .elif_else_clauses
                    .iter()
                    .filter(|clause| clause.test.is_some())
                    .count();
                self.complexity += 1 + elifs as u32;
            }
            Stmt::For(for_stmt) => {
                self.complexity += 1 + u32::from(!for_stmt.orelse.is_empty());
            }
            Stmt::While(while_stmt) => {
                self.complexity += 1 + u32::from(!while_stmt.orelse.is_empty());
            }
            Stmt::Try(try_stmt) => {
                self.complexity +=
                    try_stmt.handlers.len() as u32 + u32::from(!try_stmt.orelse.is_empty());
            }
            Stmt::Match(match_stmt) => {
                let cases = match_stmt.cases.len() as u32;
                let catch_all = match_stmt.cases.iter().any(is_irrefutable);
                self.complexity += cases - u32::from(catch_all);
            }
            // Like radon, an assertion counts as a single decision, whatever its condition.
            Stmt::Assert(_) => {
                self.complexity += 1;
                return;
            }
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::If(_) => self.complexity += 1,
            Expr::BoolOp(bool_op) => {
                self.complexity += bool_op.values.len().saturating_sub(1) as u32;
            }
            _ => {}
        }
        visitor::walk_expr(self, expr);
    }

    fn visit_comprehension(&mut self, comprehension: &'a Comprehension) {
        self.complexity += 1 + comprehension.ifs.len() as u32;
        visitor::walk_comprehension(self, comprehension);
    }
}
