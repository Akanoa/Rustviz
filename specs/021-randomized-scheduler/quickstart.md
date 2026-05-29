# Quickstart — Randomized scheduler: dev + QA

Audience: maintainer + contributors working on this feature or extending it.

## Run the page

```bash
cd web && trunk serve --open
```

After this feature ships, the toolbar has a numeric `seed` input + a 🎲 re-roll button. The default seed on first load is `0`.

## Run all tests

```bash
cargo test
```

181 baseline tests should continue to pass for non-threaded programs. M08 snapshot tests re-baseline once (`INSTA_UPDATE=always cargo test --test m08`). A new `[[test]]` target `m08_2` adds the invariant tests.

To re-snap M08 only:

```bash
INSTA_UPDATE=always cargo test --test m08
git diff src/snapshots/m08__*  # review the new traces before committing
```

## Manual QA procedure

~12 minutes. Verifies the feature end-to-end across the three user stories + edge cases.

### 1. First-visit default seed

- Open dev tools → check `seed-input` value is `0`.
- Load the M08 `Arc<Mutex>` sample. Observe the trace plays back with one specific interleaving.
- Reload the page. Confirm the seed is `0` again. Step to step N; the visualization is byte-identical to before.

### 2. US1 — Seeded scheduling produces interleaved trace

- Load `Arc<Mutex> (M08)`. Step through the trace.
- Confirm the spawned closure runs SOME steps before main reaches `h.join()` — i.e., the trace is NOT the M08 v1 "everything happens at join" pattern.
- Reload, step again. Confirm same steps appear at same indices (determinism).

### 3. US2 — Change seed, see different interleaving

- Enter seed `1` in the input field. Wait 300ms. Confirm trace re-renders.
- Note the step count and the order of events (e.g., "closure runs steps 5-7 before main step 8").
- Enter seed `2`. Confirm trace re-renders DIFFERENTLY (different event order or different step count).
- Enter seed `1` again. Confirm trace matches the first observation byte-identically.

### 4. US3 — Re-roll button

- Click 🎲 three times in quick succession.
- Each click should: (a) update the seed input value visibly, (b) re-render the trace.
- Each render should produce a (typically) different interleaving from the previous.
- Note one of the re-rolled seeds (e.g., `1742368512`). Manually enter it later and confirm reproducibility.

### 5. Edge case — Single-threaded sample with seed

- Load `Arithmetic (M03)` (no `thread::spawn`).
- Change the seed to `42`, `1000`, then re-roll several times.
- The trace should remain byte-identical across all seeds (no scheduling decisions to randomize).

### 6. Edge case — Seed `0` and `4294967295`

- Enter `0`: confirm valid trace.
- Enter `4294967295` (max u32): confirm valid trace.
- Enter `-1` or `4294967296`: confirm input reverts to last valid value, no re-run.

### 7. Edge case — Deadlock detection

- Load a hand-crafted deadlock sample (TBD — `m08_2_deadlock.rs`) that locks `m1` then `m2` in thread A while thread B locks `m2` then `m1`.
- With certain seeds, observe the trace ending with a `Deadlock` event.
- Status bar should show "deadlock: threads #X, #Y waiting on each other."
- Player stops at the deadlock step. Step back to inspect prior state.

### 8. Edge case — Storage failure / private mode

- (Not applicable to this feature — seed is not persisted.)

### 9. Edge case — Mutex lock pattern restriction

- Try a sample with `let x = m.lock().value + 1` (mutex call mid-expression).
- Confirm the runtime error surfaces in the status bar: `M08.2 lock pattern restriction: wrap m.lock() in a let-binding before using the guard`.

### 10. Existing samples regression

- Cycle through M01-M07.7 samples (single-threaded). All should display byte-identically to pre-021 (SC-002).
- Cycle through M08 samples. Traces will differ from pre-021 by design (M08 v1 strict-deferred is replaced).

## Developer notes

### Why xorshift64\* and not the `rand` crate?

`rand` (even minimal `rand_core`) adds ~100KB of WASM after dead-code elimination. The visualizer needs reproducibility, not cryptographic quality. xorshift64\* has period 2^64-1 and passes BigCrush — far more than enough for picking among 2-4 ready threads at a few dozen scheduling points.

### Why statement-granularity scheduling?

Sub-statement preemption would require continuation-passing rewrite of `eval.rs` (every `eval_*` becomes `async fn` or returns a state-machine future). Statement-level scheduling matches the existing recursive eval with minimal surgery and gives the learner stable "what state is each thread in" snapshots between scheduling decisions.

### Why the "lock as RHS of let-binding" restriction?

Re-entering a partially-evaluated statement requires either side-effect rollback (complex) or evaluation idempotence (fragile). Restricting `mutex.lock()` to the simplest pattern (`let g = m.lock();`) means a parked thread can re-run the entire statement on resume — same outcome, no rollback needed. All existing M08 samples comply.

### Why a Note on every scheduling decision?

The scheduler decision is the LEARNING moment: "this code has multiple valid executions; here's why thread T was picked over T' right now." Surfacing it via the existing pedagogical-Note infrastructure keeps the visual vocabulary consistent and explains the non-determinism to a learner who's never seen it before.

### Why is the seed not persisted across reloads?

The seed belongs to "this run," not "this user." Persisting it would imply the learner WANTS the same interleaving every visit, which conflicts with the goal of exploring variations. Reloading resets to seed=0 and the learner explicitly opts in to other seeds via input or re-roll.

## When extending in future iterations

- **Memory-ordering modeling** (`Relaxed`/`Acquire`/`Release` semantics): out of scope for 021. Would need a per-thread "view of memory" model — separate visualization design problem.
- **Loom-style exhaustive interleaving search**: this feature picks ONE interleaving per seed. Exhaustive search is a different tool (bug-finding vs. pedagogy).
- **Cross-session seed persistence via URL**: future enhancement. Encode in `?seed=` query param.
- **Sub-statement scheduling**: when a future milestone needs it, will require eval.rs → async refactor. Lifts the "lock as RHS of let-binding" restriction at the same time.
- **Per-sample default seed**: could pre-bake a default seed per sample that's known to produce a pedagogically clean trace. Not necessary for v1.

## What this iteration does NOT add

- Memory-ordering / atomic-operation modeling.
- Exhaustive interleaving exploration (Loom-style).
- URL-encoded seed sharing.
- Cross-tab seed coordination.
- Seed history / undo list.
- Per-sample default seed memory.
- Sub-statement preemption.
- Continuation-passing eval rewrite.
