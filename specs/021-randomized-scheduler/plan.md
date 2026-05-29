# Implementation Plan: Randomized (seeded) thread scheduler

**Branch**: `021-randomized-scheduler` | **Date**: 2026-05-29 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/021-randomized-scheduler/spec.md`

## Summary

Replace M08 v1's strict-deferred "spawned threads run to completion at end of stmt" scheduler with a **cooperative scheduler driven by a seeded PRNG**. The scheduler picks which Ready thread advances at every scheduling point (thread spawn, statement boundary, mutex contention, join). Same seed → same trace (deterministic). Different seed → different valid interleaving (pedagogical value). UI gains a seed input + re-roll button + seed display.

This feature **absorbs M08.1** (real Mutex parking + contention handling) — the pedagogical payoff of randomization requires real parking; landing them separately would deliver M08.1 with an artificially-deterministic order and 021 with no parking semantics. They go together.

Authority chain: spec.md (this feature) → this plan → research.md (PRNG choice + cooperative-scheduling model) → data-model.md (Scheduler entity + ThreadState extensions) → tasks.md.

## Technical Context

**Language/Version**: Rust 2024 edition (same toolchain as M01–M08). No new toolchain requirements.
**Primary Dependencies**: existing `indexmap`, `serde`, `serde_json`, `wasm-bindgen`, `js-sys`, `console_error_panic_hook`. **No new Rust deps**. PRNG implemented inline (~30 LOC xorshift state machine; avoids pulling in `rand`/`rand_core`).
**Storage**: in-memory only. Seed is part of `Player` state and the WASM bridge's `set_source(source, seed)` signature. No persistence (FR / Assumptions).
**Testing**: existing `cargo test` continues to pass (181 baseline). NEW snapshot tests under a `[[test]]` target `m08_2` covering: same-seed determinism, different-seed divergence, single-thread invariance, deadlock detection. Existing M08 snapshot tests re-baseline once (the strict-deferred order is gone) — captured as a one-time re-snap, not a regression.
**Target Platform**: same as M01–M08 (host + `wasm32-unknown-unknown`, modern desktop browsers ≥ 1024px).
**Project Type**: Rust library + companion UI. Touches `src/eval.rs` + `src/event.rs` + `src/ui.rs` + `src/pipeline.rs` + `web/index.{html,js}`. CSS minor.
**Performance Goals**: SC-006 — seed change to re-render < 1s for all existing samples. Bounded by the PRNG state mutation (cheap) and trace re-render (already < 100ms for M08 samples).
**Constraints**: WASM bundle ≤ +5% (SC-008). Zero Rust warnings (SC-009). M03 / M02 / M01 snapshot tests must stay byte-identical for non-threaded programs (SC-002).
**Scale/Scope**: ~500–700 LOC across `src/eval.rs` (scheduler module + cooperative re-entry) + `src/event.rs` (`Deadlock` variant) + `src/ui.rs` (set_source signature change) + web UI strip (~50 LOC). **Sized M** — bigger than 020 (UI shell) but smaller than M08 v1 (which added the entire threading vocabulary).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

`.specify/memory/constitution.md` is still the unfilled speckit template. Same vacuous PASS as features 001–020.

**Post-design re-check**: still vacuous, still PASS.

## Project Structure

### Documentation (this feature)

```text
specs/021-randomized-scheduler/
├── plan.md                         # This file
├── spec.md                         # Feature spec
├── research.md                     # Phase 0: PRNG choice + scheduling architecture + UI layout
├── data-model.md                   # Phase 1: Scheduler state, ThreadStatus transitions, seed flow
├── quickstart.md                   # Phase 1: dev workflow + manual QA procedure
├── contracts/
│   ├── m08_2-api.md                # Phase 1: trace shape (Deadlock variant), set_source signature
│   └── seed-ui-contract.md         # Phase 1: seed input + re-roll button contract
└── checklists/
    └── requirements.md             # From /speckit-specify (16/16 PASS)
```

### Source Code (repository root) — files this feature touches

```text
src/
├── eval.rs                # MODIFIED — extract scheduler into a `Scheduler` substruct of `Evaluator`;
│                            # replace `pending_thread_runs` FIFO with seeded random pick; cooperative
│                            # re-entry at statement boundaries + mutex contention; new ThreadStatus
│                            # variants (BlockedOnLock, BlockedOnJoin); deadlock detection. ~350-450
│                            # LOC delta.
├── event.rs               # MODIFIED — add `MemEvent::Deadlock { thread_ids, span }` variant. ~30 LOC.
├── ui.rs                  # MODIFIED — `Player::set_source` gains a `seed: u32` parameter; default seed
│                            # for `Player::new(source)` is `0`; `Player::set_seed(seed: u32)` re-runs
│                            # the pipeline with the new seed.
├── pipeline.rs            # MODIFIED — `run_pipeline(source, seed)` threads the seed through to the
│                            # evaluator; existing M03 snapshot tests pass `seed=0`.

