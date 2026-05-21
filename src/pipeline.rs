//! M05: consolidated `parse → resolve → typeck → evaluate` pipeline.
//!
//! Single canonical entry point for both the WASM `Player::set_source` path
//! (live editor input) and the CLI `gen_traces` binary (offline verification).
//! The four stages all return `Result<_, ParseError>` already; this module
//! wraps the chain and tags each stage so consumers can distinguish where
//! a failure originated without losing the span or message.

use crate::event::MemEvent;
use crate::parse::error::ParseError;
use crate::parse::span::{FileId, Span};
use crate::{evaluate, parse, resolve, typeck};

/// **M05**: a compile-stage error surfaced to consumers. Wraps the underlying
/// `ParseError`'s span + message and tags which stage failed.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CompileError {
    /// Source location of the error (byte offsets + FileId).
    pub span: Span,
    /// Which pipeline stage produced the error.
    pub stage: CompileStage,
    /// Human-readable error message.
    pub message: String,
}

/// **M05**: pipeline stage labels. Closed enum from M05 — additive growth only,
/// via the same revision-milestone rule that governs `MemEvent` and `Ty`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CompileStage {
    /// Lexer / parser stage (`parse(...)`).
    Parse,
    /// Name resolution stage (`resolve(...)`).
    Resolve,
    /// Type-check stage (`typeck(...)`).
    Typeck,
    /// Tree-walking evaluator stage (`evaluate(...)`).
    Eval,
}

impl CompileError {
    fn from_parse_error(err: ParseError, stage: CompileStage) -> Self {
        Self {
            span: err.span,
            stage,
            message: err.message,
        }
    }
}

/// **M05**: run the M01 → M02 → M03 pipeline on a single in-memory source
/// string. Returns the event stream on success, or a `CompileError` tagged
/// with the first failing stage.
///
/// The pipeline short-circuits via `?` — only the first failing stage is
/// reported (matching the M01 "stop at first error" policy).
///
/// The source is added to a fresh `SourceMap` under the name `editor.rs`.
/// Callers needing multi-file support build their own `SourceMap` and call
/// each stage directly.
pub fn run_pipeline(source: &str) -> Result<Vec<MemEvent>, CompileError> {
    let mut sm = crate::parse::span::SourceMap::new();
    let file = sm.add("editor.rs".to_owned(), source.to_owned());

    let program = parse(file, &sm)
        .map_err(|e| CompileError::from_parse_error(e, CompileStage::Parse))?;
    let resolution = resolve(&program)
        .map_err(|e| CompileError::from_parse_error(e, CompileStage::Resolve))?;
    let types = typeck(&program, &resolution)
        .map_err(|e| CompileError::from_parse_error(e, CompileStage::Typeck))?;
    let events = evaluate(&program, &resolution, &types)
        .map_err(|e| CompileError::from_parse_error(e, CompileStage::Eval))?;

    let _ = FileId(file.0); // silence unused-import-on-feature gate; keeps re-export honest
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_pipeline_minimal() {
        let events = run_pipeline("fn main() { let x = 5; }")
            .expect("minimal program compiles");
        // Expect at minimum: FrameEnter, SlotAlloc, SlotWrite, ReturnValue, FrameLeave.
        assert!(events.len() >= 5);
        assert!(matches!(events[0], MemEvent::FrameEnter { .. }));
    }

    #[test]
    fn run_pipeline_arithmetic() {
        let events = run_pipeline("fn main() { let x = 2 + 3; }")
            .expect("arithmetic compiles");
        assert!(!events.is_empty());
    }

    #[test]
    fn run_pipeline_fn_call() {
        let source = "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\nfn main() {\n    let r = add(2, 3);\n}\n";
        let events = run_pipeline(source).expect("fn call compiles");
        // M03.1 post-revision: m03_fn_call's trace has 12 events.
        assert_eq!(events.len(), 12);
    }

    #[test]
    fn run_pipeline_parse_error() {
        let err = run_pipeline("fn main() { let x = ; }")
            .expect_err("missing initializer should fail to parse");
        assert_eq!(err.stage, CompileStage::Parse);
        assert!(err.span.end > err.span.start);
        assert!(!err.message.is_empty());
    }

    #[test]
    fn run_pipeline_resolve_error() {
        let err = run_pipeline("fn main() { let y = undefined_var; }")
            .expect_err("undefined ident should fail to resolve");
        assert_eq!(err.stage, CompileStage::Resolve);
        assert!(!err.message.is_empty());
    }

    #[test]
    fn run_pipeline_typeck_error() {
        let err = run_pipeline("fn main() { let z: i32 = true; }")
            .expect_err("i32 = true should fail typeck");
        assert_eq!(err.stage, CompileStage::Typeck);
        assert!(!err.message.is_empty());
    }

    #[test]
    fn compile_error_serde_roundtrip() {
        let err = run_pipeline("fn main() { let x = ; }").expect_err("parse error");
        let json = serde_json::to_string(&err).expect("serialize");
        let back: CompileError = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(err, back);
    }
}
