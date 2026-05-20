//! rustviz — pedagogical visualizer for Rust ownership and borrowing.
//!
//! See `CLAUDE.md` for project goals and `MILESTONES.md` for the milestone roadmap.

#![warn(missing_docs, unused, dead_code, unreachable_pub)]
#![warn(clippy::all)]

pub mod parse;

pub use parse::ast;
pub use parse::error::ParseError;
pub use parse::parse;
pub use parse::span::{FileId, SourceMap, Span};