web/
├── index.html             # MODIFIED — add `<input id="seed-input" type="number" min="0">` +
│                            # `<button id="btn-reroll-seed">🎲 New seed</button>` to the toolbar.
├── index.js               # MODIFIED — wire seed input change handler (debounced re-run); wire re-roll
│                            # button (generate fresh seed, set input value, re-run). Display current
│                            # seed in the toolbar.
└── style.css              # MODIFIED — small additions for the seed input + button styling. ~20 LOC.

# UNCHANGED:
samples/m08_*.rs                  # source files unchanged; traces will look different (deliberate
                                  # behavior change replacing the M08 v1 strict-deferred pattern).
tests/                            # existing tests continue to pass; new `m08_2` test target adds
                                  # determinism + divergence + invariance + deadlock cases.
```

**Structure Decision**: introduce a `Scheduler` substruct INSIDE `Evaluator` (not a separate trait/abstraction) — the M08 v1 evaluator is already heavily stateful and a sibling field is the minimum-blast-radius change. The scheduler owns the PRNG state, the ready-set, and the "next thread to advance" decision. ThreadStatus gains `BlockedOnLock(HeapAddr)` and `BlockedOnJoin(ThreadId)` variants. Statement-level cooperative re-entry — within a single statement the current thread runs uninterrupted; scheduler is consulted at every Stmt boundary in the top-level eval loop. Mutex contention mid-statement causes the thread to park (status → BlockedOnLock) and the scheduler to pick a different Ready thread; the parked thread re-tries on next pick. **Constraint for v1**: `mutex.lock()` must appear as the entire RHS of a `let` binding (the simplest pattern; covers all existing M08 samples). Mixed-expression Mutex acquisitions deferred to a future milestone via continuation-passing eval.

## Complexity Tracking

> No constitutional violations. Table omitted.

### Notable non-trivial complexity

- **Cooperative re-entry semantics**: when a thread parks mid-statement (mutex contention), the statement's pre-park side effects (events emitted, slot writes) must NOT be persisted. v1 sidesteps this by restricting Mutex-lock patterns to the simplest case (`let g = m.lock();` with no other RHS work). The lock check is the FIRST operation in the statement; if it parks, no events have fired yet. More complex patterns panic with a clear message: "M08.2 lock pattern restriction — wrap in a binding first." Future milestone removes the restriction via continuation-passing eval.
- **Deadlock detection**: at the end of each scheduler tick, if no thread is `Ready` AND at least one thread is `BlockedOn*`, emit a final `MemEvent::Deadlock { thread_ids, span }` and halt. UI surfaces this via the existing pedagogical-note + status-bar pattern.
- **Seed propagation**: `Player::set_source` signature changes from `(source: &str)` to `(source: &str, seed: u32)`. JS-side updates accordingly. Existing pipeline tests pass `seed=0` (their traces stay deterministic — single-thread programs don't use the seed; multi-thread programs re-baseline once).
- **PRNG state**: a single `u64` xorshift state. ~5 LOC of state advance. The scheduler calls `pick(ready: &[ThreadId]) -> ThreadId` (uniform pick from ready set).
- **Snapshot test re-baseline (one-time)**: existing `m08_thread_spawn`, `m08_arc_clone`, `m08_arc_mutex` snapshot tests have hard-coded traces assuming strict-deferred order. After this feature lands, these traces will differ. Re-snap with `INSTA_UPDATE=always cargo test`. New `m08_2` test target asserts the NEW invariants (determinism per seed, divergence across seeds, etc.).
- **Pedagogical Notes around scheduler decisions**: when the scheduler picks thread T over thread T', emit a Note explaining "the scheduler picked thread #T to advance next (seed=N gave it priority)". Notes coalesce with the next event via the existing coalescing rules so the cursor doesn't gain extra stops.
- **Re-roll UX**: the re-roll button generates a fresh seed via `Math.random() * 2^32 | 0`, populates the seed input field, and triggers re-run. The new seed is OBSERVABLE (input field updates) BEFORE the trace begins re-rendering.
- **Determinism guarantee**: the seeded PRNG is the ONLY source of non-determinism in the evaluator. All other code (map iteration etc.) is already deterministic (project uses `IndexMap` throughout). Re-affirmed in research.md.
- **Seed input debounce**: typing in the seed input field triggers a re-run after a 300ms debounce (matches the existing source-edit debounce). Re-roll button bypasses debounce.
- **Backwards-compatible Player API**: existing `Player::new(source)` constructor stays, defaults seed to 0. `Player::set_seed(seed)` is a new method that re-runs the pipeline with the new seed. `Player::set_source(source, seed)` becomes the canonical entry point.
- **Bundle size impact**: PRNG is tiny (~30 bytes of state + 100 bytes of advance code). Scheduler logic adds ~2-3 KB of WASM. Well under the 5% budget. The seed input + button add ~200 bytes of HTML/JS.
- **Single-threaded sample tests stay byte-identical**: the scheduler's pick is only called when MORE THAN ONE thread is in the ready set. For programs with one thread (main), pick always returns main → trace identical to the deterministic case. M01-M07.7 snapshot tests confirm this.
- **Notion of "atomic step" for the scheduler**: a step = one `eval_stmt` call on a thread plus the scheduler decision before/after. Within a step the scheduler does NOT yield. Locks that would contend force a step BOUNDARY (the thread's step ends; scheduler is consulted; parked thread waits).
