//! Integration test driver for M03.
//!
//! Each sample under `tests/samples/m03_*.rs` is parsed → resolved → type-checked
//! → evaluated; the resulting `Vec<MemEvent>` (or `ParseError`) is snapshotted
//! via `insta::assert_debug_snapshot!`. Snapshots live under `tests/snapshots/`.
//!
//! The macro also performs a US2 / SC-002 check on every event before
//! snapshotting: every event must carry a span where `start <= end` and where
//! `start < end` (non-empty) unless the event is a `FrameLeave` (whose span is
//! the body's range, which IS non-empty in practice — kept as a separate
//! exception slot for future flexibility).

use std::path::PathBuf;

use rustviz::{evaluate, parse, resolve, typeck, MemEvent, ParseError, SourceMap};

#[derive(Debug)]
#[allow(dead_code)] // fields read only by snapshot serialization
enum EvalResult {
    Ok(Vec<MemEvent>),
    StaticErr(ParseError),
}

fn analyze_sample(name: &str) -> EvalResult {
    let path: PathBuf = ["tests", "samples", &format!("{name}.rs")].iter().collect();
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("sample file {path:?}: {e}"));
    let mut sm = SourceMap::new();
    let file = sm.add(format!("{name}.rs"), src);

    let program = match parse(file, &sm) {
        Ok(p) => p,
        Err(e) => return EvalResult::StaticErr(e),
    };
    let resolution = match resolve(&program) {
        Ok(r) => r,
        Err(e) => return EvalResult::StaticErr(e),
    };
    let types = match typeck(&program, &resolution) {
        Ok(t) => t,
        Err(e) => return EvalResult::StaticErr(e),
    };
    match evaluate(&program, &resolution, &types) {
        Ok(events) => EvalResult::Ok(events),
        Err(e) => EvalResult::StaticErr(e),
    }
}

/// Extract the `span` from any `MemEvent` variant. Used by [`assert_spans_ok`].
fn event_span(event: &MemEvent) -> rustviz::Span {
    match event {
        MemEvent::ThreadSpawn { span, .. }
        | MemEvent::ThreadJoin { span, .. }
        | MemEvent::ThreadPark { span, .. }
        | MemEvent::FrameEnter { span, .. }
        | MemEvent::FrameLeave { span, .. }
        | MemEvent::SlotAlloc { span, .. }
        | MemEvent::SlotWrite { span, .. }
        | MemEvent::SlotMove { span, .. }
        | MemEvent::SlotDrop { span, .. }
        | MemEvent::HeapAlloc { span, .. }
        | MemEvent::HeapRealloc { span, .. }
        | MemEvent::HeapFree { span, .. }
        | MemEvent::StaticAlloc { span, .. }
        | MemEvent::BytesCopy { span, .. }
        | MemEvent::VtableAlloc { span, .. }
        | MemEvent::BorrowShared { span, .. }
        | MemEvent::BorrowMut { span, .. }
        | MemEvent::BorrowEnd { span, .. }
        | MemEvent::LockAcquire { span, .. }
        | MemEvent::LockRelease { span, .. }
        | MemEvent::ArcClone { span, .. }
        | MemEvent::ArcDrop { span, .. }
        | MemEvent::Note { span, .. }
        | MemEvent::ReturnValue { span, .. } => *span,
    }
}

/// US2 / SC-002 assertion: every event carries a non-zero span.
fn assert_spans_ok(events: &[MemEvent]) {
    for (i, event) in events.iter().enumerate() {
        let span = event_span(event);
        assert!(
            span.start <= span.end,
            "event #{i} has invalid span (start > end): {event:?}"
        );
        assert!(
            span.start < span.end,
            "event #{i} has zero-length span (no end-of-input case in L1): {event:?}"
        );
    }
}

macro_rules! sample_test {
    ($test_fn:ident, $sample:literal) => {
        #[test]
        fn $test_fn() {
            let result = analyze_sample($sample);
            if let EvalResult::Ok(events) = &result {
                assert_spans_ok(events);
            }
            insta::with_settings!(
                {
                    snapshot_path => "snapshots",
                    prepend_module_to_snapshot => false,
                },
                {
                    insta::assert_debug_snapshot!(result);
                }
            );
        }
    };
}

// US1 — happy path + runtime error.
sample_test!(emits_arithmetic, "m03_arithmetic");
sample_test!(emits_fn_call, "m03_fn_call");
sample_test!(emits_if_then, "m03_if_then");
sample_test!(emits_if_else, "m03_if_else");
sample_test!(emits_shadow, "m03_shadow");
sample_test!(emits_nested_block, "m03_nested_block");
sample_test!(emits_short_circuit, "m03_short_circuit");
sample_test!(emits_div_by_zero_note, "m03_div_by_zero");
