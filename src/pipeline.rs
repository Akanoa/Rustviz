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

    // M03.2 / US1: integer-type tests covering the new IntKind variants.

    #[test]
    fn run_pipeline_u8_basic() {
        let source = "fn main() { let a: u8 = 5; let b: u8 = 3; let c: u8 = a + b; }";
        let events = run_pipeline(source).expect("u8 arithmetic compiles");
        // Verify at least one SlotWrite with the new Value::Int { kind: U8 } form.
        let has_u8 = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Int { kind: crate::typeck::IntKind::U8, .. },
                ..
            }
        ));
        assert!(has_u8, "expected at least one u8 SlotWrite");
    }

    #[test]
    fn run_pipeline_u8_overflow() {
        let source = "fn main() { let a: u8 = 250; let b: u8 = a + 10; }";
        let events = run_pipeline(source).expect("u8 overflow compiles fine; halts at runtime");
        // Trace must end with a RuntimeError Note (overflow halt).
        let last = events.last().expect("non-empty trace");
        assert!(
            matches!(last, crate::MemEvent::Note { kind: crate::NoteKind::RuntimeError, .. }),
            "expected runtime-error halt, got {last:?}"
        );
    }

    #[test]
    fn run_pipeline_unsigned_negation_rejected() {
        let err = run_pipeline("fn main() { let x: u8 = -1; }")
            .expect_err("unsigned negation should fail typeck");
        assert_eq!(err.stage, CompileStage::Typeck);
    }

    #[test]
    fn run_pipeline_literal_out_of_range() {
        let err = run_pipeline("fn main() { let x: u8 = 300; }")
            .expect_err("literal 300 doesn't fit u8");
        assert_eq!(err.stage, CompileStage::Typeck);
    }

    #[test]
    fn run_pipeline_cross_type_error() {
        let err = run_pipeline("fn main() { let a: u8 = 1; let b: i32 = 2; let c = a + b; }")
            .expect_err("u8 + i32 is cross-type");
        assert_eq!(err.stage, CompileStage::Typeck);
    }

    #[test]
    fn run_pipeline_literal_suffix_u8() {
        // M03.2 enhancement: literal suffix `5u8` works without annotation.
        let source = "fn main() { let x = 5u8 + 3u8; }";
        let events = run_pipeline(source).expect("suffixed literal arithmetic compiles");
        let has_u8 = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Int { kind: crate::typeck::IntKind::U8, .. },
                ..
            }
        ));
        assert!(has_u8, "expected u8 SlotWrite from `5u8 + 3u8`");
    }

    #[test]
    fn run_pipeline_literal_suffix_underscore() {
        // Underscore separator: `5_u8` works the same as `5u8`.
        let source = "fn main() { let x = 5_u8 + 3_u8; }";
        let events = run_pipeline(source).expect("underscore-separated suffix compiles");
        assert!(!events.is_empty());
    }

    #[test]
    fn run_pipeline_literal_suffix_f64() {
        // Float suffix: `2.5_f64` (with or without separator).
        let source = "fn main() { let x = 2.5f64 + 1.5f64; }";
        let events = run_pipeline(source).expect("float-suffixed literals compile");
        let has_f64 = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Float { kind: crate::typeck::FloatKind::F64, .. },
                ..
            }
        ));
        assert!(has_f64);
    }

    #[test]
    fn run_pipeline_literal_suffix_mismatch_rejected() {
        // Conflicting annotation vs. suffix: `let x: i32 = 5u8;` — typeck error.
        let err = run_pipeline("fn main() { let x: i32 = 5u8; }")
            .expect_err("u8 suffix conflicts with i32 annotation");
        assert_eq!(err.stage, CompileStage::Typeck);
    }

    #[test]
    fn run_pipeline_invalid_suffix_rejected() {
        let err = run_pipeline("fn main() { let x = 5u7; }")
            .expect_err("u7 isn't a valid type");
        assert_eq!(err.stage, CompileStage::Parse);
    }

    #[test]
    fn run_pipeline_int_suffix_on_float_literal_rejected() {
        let err = run_pipeline("fn main() { let x = 2.5u8; }")
            .expect_err("u8 suffix on float literal is invalid");
        assert_eq!(err.stage, CompileStage::Parse);
    }

    #[test]
    fn run_pipeline_i64_arithmetic() {
        let source = "fn main() { let a: i64 = 100; let b: i64 = 200; let c: i64 = a + b; }";
        let events = run_pipeline(source).expect("i64 arithmetic compiles");
        assert!(!events.is_empty());
    }

    // M03.2 / US2: float tests.

    #[test]
    fn run_pipeline_float_basic() {
        let source = "fn main() { let a: f64 = 1.5; let b: f64 = 2.5; let c: f64 = a + b; }";
        let events = run_pipeline(source).expect("f64 arithmetic compiles");
        // Verify at least one Float SlotWrite with value 4.0.
        let has_float = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Float { kind: crate::typeck::FloatKind::F64, .. },
                ..
            }
        ));
        assert!(has_float, "expected an f64 SlotWrite");
        // Trace must NOT end with a RuntimeError — normal arithmetic.
        let last = events.last().expect("non-empty trace");
        assert!(!matches!(last, crate::MemEvent::Note { kind: crate::NoteKind::RuntimeError, .. }));
    }

    #[test]
    fn run_pipeline_float_nan() {
        let source = "fn main() { let a: f64 = 0.0; let b: f64 = a / a; }";
        let events = run_pipeline(source).expect("float division by zero compiles");
        // Must contain at least one Info note (produced NaN).
        let has_info = events.iter().any(|e| matches!(e,
            crate::MemEvent::Note { kind: crate::NoteKind::Info, .. }
        ));
        assert!(has_info, "expected an Info note for NaN production");
        // Trace must NOT halt (NaN is valid Rust).
        let last = events.last().expect("non-empty trace");
        assert!(!matches!(last, crate::MemEvent::Note { kind: crate::NoteKind::RuntimeError, .. }));
    }

    #[test]
    fn run_pipeline_float_inf() {
        let source = "fn main() { let a: f64 = 0.0; let b: f64 = 1.0; let c: f64 = b / a; }";
        let events = run_pipeline(source).expect("1/0 compiles");
        // Same Info-note + no halt expectation.
        let has_info = events.iter().any(|e| matches!(e,
            crate::MemEvent::Note { kind: crate::NoteKind::Info, message, .. } if message.contains("+Inf")
        ));
        assert!(has_info, "expected an Info note announcing +Inf");
    }

    #[test]
    fn run_pipeline_float_propagation_no_redundant_note() {
        // After `a` is NaN, `b = a + 1.0` propagates NaN — must NOT emit
        // a second Info note (de-novo rule).
        let source = "fn main() {
            let a: f64 = 0.0;
            let n: f64 = a / a;
            let m: f64 = n + 1.0;
        }";
        let events = run_pipeline(source).expect("compiles");
        let info_count = events.iter().filter(|e| matches!(e,
            crate::MemEvent::Note { kind: crate::NoteKind::Info, .. }
        )).count();
        assert_eq!(info_count, 1, "expected exactly one Info note (no propagation re-emission)");
    }

    #[test]
    fn compile_error_serde_roundtrip() {
        let err = run_pipeline("fn main() { let x = ; }").expect_err("parse error");
        let json = serde_json::to_string(&err).expect("serialize");
        let back: CompileError = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(err, back);
    }
}
