//! M08.2 — randomized (seeded) scheduler invariant tests.
//!
//! These tests assert contract-level properties that must hold regardless
//! of the underlying scheduler implementation:
//!
//! * **same_seed_determinism** (B-M082-1, SC-003): re-running the pipeline
//!   with the same `(source, seed)` pair produces bytewise-identical event
//!   streams.
//! * **single_thread_invariance** (B-M082-2, SC-002): single-threaded
//!   programs produce bytewise-identical event streams across ALL seeds.
//!
//! `different_seed_divergence` (B-M082-3, SC-001) lands when the
//! cooperative-scheduling refactor (Phase 3 T012-T019) creates multi-Ready
//! sets that the PRNG can actually reorder. Currently single-spawn M08
//! samples produce the same trace under any seed because there's never
//! more than one Ready thread at a time.
//!
//! `deadlock_detection` (B-M082-4, SC-007) lands with Phase 6 (T037-T040).

use rustviz::run_pipeline;

const M08_ARC_MUTEX: &str = include_str!("../web/samples/m08_arc_mutex.rs");
const M03_ARITHMETIC: &str = include_str!("../web/samples/m03_arithmetic.rs");
const M03_FN_CALL: &str = include_str!("../web/samples/m03_fn_call.rs");
const M07_BOX: &str = include_str!("../web/samples/m07_box.rs");

/// Same source + same seed → byte-identical event stream on every run.
/// Covers B-M082-1, SC-003.
#[test]
fn same_seed_determinism() {
    let trace_a = run_pipeline(M08_ARC_MUTEX, 42).expect("M08 Arc<Mutex> compiles");
    let trace_b = run_pipeline(M08_ARC_MUTEX, 42).expect("M08 Arc<Mutex> compiles");
    assert_eq!(trace_a, trace_b, "same (source, seed=42) must produce identical traces");

    // Spot-check across a few different seeds — each should be self-consistent.
    for seed in [0u32, 1, 7, 1000, u32::MAX] {
        let a = run_pipeline(M08_ARC_MUTEX, seed).expect("compiles");
        let b = run_pipeline(M08_ARC_MUTEX, seed).expect("compiles");
        assert_eq!(a, b, "same (source, seed={seed}) must produce identical traces");
    }
}

/// Single-threaded programs produce identical traces across all seeds.
/// The scheduler bypasses the PRNG when only one thread is Ready (VR-S2),
/// so the trace must NOT depend on the seed for single-thread programs.
/// Covers B-M082-2, SC-002.
#[test]
fn single_thread_invariance() {
    for sample in [M03_ARITHMETIC, M03_FN_CALL, M07_BOX] {
        let baseline = run_pipeline(sample, 0).expect("single-thread sample compiles");
        for seed in [1u32, 42, 1000, 1_000_000, u32::MAX] {
            let trace = run_pipeline(sample, seed).expect("compiles");
            assert_eq!(
                trace, baseline,
                "single-thread program produced a different trace for seed={seed} (must match seed=0)"
            );
        }
    }
}
