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
const M08_2_DEADLOCK: &str = include_str!("../web/samples/m08_2_deadlock.rs");
const M08_2_COUNTER: &str = include_str!("../web/samples/m08_2_counter.rs");

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

/// At least one (seed_a, seed_b) pair produces non-identical traces on
/// the M08 Arc<Mutex> sample. Phase 3 cooperative scheduling means seed
/// controls the order main vs. the spawned closure run their stmts, so
/// different seeds visibly shift event ordering.
/// Covers B-M082-3, SC-001.
#[test]
fn different_seed_divergence() {
    let baseline = run_pipeline(M08_ARC_MUTEX, 0).expect("compiles");
    let mut diverged_seeds = 0;
    for seed in 1u32..=20 {
        let trace = run_pipeline(M08_ARC_MUTEX, seed).expect("compiles");
        if trace != baseline {
            diverged_seeds += 1;
        }
    }
    assert!(
        diverged_seeds > 0,
        "expected at least one of seeds 1..=20 to produce a trace different from seed=0; got {diverged_seeds} divergent seeds (the scheduler appears purely deterministic — multi-Ready picks may not be exercising the PRNG)"
    );
}

/// Regression for the "thread frame grayed before body finishes" bug:
/// `ThreadJoin` MUST fire only after the target thread reaches Done.
/// Without this guarantee, the UI marks the thread joined (grayed-out)
/// while its body's events keep firing — visually confusing. Asserts
/// that for every ThreadJoin in the trace, all of that thread's events
/// (FrameEnter / SlotAlloc / LockAcquire / LockRelease / FrameLeave)
/// appear BEFORE the ThreadJoin event.
#[test]
fn join_fires_after_thread_actually_done() {
    use rustviz::MemEvent;
    // The seed reported by the user for the original glitch.
    let seed = 2267206117u32;
    let trace = run_pipeline(M08_2_COUNTER, seed).expect("counter sample compiles");
    // Find every ThreadJoin and assert no events of the joined thread
    // appear AFTER it (other than the ThreadJoin itself).
    let mut joined_threads: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for (i, ev) in trace.iter().enumerate() {
        if let MemEvent::ThreadJoin { thread_id, .. } = ev {
            joined_threads.insert(*thread_id);
            // Scan forward — no ThreadSwitch back to this thread should
            // follow (the thread is supposedly Done).
            for later in &trace[i+1..] {
                if let MemEvent::ThreadSwitch { thread_id: tid, .. } = later {
                    if tid.0 == *thread_id {
                        panic!(
                            "ThreadSwitch to thread {} fired AFTER its ThreadJoin (event {i}). Trace not well-ordered.",
                            thread_id
                        );
                    }
                }
            }
        }
    }
    assert!(joined_threads.len() >= 2, "expected ≥ 2 joined threads in the counter sample");
}

/// Race-for-lock sample: two threads each try to lock the same Arc<Mutex>.
/// The seed determines which thread acquires first; the other parks until
/// the first releases. The trace MUST diverge widely across seeds (this
/// is the strongest divergence signal — every Ready-set decision matters).
#[test]
fn counter_sample_diverges_across_seeds() {
    let baseline = run_pipeline(M08_2_COUNTER, 0).expect("counter sample compiles");
    let mut divergent = 0;
    for seed in 1u32..=20 {
        let trace = run_pipeline(M08_2_COUNTER, seed).expect("compiles");
        if trace != baseline {
            divergent += 1;
        }
    }
    assert!(
        divergent >= 10,
        "expected ≥ 10 of 20 seeds to diverge from seed=0 on the counter-race sample; got {divergent}"
    );
}

/// Detect deadlock on the m08_2_deadlock sample: under at least one seed
/// in 1..=50, the trace ends with `MemEvent::Deadlock`. Cover B-M082-4,
/// SC-007. The sample acquires locks in opposite orders on two threads;
/// with the right interleaving, both end up waiting on each other.
#[test]
fn deadlock_detection() {
    use rustviz::MemEvent;
    let mut found_deadlock_seed = None;
    for seed in 0u32..=50 {
        let trace = run_pipeline(M08_2_DEADLOCK, seed).expect("compiles");
        if let Some(MemEvent::Deadlock { thread_ids, .. }) = trace.last() {
            assert!(!thread_ids.is_empty(), "Deadlock thread_ids must be non-empty");
            found_deadlock_seed = Some(seed);
            break;
        }
    }
    assert!(
        found_deadlock_seed.is_some(),
        "expected at least one of seeds 0..=50 to produce a Deadlock event on the m08_2_deadlock sample"
    );
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
