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

    // M06 / US1: shared borrow.
    #[test]
    fn run_pipeline_shared_borrow() {
        let source = "fn main() { let x = 5; let r = &x; }";
        let events = run_pipeline(source).expect("shared borrow compiles");
        let has_shared = events.iter().any(|e| matches!(e,
            crate::MemEvent::BorrowShared { .. }));
        let has_end = events.iter().any(|e| matches!(e,
            crate::MemEvent::BorrowEnd { .. }));
        assert!(has_shared, "expected a BorrowShared event");
        assert!(has_end, "expected a BorrowEnd event");
    }

    #[test]
    fn run_pipeline_shared_borrow_multiple() {
        // Multiple shared borrows OK in Rust.
        let source = "fn main() { let x = 5; let r1 = &x; let r2 = &x; }";
        let events = run_pipeline(source).expect("two shared borrows are valid");
        let shared_count = events.iter().filter(|e| matches!(e,
            crate::MemEvent::BorrowShared { .. })).count();
        assert_eq!(shared_count, 2);
    }

    // M06 / US2: mutable borrow.
    #[test]
    fn run_pipeline_mut_borrow() {
        let source = "fn main() { let mut x = 5; let r = &mut x; }";
        let events = run_pipeline(source).expect("mut borrow compiles");
        let has_mut = events.iter().any(|e| matches!(e,
            crate::MemEvent::BorrowMut { .. }));
        assert!(has_mut, "expected a BorrowMut event");
    }

    #[test]
    fn run_pipeline_mut_borrow_on_non_mut_rejected() {
        let err = run_pipeline("fn main() { let x = 5; let r = &mut x; }")
            .expect_err("cannot &mut a non-mut binding");
        assert_eq!(err.stage, CompileStage::Typeck);
    }

    // M06 / US3: aliasing rule violations.
    #[test]
    fn run_pipeline_shared_then_mut_rejected() {
        let err = run_pipeline("fn main() { let mut x = 5; let r1 = &x; let r2 = &mut x; }")
            .expect_err("&mut while & exists is invalid");
        assert_eq!(err.stage, CompileStage::Typeck);
    }

    #[test]
    fn run_pipeline_two_mut_rejected() {
        let err = run_pipeline("fn main() { let mut x = 5; let r1 = &mut x; let r2 = &mut x; }")
            .expect_err("two &mut on same binding is invalid");
        assert_eq!(err.stage, CompileStage::Typeck);
    }

    #[test]
    fn run_pipeline_mut_then_shared_rejected() {
        let err = run_pipeline("fn main() { let mut x = 5; let r1 = &mut x; let r2 = &x; }")
            .expect_err("& while &mut exists is invalid");
        assert_eq!(err.stage, CompileStage::Typeck);
    }

    // M06 / US4: scope-level lifetime end.
    #[test]
    fn run_pipeline_scoped_borrow_ends_at_inner_brace() {
        let source = "fn main() { let x = 5; { let r = &x; } }";
        let events = run_pipeline(source).expect("scoped borrow compiles");
        // Find positions of BorrowShared and BorrowEnd; BorrowEnd must come
        // before the outer SlotDrop for x (if any), but more concretely,
        // before the function's FrameLeave.
        let mut borrow_end_idx = None;
        let mut frame_leave_idx = None;
        for (i, e) in events.iter().enumerate() {
            match e {
                crate::MemEvent::BorrowEnd { .. } => borrow_end_idx = Some(i),
                crate::MemEvent::FrameLeave { .. } => frame_leave_idx = Some(i),
                _ => {}
            }
        }
        let end = borrow_end_idx.expect("BorrowEnd should fire");
        let leave = frame_leave_idx.expect("FrameLeave should fire");
        assert!(end < leave, "BorrowEnd at inner `}}` should precede main's FrameLeave");
    }

    // M06: place-expression check — borrowing a non-place is rejected.
    #[test]
    fn run_pipeline_borrow_of_literal_rejected() {
        let err = run_pipeline("fn main() { let r = &5; }")
            .expect_err("&literal is invalid (not a place)");
        assert_eq!(err.stage, CompileStage::Typeck);
    }

    // M06.1 / US1: direct assignment to `let mut` bindings.

    #[test]
    fn run_pipeline_assign_basic() {
        let source = "fn main() { let mut x = 0; x = 7; }";
        let events = run_pipeline(source).expect("direct assign compiles");
        let writes = events
            .iter()
            .filter(|e| matches!(e, crate::MemEvent::SlotWrite { .. }))
            .count();
        assert!(writes >= 2, "expected ≥ 2 SlotWrite events (init + reassign)");
    }

    #[test]
    fn run_pipeline_assign_immutable_rejected() {
        let err = run_pipeline("fn main() { let x = 0; x = 7; }")
            .expect_err("immutable binding rejects reassignment");
        assert_eq!(err.stage, CompileStage::Typeck);
    }

    // M06.1 / US2: deref-read.

    #[test]
    fn run_pipeline_deref_read_shared() {
        let source = "fn main() { let x = 42; let r = &x; let y = *r; }";
        let events = run_pipeline(source).expect("deref-read through &T compiles");
        let has_42 = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Int { bits: 42, .. },
                ..
            }
        ));
        assert!(has_42, "expected a SlotWrite with value 42 (y bound through *r)");
    }

    #[test]
    fn run_pipeline_deref_on_non_reference_rejected() {
        let err = run_pipeline("fn main() { let x = 5; let y = *x; }")
            .expect_err("deref of non-reference is invalid");
        assert_eq!(err.stage, CompileStage::Typeck);
    }

    // M06.1 / US3: deref-write.

    #[test]
    fn run_pipeline_deref_write_basic() {
        let source = "fn main() { let mut x = 5; let r = &mut x; *r = 10; }";
        let events = run_pipeline(source).expect("through-ref write compiles");
        let has_10 = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Int { bits: 10, .. },
                ..
            }
        ));
        assert!(has_10, "expected a SlotWrite with value 10 (through *r)");
    }

    #[test]
    fn run_pipeline_deref_write_through_shared_rejected() {
        let err = run_pipeline("fn main() { let x = 5; let r = &x; *r = 10; }")
            .expect_err("cannot assign through &T");
        assert_eq!(err.stage, CompileStage::Typeck);
    }

    #[test]
    fn run_pipeline_assign_to_borrowed_rejected() {
        let err = run_pipeline("fn main() { let mut x = 5; let r = &x; x = 7; }")
            .expect_err("cannot assign to a borrowed binding");
        assert_eq!(err.stage, CompileStage::Typeck);
    }

    // M07 / US1: Box owning arrow + HeapAlloc/Free.

    #[test]
    fn run_pipeline_box_basic() {
        let source = "fn main() { let b = Box::new(5); }";
        let events = run_pipeline(source).expect("box compiles");
        let alloc_count = events.iter().filter(|e| matches!(e, crate::MemEvent::HeapAlloc { .. })).count();
        let free_count = events.iter().filter(|e| matches!(e, crate::MemEvent::HeapFree { .. })).count();
        assert_eq!(alloc_count, 1, "expected exactly 1 HeapAlloc");
        assert_eq!(free_count, 1, "expected exactly 1 HeapFree");
    }

    #[test]
    fn run_pipeline_box_deref_read() {
        let source = "fn main() { let b = Box::new(42); let y = *b; }";
        let events = run_pipeline(source).expect("box deref compiles");
        let has_42 = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite { value: crate::Value::Int { bits: 42, .. }, .. }
        ));
        assert!(has_42, "expected y to be written with value 42 (via *b)");
    }

    // M07 / US2: Vec realloc + dangling.

    #[test]
    fn run_pipeline_vec_push_grows() {
        let source = "fn main() {
            let mut v: Vec<i32> = Vec::new();
            v.push(1);
            v.push(2);
            v.push(3);
        }";
        let events = run_pipeline(source).expect("vec push compiles");
        // Vec::new emits 1 HeapAlloc (cap 0). Pushes 1,2,3 each cross a
        // capacity boundary (0→1, 1→2, 2→4) = 3 HeapReallocs.
        let realloc_count = events.iter().filter(|e| matches!(e, crate::MemEvent::HeapRealloc { .. })).count();
        assert!(realloc_count >= 2, "expected ≥ 2 HeapReallocs for capacity-growing pushes, got {realloc_count}");
    }

    #[test]
    fn run_pipeline_vec_index_basic() {
        let source = "fn main() {
            let mut v: Vec<i32> = Vec::new();
            v.push(5);
            let x = v[0];
        }";
        let events = run_pipeline(source).expect("vec index compiles");
        let has_5 = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite { value: crate::Value::Int { bits: 5, .. }, .. }
        ));
        assert!(has_5, "expected x = v[0] = 5");
    }

    #[test]
    fn run_pipeline_vec_index_oob() {
        let source = "fn main() {
            let v: Vec<i32> = Vec::new();
            let x = v[0];
        }";
        let events = run_pipeline(source).expect("vec OOB compiles; halts at runtime");
        let has_oob = events.iter().any(|e| matches!(e,
            crate::MemEvent::Note { kind: crate::NoteKind::RuntimeError, message, .. }
                if message.contains("index out of bounds")
        ));
        assert!(has_oob, "expected runtime error for OOB index");
    }

    #[test]
    fn run_pipeline_vec_dangling_borrow() {
        // The headline dangling demo: &v[0] becomes dangling after v.push
        // triggers a *copy*-realloc. M07.1 update: the allocator now grows
        // in place when there's room, so the demo needs a Box blocker
        // physically occupying the adjacent region to force the copy.
        let source = "fn main() {
            let mut v: Vec<i32> = Vec::new();
            v.push(1);
            v.push(2);
            let r = &v[0];
            let b = Box::new(99);
            v.push(3);
        }";
        let events = run_pipeline(source).expect("vec dangling compiles");
        let has_dangling = events.iter().any(|e| matches!(e,
            crate::MemEvent::Note { kind: crate::NoteKind::RuntimeError, message, .. }
                if message.contains("dangling reference")
        ));
        assert!(has_dangling, "expected dangling-reference RuntimeError at realloc");
    }

    // M07 / US3: String. M07.2 re-baseline: count HeapAlloc + StaticAlloc
    // separately so the test verifies both the heap allocation for the
    // String's buffer AND the static-region interning of the literal.

    #[test]
    fn run_pipeline_string_from() {
        let source = "fn main() { let s = String::from(\"hi\"); }";
        let events = run_pipeline(source).expect("String compiles");
        let heap_count = events.iter().filter(|e| matches!(e, crate::MemEvent::HeapAlloc { .. })).count();
        let static_count = events.iter().filter(|e| matches!(e, crate::MemEvent::StaticAlloc { .. })).count();
        assert_eq!(heap_count, 1, "expected one heap allocation for the String buffer");
        assert_eq!(static_count, 1, "expected one static block for the \"hi\" literal");
    }

    #[test]
    fn run_pipeline_string_push_str_realloc() {
        let source = "fn main() {
            let mut s = String::from(\"hi\");
            s.push_str(\"world\");
        }";
        let events = run_pipeline(source).expect("String push_str compiles");
        let realloc_count = events.iter().filter(|e| matches!(e, crate::MemEvent::HeapRealloc { .. })).count();
        let static_count = events.iter().filter(|e| matches!(e, crate::MemEvent::StaticAlloc { .. })).count();
        assert!(realloc_count >= 1, "expected ≥ 1 HeapRealloc when push_str grows capacity");
        // M07.2: both literals interned in static (one for "hi", one for "world").
        assert_eq!(static_count, 2, "expected two static blocks for \"hi\" and \"world\"");
    }

    // ─── M07.1: typeck rejections (mutable slice, standalone range) ───────

    #[test]
    fn run_pipeline_mut_slice_rejected() {
        let source = "fn main() {
            let mut v: Vec<i32> = Vec::new();
            v.push(1);
            let s = &mut v[..];
        }";
        let err = run_pipeline(source).expect_err("mutable slice must be rejected");
        assert_eq!(err.stage, CompileStage::Typeck);
        assert!(
            err.message.contains("mutable slices are out of scope"),
            "expected mutable-slice rejection message, got: {}",
            err.message
        );
    }

    /// Standalone range outside `[ ]` is rejected by the parser (M07.1 only
    /// accepts `..` inside index brackets — see research R-003). The error
    /// surfaces as a parse-stage failure pointing at the unexpected `..`.
    #[test]
    fn run_pipeline_standalone_range_rejected() {
        let source = "fn main() { let r = 1..3; }";
        let err = run_pipeline(source).expect_err("standalone range must be rejected");
        assert_eq!(err.stage, CompileStage::Parse);
    }

    // ─── M07.1 / US3: slice dangles after Vec realloc ─────────────────────

    /// A slice taken before a Vec realloc produces a `Note { RuntimeError }`
    /// at the realloc step — same pedagogy as M07's `&v[0]` case but at
    /// slice granularity.
    #[test]
    fn run_pipeline_slice_dangling() {
        // M07.1: the allocator does in-place growth when nothing physically
        // blocks it — so a slice + naive push wouldn't dangle (the Vec just
        // grows where it is). The dangling demo needs a Box::new blocker
        // sitting in the adjacent region to force a copy-realloc, which IS
        // what invalidates the slice.
        let source = "fn main() {
            let mut v: Vec<i32> = Vec::new();
            v.push(1);
            v.push(2);
            let s = &v[..];
            let b = Box::new(99);
            v.push(3);
        }";
        let events = run_pipeline(source).expect("slice dangling compiles");
        let has_dangling = events.iter().any(|e| matches!(e,
            crate::MemEvent::Note { kind: crate::NoteKind::RuntimeError, message, .. }
                if message.contains("dangling reference")
        ));
        assert!(has_dangling, "expected dangling-reference RuntimeError on Vec realloc with active slice");
    }

    #[test]
    fn compile_error_serde_roundtrip() {
        let err = run_pipeline("fn main() { let x = ; }").expect_err("parse error");
        let json = serde_json::to_string(&err).expect("serialize");
        let back: CompileError = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(err, back);
    }

    // ─── M07.2 / US1: string literal as `&str` (slice into static memory) ──

    /// A string literal `let s = "toto";` typechecks as `&str`, evaluates
    /// to a `Value::Slice` targeting the static region, emits exactly one
    /// `StaticAlloc` event and one `BorrowShared` with `Pointee::Static`,
    /// and emits **zero** `HeapAlloc` events.
    #[test]
    fn run_pipeline_str_literal() {
        let source = "fn main() { let s = \"toto\"; }";
        let events = run_pipeline(source).expect("str literal compiles");
        // Exactly one StaticAlloc with the literal's bytes.
        let static_allocs: Vec<&str> = events.iter().filter_map(|e| match e {
            crate::MemEvent::StaticAlloc { bytes, .. } => Some(bytes.as_str()),
            _ => None,
        }).collect();
        assert_eq!(static_allocs, vec!["toto"], "expected exactly one StaticAlloc with bytes 'toto'");
        // Zero HeapAlloc — string literals don't allocate at runtime.
        let heap_count = events.iter().filter(|e| matches!(e, crate::MemEvent::HeapAlloc { .. })).count();
        assert_eq!(heap_count, 0, "expected zero HeapAlloc events for a bare literal");
        // BorrowShared with Pointee::Static target.
        let shared_static = events.iter().any(|e| matches!(e,
            crate::MemEvent::BorrowShared { target: crate::event::Pointee::Static(_), .. }
        ));
        assert!(shared_static, "expected BorrowShared with Pointee::Static target");
        // s's SlotWrite carries Value::Slice with len 4.
        let slice_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite { value: crate::Value::Slice { len: 4, target: crate::event::Pointee::Static(_), .. }, .. }
        ));
        assert!(slice_write, "expected SlotWrite of Value::Slice {{ len: 4, target: Pointee::Static(_), .. }}");
    }

    /// `String::from(s)` where `s` is an existing `&str` binding (not a
     /// literal) is accepted by typeck and produces the same trace shape as
     /// the literal form: 1 StaticAlloc (for `s`'s literal) + 1 HeapAlloc
     /// (for the String's buffer) + 1 BytesCopy (data flow).
    #[test]
    fn run_pipeline_string_from_str_binding() {
        let source = "fn main() {
            let s = \"hi\";
            let t = String::from(s);
        }";
        let events = run_pipeline(source).expect("String::from(&str binding) compiles");
        let static_count = events.iter().filter(|e| matches!(e, crate::MemEvent::StaticAlloc { .. })).count();
        let heap_count = events.iter().filter(|e| matches!(e, crate::MemEvent::HeapAlloc { .. })).count();
        let copy_count = events.iter().filter(|e| matches!(e, crate::MemEvent::BytesCopy { .. })).count();
        assert_eq!(static_count, 1, "expected one static block for \"hi\"");
        assert_eq!(heap_count, 1, "expected one heap allocation for String t");
        assert_eq!(copy_count, 1, "expected one BytesCopy event from static to heap");
    }

    /// `String::from(&s[1..3])` — a sub-slice of an existing `&str` — copies
    /// just the sub-slice's bytes (3 bytes "ell" from "hello"), not the
    /// whole source.
    #[test]
    fn run_pipeline_string_from_subslice() {
        let source = "fn main() {
            let s = \"hello\";
            let t = String::from(&s[1..4]);
        }";
        let events = run_pipeline(source).expect("compiles");
        let copy = events.iter().find_map(|e| match e {
            crate::MemEvent::BytesCopy { n_bytes, .. } => Some(*n_bytes),
            _ => None,
        });
        assert_eq!(copy, Some(3), "expected BytesCopy of exactly 3 bytes (sub-slice [1..4] of \"hello\")");
    }

    // ─── M07.2 / US2: `String::from` copies static bytes to heap ──────────

    /// `String::from("hi")` produces a trace where both the static `"hi"`
    /// block AND a fresh heap `String` block coexist. Verifies BOTH the
    /// alloc events fire AND no spurious extra events are emitted.
    #[test]
    fn run_pipeline_string_from_static_visible() {
        let source = "fn main() { let s = String::from(\"hi\"); }";
        let events = run_pipeline(source).expect("String::from compiles");
        // Exactly one StaticAlloc for the "hi" literal.
        let static_allocs: Vec<&str> = events.iter().filter_map(|e| match e {
            crate::MemEvent::StaticAlloc { bytes, .. } => Some(bytes.as_str()),
            _ => None,
        }).collect();
        assert_eq!(static_allocs, vec!["hi"], "expected one StaticAlloc with bytes 'hi'");
        // Exactly one HeapAlloc for the String's buffer.
        let heap_count = events.iter().filter(|e| matches!(e, crate::MemEvent::HeapAlloc { .. })).count();
        assert_eq!(heap_count, 1, "expected one heap allocation");
        // The trace ends with the heap String being freed (scope exit).
        let free_count = events.iter().filter(|e| matches!(e, crate::MemEvent::HeapFree { .. })).count();
        assert_eq!(free_count, 1, "expected the heap String to be freed at scope exit");
        // The static block is NEVER freed — no StaticFree event variant exists.
    }

    // ─── M07.2 / US3: `push_str` + `s.len()` on `&str` ────────────────────

    /// `push_str("!")` — the literal arg interns in static; bytes flow from
    /// the static block into the heap String's buffer. No separate heap
    /// allocation for the argument.
    #[test]
    fn run_pipeline_push_str_static() {
        let source = "fn main() {
            let mut s = String::from(\"hi\");
            s.push_str(\"!\");
        }";
        let events = run_pipeline(source).expect("push_str compiles");
        let static_count = events.iter().filter(|e| matches!(e, crate::MemEvent::StaticAlloc { .. })).count();
        let heap_count = events.iter().filter(|e| matches!(e, crate::MemEvent::HeapAlloc { .. })).count();
        assert_eq!(static_count, 2, "expected two static blocks (one for \"hi\", one for \"!\")");
        assert_eq!(heap_count, 1, "expected one heap allocation (the String's buffer)");
    }

    /// Sub-slicing a `&str` produces another `&str` pointing into the same
    /// static block, with offsets adjusted. `let s = "hello"; let s2 = &s[..2];`
    /// gives `s2` viewing bytes 0..2 ("he") of the static "hello".
    #[test]
    fn run_pipeline_str_subslice() {
        let source = "fn main() {
            let s = \"hello\";
            let s2 = &s[..2];
        }";
        let events = run_pipeline(source).expect("&str sub-slice compiles");
        // One StaticAlloc for "hello"; no separate alloc for the sub-slice
        // (it shares the same static block).
        let static_count = events.iter().filter(|e| matches!(e, crate::MemEvent::StaticAlloc { .. })).count();
        assert_eq!(static_count, 1, "sub-slice should reuse the parent's static block");
        // Two BorrowShared events targeting the same Pointee::Static — one
        // for the literal `s`, one for the sub-slice `s2`.
        let shared_static_count = events.iter().filter(|e| matches!(e,
            crate::MemEvent::BorrowShared { target: crate::event::Pointee::Static(_), .. }
        )).count();
        assert_eq!(shared_static_count, 2, "expected two static-targeted borrows (literal + sub-slice)");
        // s2's SlotWrite has Value::Slice with len 2 + byte_offset 0 + byte_len 2.
        let s2_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Slice {
                    target: crate::event::Pointee::Static(_),
                    start: 0, len: 2, byte_offset: 0, byte_len: 2, ..
                },
                ..
            }
        ));
        assert!(s2_write, "expected s2 = &s[..2] to produce Value::Slice {{ len: 2, byte_offset: 0, .. }}");
    }

    /// Sub-slicing with a non-zero start preserves byte_offset accumulation:
    /// `&s[1..]` should pick up at byte 1 of the static block.
    #[test]
    fn run_pipeline_str_subslice_with_start() {
        let source = "fn main() {
            let s = \"hello\";
            let s2 = &s[1..4];
        }";
        let events = run_pipeline(source).expect("compiles");
        let s2_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Slice {
                    target: crate::event::Pointee::Static(_),
                    start: 1, len: 3, byte_offset: 1, byte_len: 3, ..
                },
                ..
            }
        ));
        assert!(s2_write, "expected s2 = &s[1..4] → Value::Slice {{ start: 1, len: 3, byte_offset: 1, byte_len: 3 }}");
    }

    /// `s.len()` on `&str` returns the byte length as `u64`.
    #[test]
    fn run_pipeline_str_len() {
        let source = "fn main() { let s = \"toto\"; let n = s.len(); }";
        let events = run_pipeline(source).expect("str len compiles");
        let n_is_4 = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Int { kind: crate::typeck::IntKind::U64, bits: 4 },
                ..
            }
        ));
        assert!(n_is_4, "expected n = s.len() to be Int {{ U64, 4 }}");
    }

    /// Two identical literals share one StaticAlloc (content-deduplicated
    /// to match Rust linker behavior). Each occurrence still gets its own
    /// `BorrowShared`.
    #[test]
    fn run_pipeline_literal_dedup() {
        let source = "fn main() { let a = \"hi\"; let b = \"hi\"; }";
        let events = run_pipeline(source).expect("literal dedup compiles");
        let static_count = events.iter().filter(|e| matches!(e, crate::MemEvent::StaticAlloc { .. })).count();
        assert_eq!(static_count, 1, "two identical literals should share one StaticAlloc");
        let shared_count = events.iter().filter(|e| matches!(e,
            crate::MemEvent::BorrowShared { target: crate::event::Pointee::Static(_), .. }
        )).count();
        assert_eq!(shared_count, 2, "each literal occurrence should fire its own BorrowShared");
    }

    // ─── M07.1 / US1: partial-range slice ────────────────────────────────

    /// A slice `&v[1..3]` typechecks, emits a `BorrowShared` event whose
    /// target is the Vec's heap allocation, and the binding's SlotWrite
    /// carries a `Value::Slice { len: 2, .. }`.
    #[test]
    fn run_pipeline_slice_range() {
        let source = "fn main() {
            let mut v: Vec<i32> = Vec::new();
            v.push(10);
            v.push(20);
            v.push(30);
            v.push(40);
            let s = &v[1..3];
        }";
        let events = run_pipeline(source).expect("slice range compiles");
        // BorrowShared with Pointee::Heap target.
        let shared_heap = events.iter().any(|e| matches!(e,
            crate::MemEvent::BorrowShared { target: crate::event::Pointee::Heap(_), .. }
        ));
        assert!(shared_heap, "expected BorrowShared with Heap target for slice borrow");
        // SlotWrite of Value::Slice with len 2.
        let slice_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite { value: crate::Value::Slice { len: 2, .. }, .. }
        ));
        assert!(slice_write, "expected SlotWrite of Value::Slice {{ len: 2, .. }}");
    }

    #[test]
    fn run_pipeline_slice_oob_end() {
        let source = "fn main() {
            let mut v: Vec<i32> = Vec::new();
            v.push(1);
            let s = &v[0..5];
        }";
        let events = run_pipeline(source).expect("slice OOB compiles; halts at runtime");
        let has_oob = events.iter().any(|e| matches!(e,
            crate::MemEvent::Note { kind: crate::NoteKind::RuntimeError, message, .. }
                if message.contains("slice end out of bounds")
        ));
        assert!(has_oob, "expected runtime error for OOB slice end");
    }

    // ─── M07.1 / US2: full-vec slice + `s.len()` ─────────────────────────

    #[test]
    fn run_pipeline_slice_basic() {
        let source = "fn main() {
            let mut v: Vec<i32> = Vec::new();
            v.push(1);
            v.push(2);
            v.push(3);
            let s = &v[..];
            let n = s.len();
        }";
        let events = run_pipeline(source).expect("slice basic compiles");
        // `s` is a slice with len 3.
        let slice_len_3 = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite { value: crate::Value::Slice { len: 3, .. }, .. }
        ));
        assert!(slice_len_3, "expected Value::Slice {{ len: 3, .. }} for &v[..]");
        // `n = s.len()` produces a U64 3.
        let n_eq_3 = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Int { kind: crate::typeck::IntKind::U64, bits: 3 },
                ..
            }
        ));
        assert!(n_eq_3, "expected n = s.len() to be Int {{ U64, 3 }}");
    }

    /// All four range forms parse + typecheck on the same Vec, each producing
    /// a slice with the expected length.
    #[test]
    fn run_pipeline_slice_all_forms() {
        let source = "fn main() {
            let mut v: Vec<i32> = Vec::new();
            v.push(1);
            v.push(2);
            v.push(3);
            let a = &v[..];
            let b = &v[1..];
            let c = &v[..2];
            let d = &v[0..2];
        }";
        let events = run_pipeline(source).expect("all four range forms compile");
        let slice_writes: Vec<u64> = events.iter().filter_map(|e| match e {
            crate::MemEvent::SlotWrite { value: crate::Value::Slice { len, .. }, .. } => Some(*len),
            _ => None,
        }).collect();
        assert_eq!(
            slice_writes,
            vec![3, 2, 2, 2],
            "expected slice lengths 3, 2, 2, 2 for &v[..], &v[1..], &v[..2], &v[0..2]"
        );
    }

    #[test]
    fn run_pipeline_slice_oob_start_gt_end() {
        let source = "fn main() {
            let mut v: Vec<i32> = Vec::new();
            v.push(1);
            v.push(2);
            let s = &v[2..1];
        }";
        let events = run_pipeline(source).expect("slice start>end compiles; halts");
        let has_inv = events.iter().any(|e| matches!(e,
            crate::MemEvent::Note { kind: crate::NoteKind::RuntimeError, message, .. }
                if message.contains("slice start > end")
        ));
        assert!(has_inv, "expected runtime error for inverted slice range");
    }
}
