use ruff_python_ast::token::TokenKind;
use ruff_text_size::Ranged;

use crate::Locator;

/// Maps every physical line to the first line of the logical line containing it.
///
/// This is coverage.py's `multiline_map`: a statement spanning several physical lines (an
/// open bracket, a backslash continuation, a triple-quoted string, ...) is reported on its
/// first line.
#[derive(Debug, Default)]
pub(crate) struct MultilineMap {
    /// `first[line]` is the first line of `line`'s statement, for multi-line statements only.
    first: Vec<u32>,
}

impl MultilineMap {
    pub(crate) fn from_tokens(locator: &Locator<'_>) -> Self {
        let mut first = Vec::new();
        let mut first_line = 0;

        for token in &**locator.tokens {
            match token.kind() {
                TokenKind::Newline => {
                    let end_line = locator.line(token.start());
                    if first_line != 0 && end_line != first_line {
                        let end = end_line as usize;
                        if first.len() <= end {
                            first.resize(end + 1, 0);
                        }
                        for slot in &mut first[first_line as usize..=end] {
                            *slot = first_line;
                        }
                    }
                    first_line = 0;
                }
                kind if is_significant(kind) && first_line == 0 => {
                    first_line = locator.line(token.start());
                }
                _ => {}
            }
        }

        Self { first }
    }

    pub(crate) fn first_line(&self, line: u32) -> u32 {
        match self.first.get(line as usize) {
            Some(&first) if first != 0 => first,
            _ => line,
        }
    }
}

/// Whether a token is a "non-whitespace" token in the sense of Python's `tokenize`: anything
/// but comments, newlines, indentation and the end-of-file marker.
pub(crate) fn is_significant(kind: TokenKind) -> bool {
    !matches!(
        kind,
        TokenKind::Comment
            | TokenKind::Newline
            | TokenKind::NonLogicalNewline
            | TokenKind::Indent
            | TokenKind::Dedent
            | TokenKind::EndOfFile
    )
}
