//! ParseError type.

use super::span::Span;

/// An error from lexing or parsing. M01 returns at most one of these per input
/// (stop-at-first-error policy, CLAUDE.md locked-in decision).
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    /// Human-readable, single-line message.
    pub message: String,
    /// Source span the error attaches to. For unexpected-EOF, this is a
    /// zero-length span at `src.len()`.
    pub span: Span,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ParseError {}
