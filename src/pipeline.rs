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
        // Expect at minimum: FrameEnter, SlotAlloc, SlotWrite, FrameLeave.
        // M07.2: ReturnValue(Unit) for implicit-unit returns is skipped
        // (no caller to flash; would produce a silent cursor step).
        assert!(events.len() >= 4);
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
        // M03.1 post-revision: 12 events.
        // M07.2: -1 because main's implicit-unit ReturnValue is now skipped.
        // `add` still emits ReturnValue(Int 5) because it has an explicit
        // tail expression returning a non-unit value.
        assert_eq!(events.len(), 11);
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

    // ─── M07.3 / US1: stack-allocated array literal + len() ───────────────

    /// `let t = [10, 20, 30]; let n = t.len();` — typechecks as
    /// `[i32; 3]`; SlotWrite carries Value::Array with 3 elements;
    /// n's SlotWrite has `Int { U64, 3 }`.
    #[test]
    fn run_pipeline_array_basic() {
        let source = "fn main() { let t = [10, 20, 30]; let n = t.len(); }";
        let events = run_pipeline(source).expect("array compiles");
        // SlotWrite of t with Value::Array of 3 elements.
        let array_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Array { elements, .. },
                ..
            } if elements.len() == 3
        ));
        assert!(array_write, "expected SlotWrite of Value::Array with 3 elements");
        // SlotWrite of n with Int(U64, 3).
        let n_eq_3 = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Int { kind: crate::typeck::IntKind::U64, bits: 3 },
                ..
            }
        ));
        assert!(n_eq_3, "expected n = 3_u64 from t.len()");
    }

    // ─── M07.3 / US2: array indexing ──────────────────────────────────────

    #[test]
    fn run_pipeline_array_index() {
        let source = "fn main() { let t = [10, 20, 30]; let x = t[1]; }";
        let events = run_pipeline(source).expect("array index compiles");
        let x_eq_20 = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Int { kind: crate::typeck::IntKind::I32, bits: 20 },
                ..
            }
        ));
        assert!(x_eq_20, "expected x = t[1] = 20_i32");
    }

    #[test]
    fn run_pipeline_array_index_oob() {
        let source = "fn main() { let t = [10, 20]; let x = t[5]; }";
        let events = run_pipeline(source).expect("OOB compiles; halts at runtime");
        let has_oob = events.iter().any(|e| matches!(e,
            crate::MemEvent::Note { kind: crate::NoteKind::RuntimeError, message, .. }
                if message.contains("array len is 2") && message.contains("index is 5")
        ));
        assert!(has_oob, "expected RuntimeError 'index out of bounds: array len is 2 but the index is 5'");
    }

    // ─── M07.3 / US3: array slicing (slot-target slice) ───────────────────

    /// `let t = [1, 2, 3, 4]; let s = &t[1..3];` — slicing an array
    /// produces a `Value::Slice` with `target: Pointee::Slot(t_slot)`.
    /// First scenario constructing the Slot variant on Value::Slice.
    #[test]
    fn run_pipeline_array_slice() {
        let source = "fn main() { let t = [1, 2, 3, 4]; let s = &t[1..3]; }";
        let events = run_pipeline(source).expect("array slice compiles");
        // s's SlotWrite carries Value::Slice with Slot target.
        let slot_slice = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Slice {
                    target: crate::event::Pointee::Slot(_),
                    start: 1, len: 2, byte_offset: 4, byte_len: 8, ..
                },
                ..
            }
        ));
        assert!(slot_slice, "expected SlotWrite of Value::Slice {{ target: Slot, len: 2, byte_offset: 4, byte_len: 8, .. }}");
        // No BorrowShared events for Slot-target slices (M07.2 pattern).
        let slot_borrows = events.iter().filter(|e| matches!(e,
            crate::MemEvent::BorrowShared { target: crate::event::Pointee::Slot(_), .. }
        )).count();
        assert_eq!(slot_borrows, 0, "Slot-target slice borrows skip BorrowShared (lazy materialization in UI)");
        // Zero heap events (array + slice both stack-resident).
        let heap_count = events.iter().filter(|e| matches!(
            e,
            crate::MemEvent::HeapAlloc { .. } | crate::MemEvent::HeapRealloc { .. } | crate::MemEvent::HeapFree { .. }
        )).count();
        assert_eq!(heap_count, 0, "array slicing must not emit any heap events");
    }

    #[test]
    fn run_pipeline_array_slice_oob() {
        let source = "fn main() { let t = [1, 2]; let s = &t[0..5]; }";
        let events = run_pipeline(source).expect("OOB slice compiles; halts");
        let has_oob = events.iter().any(|e| matches!(e,
            crate::MemEvent::Note { kind: crate::NoteKind::RuntimeError, message, .. }
                if message.contains("slice end out of bounds")
        ));
        assert!(has_oob, "expected RuntimeError for OOB array slice");
    }

    /// Element-type inference: an array literal's type is driven by the
    /// first explicitly-typed element. Untyped literals follow via
    /// literal-narrowing. `[10, 20, 30_u64]` should infer `[u64; 3]`,
    /// not error with "i32 != u64".
    #[test]
    fn run_pipeline_array_lit_type_inference() {
        let source = "fn main() { let t = [10, 20, 30_u64]; }";
        let events = run_pipeline(source).expect("array with suffix-driven inference compiles");
        // SlotWrite of t with Value::Array of 3 U64 elements.
        let ok = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Array { elements, elem_ty: crate::typeck::Ty::Int(crate::typeck::IntKind::U64) },
                ..
            } if elements.len() == 3
        ));
        assert!(ok, "expected Value::Array of 3 U64 elements (anchor = the suffixed literal)");
    }

    /// **Headline pedagogical assertion** for M07.3: array-only programs
    /// emit zero heap events. The heap panel stays empty.
    #[test]
    fn run_pipeline_array_no_heap() {
        let source = "fn main() { let t = [10, 20, 30]; let n = t.len(); }";
        let events = run_pipeline(source).expect("compiles");
        let heap_event_count = events.iter().filter(|e| matches!(
            e,
            crate::MemEvent::HeapAlloc { .. }
                | crate::MemEvent::HeapRealloc { .. }
                | crate::MemEvent::HeapFree { .. }
        )).count();
        assert_eq!(heap_event_count, 0, "array-only program must emit zero heap events");
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
        // **M07.2**: Pointee::Static borrows no longer emit BorrowShared
        // events (lifecycle is invisible — silent no-op cursor steps).
        // The arrow is materialized lazily at SlotWrite time. Verify via
        // the SlotWrite carrying Value::Slice with Pointee::Static target.
        let slice_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite { value: crate::Value::Slice { len: 4, target: crate::event::Pointee::Static(_), .. }, .. }
        ));
        assert!(slice_write, "expected SlotWrite of Value::Slice {{ len: 4, target: Pointee::Static(_), .. }}");
        // And zero BorrowShared events should be present.
        let shared_count = events.iter().filter(|e| matches!(e, crate::MemEvent::BorrowShared { .. })).count();
        assert_eq!(shared_count, 0, "M07.2: static-target borrows no longer emit BorrowShared");
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
        // M07.2: Pointee::Static borrows skip BorrowShared/BorrowEnd
        // entirely. Verify via SlotWrites for both s and s2.
        let slice_writes: Vec<u64> = events.iter().filter_map(|e| match e {
            crate::MemEvent::SlotWrite { value: crate::Value::Slice { target: crate::event::Pointee::Static(_), len, .. }, .. } => Some(*len),
            _ => None,
        }).collect();
        assert_eq!(slice_writes, vec![5, 2], "expected SlotWrites for s (len 5) and s2 (len 2)");
        // s2's specific shape: Value::Slice with byte_offset 0, byte_len 2.
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
        // M07.2: Pointee::Static borrows no longer emit BorrowShared events.
        // Verify dedup via SlotWrites instead — both `a` and `b` should
        // carry Value::Slice targeting the same StaticAddr.
        let slice_writes: Vec<crate::event::StaticAddr> = events.iter().filter_map(|e| match e {
            crate::MemEvent::SlotWrite { value: crate::Value::Slice { target: crate::event::Pointee::Static(addr), .. }, .. } => Some(*addr),
            _ => None,
        }).collect();
        assert_eq!(slice_writes.len(), 2, "expected SlotWrite for both a and b");
        assert_eq!(slice_writes[0], slice_writes[1], "both should reference the same deduped static addr");
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

    // ─── M07.4 / US1: struct decl + literal + field access ────────────────

    /// `struct Point { x: i32, y: i32 } let p = Point { x: 1, y: 2 };
    /// let a = p.x;` — typechecks; emits Value::Struct SlotWrite for p
    /// with field order matching the declaration; a's SlotWrite has
    /// Int(I32, 1); zero heap events.
    #[test]
    fn run_pipeline_struct_basic() {
        let source = "struct Point { x: i32, y: i32 }\nfn main() {\n    let p = Point { x: 1, y: 2 };\n    let a = p.x;\n}\n";
        let events = run_pipeline(source).expect("struct compiles");
        // SlotWrite of p with Value::Struct { name: "Point", fields: [("x", 1), ("y", 2)] }.
        let p_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Struct { name, fields },
                ..
            } if name == "Point"
                && fields.len() == 2
                && matches!(&fields[0], (fname, crate::Value::Int { kind: crate::typeck::IntKind::I32, bits: 1 }) if fname == "x")
                && matches!(&fields[1], (fname, crate::Value::Int { kind: crate::typeck::IntKind::I32, bits: 2 }) if fname == "y")
        ));
        assert!(p_write, "expected SlotWrite of Value::Struct {{ Point, [(x,1),(y,2)] }}");
        // SlotWrite of a with Int(I32, 1).
        let a_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Int { kind: crate::typeck::IntKind::I32, bits: 1 },
                ..
            }
        ));
        assert!(a_write, "expected a = p.x = 1_i32");
        // Zero heap events.
        let heap_count = events.iter().filter(|e| matches!(
            e,
            crate::MemEvent::HeapAlloc { .. } | crate::MemEvent::HeapRealloc { .. } | crate::MemEvent::HeapFree { .. }
        )).count();
        assert_eq!(heap_count, 0, "structs are stack-only — no heap events expected");
    }

    /// Field-shorthand: `let x = 1; let y = 2; let p = Point { x, y };`.
    /// Resolves each shorthand to the bound local; constructed struct
    /// identical to the full-form literal.
    #[test]
    fn run_pipeline_struct_shorthand() {
        let source = "struct Point { x: i32, y: i32 }\nfn main() {\n    let x = 1;\n    let y = 2;\n    let p = Point { x, y };\n}\n";
        let events = run_pipeline(source).expect("shorthand compiles");
        let p_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Struct { name, fields },
                ..
            } if name == "Point"
                && matches!(&fields[0], (n, crate::Value::Int { bits: 1, .. }) if n == "x")
                && matches!(&fields[1], (n, crate::Value::Int { bits: 2, .. }) if n == "y")
        ));
        assert!(p_write, "expected Point {{ x: 1, y: 2 }} via shorthand");
    }

    /// Missing field in struct literal — typeck error.
    #[test]
    fn run_pipeline_struct_missing_field() {
        let source = "struct Point { x: i32, y: i32 }\nfn main() {\n    let p = Point { x: 1 };\n}\n";
        let err = run_pipeline(source).expect_err("missing field should fail typeck");
        assert_eq!(err.stage, CompileStage::Typeck);
        assert!(
            err.message.contains("missing field `y`"),
            "error mentions the missing field: got `{}`",
            err.message
        );
    }

    /// Extra field in struct literal — typeck error.
    #[test]
    fn run_pipeline_struct_extra_field() {
        let source = "struct Point { x: i32, y: i32 }\nfn main() {\n    let p = Point { x: 1, y: 2, z: 3 };\n}\n";
        let err = run_pipeline(source).expect_err("extra field should fail typeck");
        assert_eq!(err.stage, CompileStage::Typeck);
        assert!(
            err.message.contains("no field `z`"),
            "error mentions the unknown field: got `{}`",
            err.message
        );
    }

    /// Mixed primitive field types — integer literal coerces to f64 field.
    /// Regression: an early M07.4 build emitted `Value::Int { I32, 2 }`
    /// for `y: 2` because eval's `LitInt` arm only honored coerced
    /// `Ty::Int` types and fell through to default I32 for `Ty::Float`.
    /// The UI showed `y: i32` for an f64 field as a result.
    #[test]
    fn run_pipeline_struct_int_to_float_coercion() {
        let source = "struct Point { x: i32, y: f64 }\nfn main() {\n    let p = Point { x: 1, y: 2 };\n}\n";
        let events = run_pipeline(source).expect("mixed-type struct compiles");
        let p_write = events.iter().find_map(|e| match e {
            crate::MemEvent::SlotWrite {
                value: crate::Value::Struct { name, fields },
                ..
            } if name == "Point" => Some(fields.clone()),
            _ => None,
        });
        let fields = p_write.expect("expected Value::Struct for Point");
        assert!(
            matches!(&fields[0], (n, crate::Value::Int { kind: crate::typeck::IntKind::I32, bits: 1 }) if n == "x"),
            "x should be i32 = 1, got {:?}", fields[0],
        );
        assert!(
            matches!(&fields[1], (n, crate::Value::Float { kind: crate::typeck::FloatKind::F64, value }) if n == "y" && *value == 2.0),
            "y should be f64 = 2.0 (coerced from int literal), got {:?}", fields[1],
        );
    }

    /// Wrong-type field value — typeck error pointing at the value.
    #[test]
    fn run_pipeline_struct_wrong_type() {
        let source = "struct Point { x: i32, y: i32 }\nfn main() {\n    let p = Point { x: true, y: 2 };\n}\n";
        let err = run_pipeline(source).expect_err("wrong-type field should fail typeck");
        assert_eq!(err.stage, CompileStage::Typeck);
        assert!(
            err.message.contains("expected `i32`") && err.message.contains("found `bool`"),
            "error mentions the type mismatch: got `{}`",
            err.message
        );
    }

    // ─── M07.4 / US2: field borrow + per-field metadata ─────────────────

    /// `let r = &p.x;` — emits Value::Ref { field_path: ["x"],
    /// target: Pointee::Slot(p_slot) } in r's SlotWrite, AND skips
    /// BorrowShared emission (slot-target field borrows use lazy
    /// materialization per M07.3 pattern).
    #[test]
    fn run_pipeline_field_borrow() {
        let source = "struct Point { x: i32, y: i32 }\nfn main() {\n    let p = Point { x: 1, y: 2 };\n    let r = &p.x;\n}\n";
        let events = run_pipeline(source).expect("field borrow compiles");
        let ref_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Ref {
                    target: crate::event::Pointee::Slot(_),
                    mutable: false,
                    field_path,
                    ..
                },
                ..
            } if field_path.len() == 1 && field_path[0] == "x"
        ));
        assert!(ref_write, "expected SlotWrite of Value::Ref with field_path=[\"x\"]");
        // Lazy materialization: no BorrowShared event for this borrow.
        let bs_count = events.iter().filter(|e| matches!(
            e,
            crate::MemEvent::BorrowShared { target: crate::event::Pointee::Slot(_), .. }
        )).count();
        assert_eq!(
            bs_count, 0,
            "field-borrow lifecycle is invisible — no BorrowShared event expected"
        );
    }

    // ─── M07.4 / US3: method dispatch ───────────────────────────────────

    /// `impl Point { fn x(&self) -> i32 { self.x } } let v = p.x();` —
    /// method dispatches into a new frame; self is bound to a Value::Ref
    /// pointing at p's slot; self.x reads through the ref to find the
    /// struct field; ReturnValue carries Int(I32, 1); v gets 1_i32.
    #[test]
    fn run_pipeline_method() {
        let source = "struct Point { x: i32, y: i32 }\nimpl Point { fn x(&self) -> i32 { self.x } }\nfn main() {\n    let p = Point { x: 1, y: 2 };\n    let v = p.x();\n}\n";
        let events = run_pipeline(source).expect("method compiles");
        // FrameEnter for Point::x.
        let frame_enter = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "Point::x"
        ));
        assert!(frame_enter, "expected FrameEnter for `Point::x`");
        // self SlotWrite with Value::Ref → Pointee::Slot.
        let self_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Ref {
                    target: crate::event::Pointee::Slot(_),
                    mutable: false,
                    field_path,
                    ..
                },
                ..
            } if field_path.is_empty()
        ));
        assert!(self_write, "expected SlotWrite for self with Value::Ref {{ target: Slot, .. }}");
        // ReturnValue Int(I32, 1).
        let return_value = events.iter().any(|e| matches!(e,
            crate::MemEvent::ReturnValue {
                value: crate::Value::Int { kind: crate::typeck::IntKind::I32, bits: 1 },
                ..
            }
        ));
        assert!(return_value, "expected ReturnValue Int(I32, 1) from Point::x");
        // v's SlotWrite carries Int(I32, 1).
        let v_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Int { kind: crate::typeck::IntKind::I32, bits: 1 },
                ..
            }
        ));
        assert!(v_write, "expected v = p.x() = 1_i32");
    }

    /// Two methods in one impl block — dispatch correctly resolves each.
    #[test]
    fn run_pipeline_method_two_methods() {
        let source = "struct Point { x: i32, y: i32 }\nimpl Point {\n    fn x(&self) -> i32 { self.x }\n    fn dist(&self) -> i32 { self.x }\n}\nfn main() {\n    let p = Point { x: 7, y: 9 };\n    let v = p.dist();\n}\n";
        let events = run_pipeline(source).expect("two-method impl compiles");
        let dist_frame = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "Point::dist"
        ));
        assert!(dist_frame, "expected FrameEnter for Point::dist (not Point::x)");
        let v_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Int { kind: crate::typeck::IntKind::I32, bits: 7 },
                ..
            }
        ));
        assert!(v_write, "expected v = p.dist() = 7_i32");
    }

    /// Unknown method — typeck error.
    #[test]
    fn run_pipeline_method_unknown() {
        let source = "struct Point { x: i32, y: i32 }\nfn main() {\n    let p = Point { x: 1, y: 2 };\n    let v = p.bogus();\n}\n";
        let err = run_pipeline(source).expect_err("unknown method should fail typeck");
        assert_eq!(err.stage, CompileStage::Typeck);
        assert!(
            err.message.contains("no method `bogus`"),
            "error mentions the unknown method: got `{}`",
            err.message
        );
    }

    /// `&p.z` where Point has no field `z` — typeck error.
    #[test]
    fn run_pipeline_field_borrow_unknown() {
        let source = "struct Point { x: i32, y: i32 }\nfn main() {\n    let p = Point { x: 1, y: 2 };\n    let r = &p.z;\n}\n";
        let err = run_pipeline(source).expect_err("unknown field should fail typeck");
        assert_eq!(err.stage, CompileStage::Typeck);
        assert!(
            err.message.contains("no field `z`"),
            "error mentions the unknown field: got `{}`",
            err.message
        );
    }

    // ─── M07.4 / US4: associated function ────────────────────────────────

    /// `impl Point { fn new(x: i32, y: i32) -> Point { Point { x, y } } }
    /// let p = Point::new(1, 2);` — path call dispatches to the user
    /// assoc fn; new frame opens with x=1, y=2 params (NO self); body
    /// constructs Point { x, y } via shorthand; returns the struct; p
    /// lands it.
    #[test]
    fn run_pipeline_assoc_fn() {
        let source = "struct Point { x: i32, y: i32 }\nimpl Point { fn new(x: i32, y: i32) -> Point { Point { x, y } } }\nfn main() {\n    let p = Point::new(1, 2);\n}\n";
        let events = run_pipeline(source).expect("assoc fn compiles");
        let frame = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "Point::new"
        ));
        assert!(frame, "expected FrameEnter for `Point::new`");
        // ReturnValue carries the constructed Value::Struct.
        let rv = events.iter().any(|e| matches!(e,
            crate::MemEvent::ReturnValue {
                value: crate::Value::Struct { name, fields },
                ..
            } if name == "Point"
                && matches!(&fields[0], (n, crate::Value::Int { bits: 1, .. }) if n == "x")
                && matches!(&fields[1], (n, crate::Value::Int { bits: 2, .. }) if n == "y")
        ));
        assert!(rv, "expected ReturnValue of constructed Point");
        // p's SlotWrite carries that struct.
        let p_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Struct { name, fields },
                ..
            } if name == "Point" && fields.len() == 2
        ));
        assert!(p_write, "expected SlotWrite for p with Value::Struct(Point)");
    }

    /// Mixed dispatch: `Vec::new` (builtin) + `Point::new` (user) both
    /// resolve correctly.
    #[test]
    fn run_pipeline_assoc_fn_mixed() {
        let source = "struct Point { x: i32, y: i32 }\nimpl Point { fn new(x: i32, y: i32) -> Point { Point { x, y } } }\nfn main() {\n    let v: Vec<i32> = Vec::new();\n    let p = Point::new(3, 4);\n}\n";
        let events = run_pipeline(source).expect("mixed dispatch compiles");
        // Vec::new emits HeapAlloc.
        let heap = events.iter().any(|e| matches!(e, crate::MemEvent::HeapAlloc { .. }));
        assert!(heap, "expected HeapAlloc from Vec::new (builtin)");
        // Point::new emits FrameEnter.
        let frame = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "Point::new"
        ));
        assert!(frame, "expected FrameEnter for Point::new (user assoc fn)");
    }

    /// Forward reference: `impl Point` AT TOP precedes `struct Point` —
    /// 2-pass typeck makes this work since phase 1 collects all schemas
    /// + impls before phase 2 typechecks bodies.
    #[test]
    fn run_pipeline_struct_forward_ref() {
        let source = "impl Point { fn new(x: i32, y: i32) -> Point { Point { x, y } } }\nstruct Point { x: i32, y: i32 }\nfn main() {\n    let p = Point::new(5, 6);\n}\n";
        let events = run_pipeline(source).expect("forward reference compiles via two-pass typeck");
        let p_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Struct { name, .. },
                ..
            } if name == "Point"
        ));
        assert!(p_write, "expected p = Point::new(5, 6) → Value::Struct(Point)");
    }

    // ─── M07.4 / T052: field assignment ─────────────────────────────────

    /// `let mut p = Point { x: 1, y: 2 }; p.x = 5;` — mutates p's struct
    /// in place. Emits a SlotWrite with the WHOLE updated Value::Struct
    /// (x=5, y=2). After the write, p.x reads as 5.
    #[test]
    fn run_pipeline_struct_field_assign() {
        let source = "struct Point { x: i32, y: i32 }\nfn main() {\n    let mut p = Point { x: 1, y: 2 };\n    p.x = 5;\n    let a = p.x;\n}\n";
        let events = run_pipeline(source).expect("field assign compiles");
        // Two SlotWrites for p: first with x=1,y=2; second with x=5,y=2.
        let p_writes: Vec<_> = events.iter().filter_map(|e| match e {
            crate::MemEvent::SlotWrite {
                value: crate::Value::Struct { name, fields },
                ..
            } if name == "Point" => Some(fields.clone()),
            _ => None,
        }).collect();
        assert_eq!(p_writes.len(), 2, "expected 2 SlotWrites for p (init + assign)");
        // Second write has x=5, y=2.
        let second = &p_writes[1];
        assert!(
            matches!(&second[0], (n, crate::Value::Int { bits: 5, .. }) if n == "x"),
            "second SlotWrite should have x=5"
        );
        assert!(
            matches!(&second[1], (n, crate::Value::Int { bits: 2, .. }) if n == "y"),
            "second SlotWrite should keep y=2"
        );
        // a = p.x reads back as 5.
        let a_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Int { bits: 5, .. },
                ..
            }
        ));
        assert!(a_write, "expected a = p.x = 5_i32 after the assignment");
    }

    /// Field assignment to an immutable struct binding — typeck error.
    #[test]
    fn run_pipeline_struct_field_assign_immutable() {
        let source = "struct Point { x: i32, y: i32 }\nfn main() {\n    let p = Point { x: 1, y: 2 };\n    p.x = 5;\n}\n";
        let err = run_pipeline(source).expect_err("immutable field assign should fail");
        assert_eq!(err.stage, CompileStage::Typeck);
        assert!(
            err.message.contains("immutable"),
            "error mentions immutability: got `{}`",
            err.message
        );
    }

    /// Struct-only programs emit ZERO heap events (stack-only pedagogy).
    #[test]
    fn run_pipeline_struct_no_heap() {
        let source = "struct Point { x: i32, y: i32 }\nimpl Point { fn new(x: i32, y: i32) -> Point { Point { x, y } } fn x(&self) -> i32 { self.x } }\nfn main() {\n    let p = Point::new(1, 2);\n    let v = p.x();\n}\n";
        let events = run_pipeline(source).expect("struct-only compiles");
        let heap_count = events.iter().filter(|e| matches!(
            e,
            crate::MemEvent::HeapAlloc { .. }
                | crate::MemEvent::HeapRealloc { .. }
                | crate::MemEvent::HeapFree { .. }
        )).count();
        assert_eq!(heap_count, 0, "struct-only programs must emit zero heap events");
    }

    // ─── M07.5 / US1: generic identity fn with monomorphization ─────────

    /// `fn id<T>(x: T) -> T { x } let a = id(5); let b = id(true);` —
    /// monomorphization-visible pedagogy. Two distinct FrameEnter events
    /// with mangled fn_name (`id::<i32>` and `id::<bool>`). Param + return
    /// types carry the substituted concrete types.
    #[test]
    fn run_pipeline_generic_id_fn() {
        let source = "fn id<T>(x: T) -> T {\n    x\n}\nfn main() {\n    let a = id(5);\n    let b = id(true);\n}\n";
        let events = run_pipeline(source).expect("generic id fn compiles");
        let id_i32 = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "id::<i32>"
        ));
        let id_bool = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "id::<bool>"
        ));
        assert!(id_i32, "expected FrameEnter `id::<i32>`");
        assert!(id_bool, "expected FrameEnter `id::<bool>`");
        // Param x slot carries concrete (substituted) types per call.
        let x_i32 = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotAlloc { name, ty: crate::Ty::Int(crate::typeck::IntKind::I32), .. } if name == "x"
        ));
        let x_bool = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotAlloc { name, ty: crate::Ty::Bool, .. } if name == "x"
        ));
        assert!(x_i32, "expected SlotAlloc `x : i32` in id::<i32>");
        assert!(x_bool, "expected SlotAlloc `x : bool` in id::<bool>");
    }

    /// `fn pair<T>(a: T, b: T) -> T { a } let _ = pair(5, true);` —
    /// typeck error: cannot infer T from conflicting args.
    #[test]
    fn run_pipeline_generic_inference_mismatch() {
        let source = "fn pair<T>(a: T, b: T) -> T {\n    a\n}\nfn main() {\n    let _ = pair(5, true);\n}\n";
        let err = run_pipeline(source).expect_err("inference mismatch should fail typeck");
        assert_eq!(err.stage, CompileStage::Typeck);
        assert!(
            err.message.contains("cannot infer") && err.message.contains("conflicting"),
            "expected 'cannot infer ... conflicting' error, got: `{}`",
            err.message
        );
    }

    // ─── M07.5 / US2: generic struct ─────────────────────────────────────

    /// `struct Wrapper<T> { v: T } let w = Wrapper { v: 5 }; let a = w.v;`
    /// — w renders as `Wrapper<i32>` (substituted), field access yields 5_i32.
    #[test]
    fn run_pipeline_generic_struct() {
        let source = "struct Wrapper<T> {\n    v: T,\n}\nfn main() {\n    let w = Wrapper { v: 5 };\n    let a = w.v;\n}\n";
        let events = run_pipeline(source).expect("generic struct compiles");
        // SlotWrite for w carries Value::Struct { name: "Wrapper", fields: [("v", Int{I32, 5})] }.
        let w_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Struct { name, fields },
                ..
            } if name == "Wrapper"
                && matches!(&fields[0], (n, crate::Value::Int { bits: 5, .. }) if n == "v")
        ));
        assert!(w_write, "expected SlotWrite of Value::Struct(Wrapper, [(v, 5)])");
        // SlotAlloc for w carries Ty::Struct { name: "Wrapper", type_args: [Int(I32)], .. }.
        let w_alloc = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotAlloc {
                name,
                ty: crate::Ty::Struct { name: tn, type_args, .. },
                ..
            } if name == "w" && tn == "Wrapper" && type_args.len() == 1
                && matches!(&type_args[0], crate::Ty::Int(crate::typeck::IntKind::I32))
        ));
        assert!(w_alloc, "expected SlotAlloc with type_args [Int(I32)]");
        // a = w.v = 5_i32.
        let a_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Int { kind: crate::typeck::IntKind::I32, bits: 5 },
                ..
            }
        ));
        assert!(a_write, "expected a = 5_i32");
    }

    /// Two instantiations of the same generic struct produce distinct
    /// substituted Tys (Wrapper<i32> vs Wrapper<bool>).
    #[test]
    fn run_pipeline_generic_struct_two_instantiations() {
        let source = "struct Wrapper<T> {\n    v: T,\n}\nfn main() {\n    let w1 = Wrapper { v: 5 };\n    let w2 = Wrapper { v: true };\n}\n";
        let events = run_pipeline(source).expect("compiles");
        let i32_alloc = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotAlloc {
                ty: crate::Ty::Struct { name, type_args, .. },
                ..
            } if name == "Wrapper" && matches!(type_args.first(), Some(crate::Ty::Int(crate::typeck::IntKind::I32)))
        ));
        let bool_alloc = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotAlloc {
                ty: crate::Ty::Struct { name, type_args, .. },
                ..
            } if name == "Wrapper" && matches!(type_args.first(), Some(crate::Ty::Bool))
        ));
        assert!(i32_alloc, "expected Wrapper<i32> SlotAlloc");
        assert!(bool_alloc, "expected Wrapper<bool> SlotAlloc");
    }

    // ─── M07.5 / US3: turbofish call ─────────────────────────────────────

    /// `let v = id::<bool>(false);` — explicit type-arg pins T=bool;
    /// frame labeled `id::<bool>`; v's SlotWrite carries Value::Bool(false).
    #[test]
    fn run_pipeline_turbofish() {
        let source = "fn id<T>(x: T) -> T {\n    x\n}\nfn main() {\n    let v = id::<bool>(false);\n}\n";
        let events = run_pipeline(source).expect("turbofish compiles");
        let frame = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "id::<bool>"
        ));
        assert!(frame, "expected FrameEnter `id::<bool>`");
        let v_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite { value: crate::Value::Bool(false), .. }
        ));
        assert!(v_write, "expected v = false");
    }

    /// `let v = id::<bool>(5);` — turbofish pins T=bool; arg is i32 →
    /// typeck error.
    #[test]
    fn run_pipeline_turbofish_type_mismatch() {
        let source = "fn id<T>(x: T) -> T {\n    x\n}\nfn main() {\n    let v = id::<bool>(5);\n}\n";
        let err = run_pipeline(source).expect_err("turbofish mismatch should fail");
        assert_eq!(err.stage, CompileStage::Typeck);
        assert!(
            err.message.contains("expected `bool`") && err.message.contains("found `i32`"),
            "expected type-mismatch error mentioning bool + i32, got: `{}`",
            err.message
        );
    }

    // ─── M07.5 / rejection tests (out-of-scope shapes) ──────────────────

    /// `fn pair<T, U>(...)` — typeck error: M07.5 supports single type-param.
    #[test]
    fn run_pipeline_generic_multi_param_rejected() {
        let source = "fn pair<T, U>(a: T, b: U) -> T {\n    a\n}\nfn main() {\n    let _ = pair(5, true);\n}\n";
        let err = run_pipeline(source).expect_err("multi-T should fail");
        assert_eq!(err.stage, CompileStage::Typeck);
        assert!(
            err.message.contains("single type parameter"),
            "expected single-param restriction error, got: `{}`",
            err.message
        );
    }

    /// `fn id<T: Foo>(...)` — typeck error: `Foo` is not a registered trait.
    /// **M07.6 update**: this test was written in M07.5 expecting the
    /// "bounds deferred to M07.6" pointer. Now that M07.6 ships, bounds
    /// are accepted syntactically; this test asserts the unknown-trait
    /// rejection instead.
    #[test]
    fn run_pipeline_generic_bound_rejected() {
        let source = "fn id<T: Foo>(x: T) -> T {\n    x\n}\nfn main() {\n    let _ = id(5);\n}\n";
        let err = run_pipeline(source).expect_err("bound on unknown trait should fail");
        assert_eq!(err.stage, CompileStage::Typeck);
        assert!(
            err.message.contains("Foo") && err.message.contains("trait"),
            "expected unknown-trait error mentioning `Foo`, got: `{}`",
            err.message
        );
    }

    /// `fn outer<T>(x: T) -> T { id::<T>(x) }` — nested generic call rejected.
    #[test]
    fn run_pipeline_generic_nested_call_rejected() {
        let source = "fn id<T>(x: T) -> T {\n    x\n}\nfn outer<T>(x: T) -> T {\n    id::<T>(x)\n}\nfn main() {\n    let _ = outer(5);\n}\n";
        let err = run_pipeline(source).expect_err("nested generic call should fail");
        assert_eq!(err.stage, CompileStage::Typeck);
        assert!(
            err.message.contains("out of scope in M07.5") || err.message.contains("inside another generic"),
            "expected nested-call error, got: `{}`",
            err.message
        );
    }

    // ─── M07.6 / US1: trait decl + impl + dispatch ────────────────────────

    /// `trait Show { fn show(&self) -> i32; } impl Show for Point { ... }
    /// let s = p.show();` — trace contains FrameEnter for
    /// `<Point as Show>::show`; s = 1_i32.
    #[test]
    fn run_pipeline_trait_basic() {
        let source = "struct Point { x: i32, y: i32 }\ntrait Show { fn show(&self) -> i32; }\nimpl Show for Point { fn show(&self) -> i32 { self.x } }\nfn main() { let p = Point { x: 1, y: 2 }; let s = p.show(); }\n";
        let events = run_pipeline(source).expect("trait basic compiles");
        let frame = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "<Point as Show>::show"
        ));
        assert!(frame, "expected FrameEnter `<Point as Show>::show`");
        let s_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Int { kind: crate::typeck::IntKind::I32, bits: 1 },
                ..
            }
        ));
        assert!(s_write, "expected s = 1_i32");
    }

    /// Impl missing a required method → typeck error.
    #[test]
    fn run_pipeline_trait_missing_method() {
        let source = "struct Point { x: i32, y: i32 }\ntrait Show { fn show(&self) -> i32; }\nimpl Show for Point {}\nfn main() {}\n";
        let err = run_pipeline(source).expect_err("missing required method should fail");
        assert_eq!(err.stage, CompileStage::Typeck);
        assert!(
            err.message.contains("missing implementation") && err.message.contains("show"),
            "expected missing-method error: got `{}`",
            err.message
        );
    }

    /// Impl with a method not on the trait → typeck error.
    #[test]
    fn run_pipeline_trait_extra_method() {
        let source = "struct Point { x: i32, y: i32 }\ntrait Show { fn show(&self) -> i32; }\nimpl Show for Point { fn show(&self) -> i32 { self.x } fn other(&self) -> i32 { 0 } }\nfn main() {}\n";
        let err = run_pipeline(source).expect_err("extra method should fail");
        assert_eq!(err.stage, CompileStage::Typeck);
        assert!(
            err.message.contains("not on trait") && err.message.contains("other"),
            "expected extra-method error: got `{}`",
            err.message
        );
    }

    // ─── M07.6 / US2: default method ──────────────────────────────────────

    /// Default method dispatch: impl provides only `count`; calling
    /// `p.double()` routes to the trait's default body, which calls
    /// `self.count()` (the impl override). Result = 2 * p.x.
    #[test]
    fn run_pipeline_default_method() {
        let source = "struct Point { x: i32, y: i32 }\ntrait Counter { fn count(&self) -> i32; fn double(&self) -> i32 { self.count() * 2 } }\nimpl Counter for Point { fn count(&self) -> i32 { self.x } }\nfn main() { let p = Point { x: 1, y: 2 }; let v = p.double(); }\n";
        let events = run_pipeline(source).expect("default method compiles");
        let outer = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "<Point as Counter>::double"
        ));
        let inner = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "<Point as Counter>::count"
        ));
        assert!(outer, "expected outer FrameEnter `<Point as Counter>::double`");
        assert!(inner, "expected nested FrameEnter `<Point as Counter>::count`");
        let v_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Int { kind: crate::typeck::IntKind::I32, bits: 2 },
                ..
            }
        ));
        assert!(v_write, "expected v = 2_i32 (1 * 2)");
    }

    // ─── M07.6 / US3: generic bound (THE HEADLINE) ────────────────────────

    /// `fn print<T: Show>(x: T) -> i32 { x.show() } let r = print(p);` —
    /// bound proves the call; nested frames `print::<Point>` and
    /// `<Point as Show>::show`; r = p.x.
    #[test]
    fn run_pipeline_generic_bound() {
        let source = "struct Point { x: i32, y: i32 }\ntrait Show { fn show(&self) -> i32; }\nimpl Show for Point { fn show(&self) -> i32 { self.x } }\nfn print<T: Show>(x: T) -> i32 { x.show() }\nfn main() { let p = Point { x: 1, y: 2 }; let r = print(p); }\n";
        let events = run_pipeline(source).expect("generic bound compiles");
        let outer = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "print::<Point>"
        ));
        let inner = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "<Point as Show>::show"
        ));
        assert!(outer, "expected outer FrameEnter `print::<Point>`");
        assert!(inner, "expected nested FrameEnter `<Point as Show>::show`");
    }

    /// Bound not satisfied — `print(5)` where i32: Show absent.
    #[test]
    fn run_pipeline_trait_bound_unsatisfied() {
        let source = "struct Point { x: i32, y: i32 }\ntrait Show { fn show(&self) -> i32; }\nimpl Show for Point { fn show(&self) -> i32 { self.x } }\nfn print<T: Show>(x: T) -> i32 { x.show() }\nfn main() { let r = print(5); }\n";
        let err = run_pipeline(source).expect_err("bound not satisfied should fail");
        assert_eq!(err.stage, CompileStage::Typeck);
        assert!(
            err.message.contains("trait bound") && err.message.contains("i32") && err.message.contains("Show"),
            "expected bound-not-satisfied error: got `{}`",
            err.message
        );
    }

    // ─── M07.6 / US4: multi-bound ─────────────────────────────────────────

    /// `fn show_n_count<T: Show + Counter>(x: T) -> i32 { x.show() + x.count() }`
    /// — both bounds active; nested dispatches; r = 1 + 2 = 3.
    #[test]
    fn run_pipeline_multi_bound() {
        let source = "struct Point { x: i32, y: i32 }\ntrait Show { fn show(&self) -> i32; }\ntrait Counter { fn count(&self) -> i32; }\nimpl Show for Point { fn show(&self) -> i32 { self.x } }\nimpl Counter for Point { fn count(&self) -> i32 { self.y } }\nfn show_n_count<T: Show + Counter>(x: T) -> i32 { x.show() + x.count() }\nfn main() { let p = Point { x: 1, y: 2 }; let r = show_n_count(p); }\n";
        let events = run_pipeline(source).expect("multi-bound compiles");
        let outer = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "show_n_count::<Point>"
        ));
        let show_frame = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "<Point as Show>::show"
        ));
        let count_frame = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "<Point as Counter>::count"
        ));
        assert!(outer, "expected outer FrameEnter `show_n_count::<Point>`");
        assert!(show_frame, "expected nested FrameEnter `<Point as Show>::show`");
        assert!(count_frame, "expected nested FrameEnter `<Point as Counter>::count`");
        let r_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Int { kind: crate::typeck::IntKind::I32, bits: 3 },
                ..
            }
        ));
        assert!(r_write, "expected r = 3_i32 (1 + 2)");
    }

    /// Method-name ambiguity in multi-bound → typeck error suggesting UFCS.
    #[test]
    fn run_pipeline_trait_method_ambiguous() {
        let source = "struct Point { x: i32, y: i32 }\ntrait A { fn name(&self) -> i32; }\ntrait B { fn name(&self) -> i32; }\nimpl A for Point { fn name(&self) -> i32 { self.x } }\nimpl B for Point { fn name(&self) -> i32 { self.y } }\nfn foo<T: A + B>(x: T) -> i32 { x.name() }\nfn main() { let p = Point { x: 1, y: 2 }; let _ = foo(p); }\n";
        let err = run_pipeline(source).expect_err("ambiguous method should fail");
        assert_eq!(err.stage, CompileStage::Typeck);
        assert!(
            err.message.contains("ambiguous") && err.message.contains("UFCS"),
            "expected ambiguous-method error suggesting UFCS: got `{}`",
            err.message
        );
    }

    // ─── M07.6 / cross-cutting rejection tests ────────────────────────────

    /// Inherent dispatch wins over trait when both define `show`.
    #[test]
    fn run_pipeline_trait_inherent_wins() {
        let source = "struct Point { x: i32, y: i32 }\ntrait Show { fn show(&self) -> i32; }\nimpl Point { fn show(&self) -> i32 { 42 } }\nimpl Show for Point { fn show(&self) -> i32 { self.x } }\nfn main() { let p = Point { x: 1, y: 2 }; let v = p.show(); }\n";
        let events = run_pipeline(source).expect("inherent + trait compiles");
        // Inherent frame (M07.4 format), NOT trait `<as>` form.
        let inherent_frame = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "Point::show"
        ));
        let trait_frame = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "<Point as Show>::show"
        ));
        assert!(inherent_frame, "expected inherent frame `Point::show`");
        assert!(!trait_frame, "should NOT see trait frame (inherent wins)");
        // Value = 42 (inherent body), not p.x.
        let v_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Int { kind: crate::typeck::IntKind::I32, bits: 42 },
                ..
            }
        ));
        assert!(v_write, "expected v = 42 (inherent), not 1 (trait)");
    }

    /// Duplicate trait declaration → typeck error.
    #[test]
    fn run_pipeline_trait_duplicate_decl() {
        let source = "trait Show { fn show(&self) -> i32; }\ntrait Show { fn show(&self) -> i32; }\nfn main() {}\n";
        let err = run_pipeline(source).expect_err("duplicate trait should fail");
        assert_eq!(err.stage, CompileStage::Typeck);
        assert!(
            err.message.contains("already defined") && err.message.contains("Show"),
            "expected duplicate-trait error: got `{}`",
            err.message
        );
    }

    /// Duplicate trait impl for (trait, type) pair → typeck error.
    #[test]
    fn run_pipeline_trait_duplicate_impl() {
        let source = "struct Point { x: i32, y: i32 }\ntrait Show { fn show(&self) -> i32; }\nimpl Show for Point { fn show(&self) -> i32 { self.x } }\nimpl Show for Point { fn show(&self) -> i32 { self.y } }\nfn main() {}\n";
        let err = run_pipeline(source).expect_err("duplicate impl should fail");
        assert_eq!(err.stage, CompileStage::Typeck);
        assert!(
            err.message.contains("duplicate") && err.message.contains("Show"),
            "expected duplicate-impl error: got `{}`",
            err.message
        );
    }

    // ─── M07.7 / US1: basic `&dyn Trait` borrow + method dispatch ─────────

    /// `let d: &dyn Show = &p; let s = d.show();` — vtable allocated once,
    /// d slotted with Value::DynRef, dispatch produces a `<Point as Show>::show`
    /// frame, s = 1_i32.
    #[test]
    fn run_pipeline_dyn_basic() {
        let source = "struct Point { x: i32, y: i32 }\ntrait Show { fn show(&self) -> i32; }\nimpl Show for Point { fn show(&self) -> i32 { self.x } }\nfn main() { let p = Point { x: 1, y: 2 }; let d: &dyn Show = &p; let s = d.show(); }\n";
        let events = run_pipeline(source).expect("dyn basic compiles");
        let vtable_alloc = events.iter().filter(|e| matches!(e,
            crate::MemEvent::VtableAlloc { trait_name, type_name, .. }
                if trait_name == "Show" && type_name == "Point"
        )).count();
        assert_eq!(vtable_alloc, 1, "expected exactly one VtableAlloc for (Show, Point)");
        let d_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::DynRef { trait_name, .. },
                ..
            } if trait_name == "Show"
        ));
        assert!(d_write, "expected SlotWrite of Value::DynRef with trait_name=Show");
        let frame = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "<Point as Show>::show"
        ));
        assert!(frame, "expected FrameEnter `<Point as Show>::show`");
        let s_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Int { kind: crate::typeck::IntKind::I32, bits: 1 },
                ..
            }
        ));
        assert!(s_write, "expected s = 1_i32");
    }

    /// Coercion error: `&i32 → &dyn Show` rejected because i32: Show is not implemented.
    #[test]
    fn run_pipeline_dyn_coercion_error() {
        let source = "trait Show { fn show(&self) -> i32; }\nfn main() { let n = 5_i32; let d: &dyn Show = &n; let _ = d.show(); }\n";
        let err = run_pipeline(source).expect_err("&i32 → &dyn Show should be rejected");
        assert_eq!(err.stage, CompileStage::Typeck);
        assert!(
            err.message.contains("i32") && err.message.contains("Show"),
            "expected coercion-error mentioning i32 and Show: got `{}`",
            err.message
        );
    }

    /// Inherent method called through `&dyn Trait` rejected — trait objects
    /// only expose trait methods.
    #[test]
    fn run_pipeline_dyn_inherent_rejected() {
        let source = "struct Point { x: i32, y: i32 }\ntrait Show { fn show(&self) -> i32; }\nimpl Point { fn extra(&self) -> i32 { 42 } }\nimpl Show for Point { fn show(&self) -> i32 { self.x } }\nfn main() { let p = Point { x: 1, y: 2 }; let d: &dyn Show = &p; let _ = d.extra(); }\n";
        let err = run_pipeline(source).expect_err("inherent-via-dyn should be rejected");
        assert_eq!(err.stage, CompileStage::Typeck);
        assert!(
            err.message.contains("extra") && err.message.contains("Show"),
            "expected inherent-via-dyn error mentioning method and trait: got `{}`",
            err.message
        );
    }

    // ─── M07.7 / US2: `&dyn Trait` parameter + implicit coercion ─────────

    /// `fn print(x: &dyn Show) -> i32 { x.show() } let r = print(&p);` —
    /// ONE `print` frame (no `print::<Point>` mangling), nested
    /// `<Point as Show>::show` frame via vtable dispatch, r = 1_i32.
    #[test]
    fn run_pipeline_dyn_param() {
        let source = "struct Point { x: i32, y: i32 }\ntrait Show { fn show(&self) -> i32; }\nimpl Show for Point { fn show(&self) -> i32 { self.x } }\nfn print(x: &dyn Show) -> i32 { x.show() }\nfn main() { let p = Point { x: 1, y: 2 }; let r = print(&p); }\n";
        let events = run_pipeline(source).expect("dyn param compiles");
        let outer = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "print"
        ));
        assert!(outer, "expected FrameEnter `print` (no monomorphization)");
        // No mangled per-type frame should exist.
        let mangled = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name.starts_with("print::<")
        ));
        assert!(!mangled, "should NOT see `print::<Point>` — dyn is one-frame");
        let inner = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "<Point as Show>::show"
        ));
        assert!(inner, "expected nested FrameEnter `<Point as Show>::show`");
        let r_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Int { kind: crate::typeck::IntKind::I32, bits: 1 },
                ..
            }
        ));
        assert!(r_write, "expected r = 1_i32");
    }

    /// Two different concrete types passed to the SAME `print(x: &dyn Show)`
    /// — both calls hit ONE `print` frame; each inner dispatch resolves to
    /// the appropriate type's vtable.
    #[test]
    fn run_pipeline_dyn_param_two_types() {
        let source = "struct A { v: i32 }\nstruct B { v: i32 }\ntrait Show { fn show(&self) -> i32; }\nimpl Show for A { fn show(&self) -> i32 { self.v } }\nimpl Show for B { fn show(&self) -> i32 { self.v + 100 } }\nfn print(x: &dyn Show) -> i32 { x.show() }\nfn main() { let a = A { v: 1 }; let b = B { v: 2 }; let r1 = print(&a); let r2 = print(&b); }\n";
        let events = run_pipeline(source).expect("dyn param two types compiles");
        // TWO `print` frames (one per call), neither mangled.
        let print_frames = events.iter().filter(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "print"
        )).count();
        assert_eq!(print_frames, 2, "expected exactly two `print` frames");
        let a_dispatch = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "<A as Show>::show"
        ));
        let b_dispatch = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "<B as Show>::show"
        ));
        assert!(a_dispatch, "expected inner `<A as Show>::show` dispatch");
        assert!(b_dispatch, "expected inner `<B as Show>::show` dispatch");
    }

    /// Multiple `&dyn Show` borrows of the same Point share one vtable —
    /// VtableAlloc fires exactly once for the `(Show, Point)` pair.
    #[test]
    fn run_pipeline_dyn_vtable_interned() {
        let source = "struct Point { x: i32, y: i32 }\ntrait Show { fn show(&self) -> i32; }\nimpl Show for Point { fn show(&self) -> i32 { self.x } }\nfn main() { let p = Point { x: 1, y: 2 }; let d1: &dyn Show = &p; let d2: &dyn Show = &p; let s1 = d1.show(); let s2 = d2.show(); }\n";
        let events = run_pipeline(source).expect("vtable interning compiles");
        let alloc_count = events.iter().filter(|e| matches!(e,
            crate::MemEvent::VtableAlloc { trait_name, type_name, .. }
                if trait_name == "Show" && type_name == "Point"
        )).count();
        assert_eq!(
            alloc_count, 1,
            "expected exactly ONE VtableAlloc for (Show, Point) despite two DynRef constructions"
        );
    }

    // ─── M07.7 / US3: `Box<dyn Trait>` ────────────────────────────────────

    /// `let b: Box<dyn Show> = Box::new(p); let s = b.show();` — heap alloc,
    /// vtable alloc, Value::BoxDyn binding, dispatch through vtable.
    #[test]
    fn run_pipeline_box_dyn() {
        let source = "struct Point { x: i32, y: i32 }\ntrait Show { fn show(&self) -> i32; }\nimpl Show for Point { fn show(&self) -> i32 { self.x } }\nfn main() { let p = Point { x: 1, y: 2 }; let b: Box<dyn Show> = Box::new(p); let s = b.show(); }\n";
        let events = run_pipeline(source).expect("box dyn compiles");
        let heap_alloc = events.iter().any(|e| matches!(e, crate::MemEvent::HeapAlloc { .. }));
        assert!(heap_alloc, "expected HeapAlloc for Box::new");
        let vtable_alloc = events.iter().any(|e| matches!(e,
            crate::MemEvent::VtableAlloc { trait_name, type_name, .. }
                if trait_name == "Show" && type_name == "Point"
        ));
        assert!(vtable_alloc, "expected VtableAlloc for (Show, Point)");
        let b_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::BoxDyn { trait_name, .. },
                ..
            } if trait_name == "Show"
        ));
        assert!(b_write, "expected SlotWrite of Value::BoxDyn with trait_name=Show");
        let dispatch = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "<Point as Show>::show"
        ));
        assert!(dispatch, "expected FrameEnter `<Point as Show>::show`");
        let s_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Int { kind: crate::typeck::IntKind::I32, bits: 1 },
                ..
            }
        ));
        assert!(s_write, "expected s = 1_i32");
        let heap_free = events.iter().any(|e| matches!(e, crate::MemEvent::HeapFree { .. }));
        assert!(heap_free, "expected HeapFree at b's scope exit (Box dropped)");
    }

    // ─── M07.7 / US4: static vs dyn (the headline contrast) ───────────────

    /// `fn s<T: Show>(x: T)` (static) vs `fn d(x: &dyn Show)` (dynamic) —
    /// outer frames distinguish dispatch style; inner frames identical.
    #[test]
    fn run_pipeline_static_vs_dyn() {
        let source = "struct Point { x: i32, y: i32 }\ntrait Show { fn show(&self) -> i32; }\nimpl Show for Point { fn show(&self) -> i32 { self.x } }\nfn s<T: Show>(x: T) -> i32 { x.show() }\nfn d(x: &dyn Show) -> i32 { x.show() }\nfn main() { let p = Point { x: 1, y: 2 }; let a = s(p); let b = d(&p); }\n";
        let events = run_pipeline(source).expect("static vs dyn compiles");
        // Static dispatch: mangled `s::<Point>` frame.
        let static_frame = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "s::<Point>"
        ));
        assert!(static_frame, "expected outer FrameEnter `s::<Point>` (monomorphized)");
        // Dynamic dispatch: bare `d` frame (no mangling).
        let dyn_frame = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "d"
        ));
        assert!(dyn_frame, "expected outer FrameEnter `d` (one-frame dyn)");
        let dyn_mangled = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name.starts_with("d::<")
        ));
        assert!(!dyn_mangled, "should NOT see `d::<Point>` — dyn is one-frame");
        // Both call paths land in the same UFCS-style inner method frame.
        let inner_frames = events.iter().filter(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "<Point as Show>::show"
        )).count();
        assert_eq!(inner_frames, 2, "expected two `<Point as Show>::show` dispatches");
    }

    // ─── M07.7 / cross-cutting: default method through dyn ────────────────

    /// Mutability: `&T → &mut dyn Trait` rejected (shared-to-mut upgrade
    /// is not allowed). Sibling rejection to `dyn_coercion_error`.
    #[test]
    fn run_pipeline_dyn_mut_coercion_rejected() {
        let source = "struct Point { x: i32, y: i32 }\ntrait Show { fn show(&self) -> i32; }\nimpl Show for Point { fn show(&self) -> i32 { self.x } }\nfn main() { let p = Point { x: 1, y: 2 }; let d: &mut dyn Show = &p; let _ = d.show(); }\n";
        let err = run_pipeline(source).expect_err("&T → &mut dyn should be rejected");
        assert_eq!(err.stage, CompileStage::Typeck);
        // Error message can match either the explicit-cast wording or the
        // general type-mismatch fallback — both convey the same rejection.
        assert!(
            err.message.contains("&mut dyn") || err.message.contains("expected"),
            "expected mutability-mismatch error: got `{}`",
            err.message
        );
    }

    /// Default method dispatched through `&dyn Trait` — impl provides only
    /// `count`; calling `d.double()` on `d: &dyn Counter` routes through the
    /// vtable to the trait's default body, whose `self.count()` re-dispatches
    /// to the impl override. v = 2 * 1 = 2.
    #[test]
    fn run_pipeline_dyn_default_method() {
        let source = "struct Point { x: i32, y: i32 }\ntrait Counter { fn count(&self) -> i32; fn double(&self) -> i32 { self.count() * 2 } }\nimpl Counter for Point { fn count(&self) -> i32 { self.x } }\nfn main() { let p = Point { x: 1, y: 2 }; let d: &dyn Counter = &p; let v = d.double(); }\n";
        let events = run_pipeline(source).expect("default method through dyn compiles");
        let outer = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "<Point as Counter>::double"
        ));
        let inner = events.iter().any(|e| matches!(e,
            crate::MemEvent::FrameEnter { fn_name, .. } if fn_name == "<Point as Counter>::count"
        ));
        assert!(outer, "expected outer FrameEnter `<Point as Counter>::double` (default body via dyn)");
        assert!(inner, "expected nested FrameEnter `<Point as Counter>::count` (re-dispatch through impl)");
        let v_write = events.iter().any(|e| matches!(e,
            crate::MemEvent::SlotWrite {
                value: crate::Value::Int { kind: crate::typeck::IntKind::I32, bits: 2 },
                ..
            }
        ));
        assert!(v_write, "expected v = 2_i32 (1 * 2)");
    }
}
