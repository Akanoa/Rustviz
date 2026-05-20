//! Parser front-end: lexing, parsing, AST, errors.
//!
//! Public entry point: [`parse`]. See `specs/002-m01-frontend-skeleton/contracts/parse-api.md`
//! for the stable surface.

pub mod ast;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod span;
pub mod token;

use ast::Program;
use error::ParseError;
use span::{FileId, SourceMap};

/// Parse a source file (registered in `source_map`) to a Level 1 AST.
///
/// On success, returns the program AST with non-empty spans on every node.
/// On failure, returns a single [`ParseError`] per the stop-at-first-error
/// policy (FR-006).
pub fn parse(file: FileId, source_map: &SourceMap) -> Result<Program, ParseError> {
    let tokens = lexer::lex(file, source_map)?;
    parser::parse_tokens(tokens, file, source_map)
}
