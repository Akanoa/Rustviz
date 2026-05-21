//! rustviz — pedagogical visualizer for Rust ownership and borrowing.
//!
//! See `CLAUDE.md` for project goals and `MILESTONES.md` for the milestone roadmap.

#![warn(missing_docs, unused, dead_code, unreachable_pub)]
#![warn(clippy::all)]

pub mod eval;
pub mod event;
pub mod parse;
pub mod pipeline;
pub mod resolve;
pub mod typeck;
pub mod ui;

pub use eval::evaluate;
pub use event::{BorrowId, FrameId, HeapAddr, MemEvent, NoteKind, Pointee, SlotId, Value};
pub use parse::ast;
pub use parse::error::ParseError;
pub use parse::parse;
pub use parse::span::{FileId, SourceMap, Span};
pub use pipeline::{run_pipeline, CompileError, CompileStage};
pub use resolve::{resolve, BindingDecl, BindingId, BindingKind, Resolution};
pub use typeck::{typeck, BindingType, FnSig, Ty, TypeMap};
pub use ui::{Cursor, FrameCardView, PendingReturnView, SlotRowView, StateSnapshot, StatusView};
