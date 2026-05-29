# Research — Randomized (seeded) thread scheduler: design decisions

8 decisions covering PRNG choice, cooperative-scheduling architecture, scheduling-point granularity, parking semantics, deadlock detection, UI surface, snapshot strategy, and pedagogical Notes integration.

## R-001 — PRNG: hand-rolled xorshift64\* (no external dep)

- **Decision**: implement a minimal `Prng { state: u64 }` with `next_u64()` advancing via xorshift64\*. Used inline by the `Scheduler` to pick uniformly from the ready set.
- **Rationale**: a single-multiplier xorshift64\* has period 2^64 − 1, passes BigCrush, and is ~5 LOC. Pulling in `rand` (~100KB of WASM after dead-code elimination) or even `rand_core` would dwarf the entire feature's bundle delta. The visualizer needs reproducibility, not cryptographic randomness.
- **Algorithm**:
  ```rust
  fn next(&mut self) -> u64 {
      self.state ^= self.state >> 12;
      self.state ^= self.state << 25;
      self.state ^= self.state >> 27;
      self.state.wrapping_mul(0x2545F4914F6CDD1D)
  }
  ```
- **Seed handling**: the public API takes `u32` (per FR-006) which is splatted to `u64` via `((seed as u64) << 32) | (seed as u64)` so seed=0 doesn't produce a degenerate state.
- **Alternatives considered**:
  - `rand_xoshiro::Xoshiro256PlusPlus`: better quality but adds 100+ KB. Rejected.
  - `oorandom::Rand32`: tiny but slightly worse statistical quality. Rejected for marginal benefit.
  - `wyrand`: similar size to xorshift but uses 128-bit multiply, larger code on wasm32. Rejected.

## R-002 — Cooperative scheduling: statement-boundary granularity

- **Decision**: the scheduler is consulted at every **statement boundary** in the per-thread eval loop. Within a single statement the current thread runs to completion (uninterrupted). After each statement, control returns to a top-level scheduler loop that picks the next Ready thread.
- **Rationale**: matches the existing recursive `eval_stmt` structure with minimum surgery. Rust statements are the natural "small step" granularity for a pedagogical visualizer — sub-statement preemption would let the learner step through fragments of expressions where they don't reach a stable state, defeating the "what state is each thread in right now" mental model.
- **Architecture**:
  ```text
  loop {
      let tid = scheduler.pick(&ready_set);   // seeded random pick
      let progress = eval_one_stmt(tid);       // recursive eval, runs to stmt end
      match progress {
          Done       => mark tid Done,
          Continue   => keep tid in Ready,
          ParkedLock => move tid to BlockedOnLock(addr),
          ParkedJoin => move tid to BlockedOnJoin(target),
      }
      if ready_set.is_empty() && any_blocked() {
          emit Deadlock; break;
      }
      if all_done() { break; }
  }
  ```
- **Alternatives considered**:
  - **Sub-statement (per-event) granularity**: needs continuation-passing rewrite of eval. Rejected for v1 — too much refactor for marginal pedagogical gain.
  - **Whole-thread runs-to-completion**: M08 v1's strict-deferred model. Rejected — defeats the entire point of 021.
  - **Async/await rewrite**: cleanest long-term but `wasm-bindgen` async tooling adds noise; the bulk of `eval.rs` would need to become `async fn`. Deferred to a future milestone if/when sub-statement preemption is needed.

## R-003 — Mutex parking via "first operation in stmt" pattern

- **Decision**: support real parking ONLY when `mutex.lock()` appears as the entire RHS of a `let` binding: `let g = m.lock();`. Detect this pattern in `eval_stmt`; if `holder != None`, transition the thread to `BlockedOnLock(addr)` and return to scheduler. NO events emitted in the parked case (the stmt didn't execute).
- **Rationale**: re-entry of a partially-evaluated statement requires either side-effect rollback (complex) or evaluation idempotence (fragile). Restricting the pattern to "lock is the first observable action" means the parked thread can simply re-run the entire statement from scratch on next schedule — same outcome.
- **Coverage check**: all existing M08 samples (`m08_thread_spawn`, `m08_arc_clone`, `m08_arc_mutex`) use the `let g = m.lock()` pattern. v1 supports them without modification. Future samples that need mid-expression locking get a clear runtime error: `M08.2 lock pattern restriction: wrap m.lock() in a let-binding before using the guard.`
- **Lock release path**: when the holder thread drops its `MutexGuard` (existing M08 code), the scheduler scans `BlockedOnLock(addr)` threads and flips ALL waiting on this addr back to Ready. The scheduler then picks among them on its next tick (with seeded randomness — which parked thread gets the lock is itself a scheduling decision).
- **Alternatives considered**:
  - **Synchronous spin-wait**: thread "waits" by emitting wait Notes until lock released. Deterministic but doesn't model parking semantics — defeats pedagogy.
  - **Continuation-style park/resume**: encoded as Rust generators (`genawaiter`) or hand-rolled state machines. Defers v1 ship for substantial refactor work. Rejected for v1.

## R-004 — Join parking (same model as lock)

- **Decision**: `JoinHandle::join()` on a thread that isn't `Done` transitions the calling thread to `BlockedOnJoin(target)`. When `target` reaches `Done`, the blocked thread flips to Ready.
- **Constraint mirrors R-003**: `h.join()` must be the entire RHS of a let-binding or a statement-expression. Existing samples use `h.join();` (statement) which qualifies. Future patterns like `let x = h.join().unwrap();` need the same wrap-then-use pattern.
- **Detection in eval_stmt**: same pre-check pattern as mutex_lock — inspect the RHS at the stmt level, park if target not Done, run normally if Done.

## R-005 — Deadlock detection & surfacing

- **Decision**: at the END of the scheduler loop iteration, if `ready_set.is_empty()` AND `any thread is BlockedOn*`, the program is deadlocked. Emit a single `MemEvent::Deadlock { thread_ids, span }` listing all blocked threads + the span of the last scheduling decision, then halt the evaluator.
- **UI surface**: the existing pedagogical-note + status-bar pattern shows "deadlock: threads #1, #2 waiting on each other's locks." The player stops at this step (no further steps available). The trace is visible up to the deadlock point, so the learner can step back to inspect.
- **Detection semantics**: a "deadlock" means NO progress is possible. A thread blocked on a lock held by a thread that's itself blocked on the first thread's lock is the textbook case. The scheduler doesn't need to detect the cycle explicitly — empty Ready set + non-empty Blocked set is sufficient (since nothing can ever release a lock if no thread can run).
- **Alternatives considered**:
  - **Timeout-based detection** (run N more steps; if no thread becomes Ready, declare deadlock): less precise, requires an arbitrary N. Rejected.
  - **Explicit cycle detection** in the lock dependency graph: more precise but more code. Not needed for the simple definition above.

## R-006 — Seed input UI: number field + dice button

- **Decision**: add to the toolbar a `<input type="number" id="seed-input" min="0" max="4294967295" value="0">` plus a `<button id="btn-reroll-seed">🎲</button>` button. Both sit between the play controls and the step indicator. Current seed value lives in the input field at all times — that's the "display."
- **Behaviors**:
  - **On input change** (debounced 300ms): parse the value as u32; clamp to [0, 2^32-1]; re-run pipeline with new seed.
  - **On re-roll click**: generate `Math.floor(Math.random() * 0x1_0000_0000)`; set input value; trigger re-run (no debounce).
  - **On non-numeric input**: revert to last valid seed, no re-run.
- **Accessibility**: input has `<label for="seed-input">seed</label>`; re-roll button has `aria-label="Generate new random seed"`. Both are reachable via tab.
- **Rationale**: matches the existing toolbar's visual vocabulary (small input + small button, sans elaborate styling). The dice emoji is universally recognizable as "random" without requiring an icon font or SVG.
- **Alternatives considered**:
  - **Slider UI**: clamps to a visible range, fun to drag. Rejected — the seed range is 2^32 which doesn't map well to a slider. Manual entry stays useful.
  - **Hidden behind a settings menu**: would obscure the feature. Rejected — surface it in the toolbar so learners DISCOVER it.

## R-007 — Snapshot test strategy: one-time re-baseline + new invariant tests

- **Decision**: existing M08 snapshot tests (`m08_thread_spawn`, `m08_arc_clone`, `m08_arc_mutex`) re-baseline ONCE under the new scheduler. The new traces are committed to the snapshot files. A new `[[test]]` target `m08_2` adds tests that don't lock to a specific trace shape but assert invariants:
  - `same_seed_determinism`: run the M08 Arc<Mutex> sample twice with seed=42, assert traces are byte-identical.
  - `different_seed_divergence`: run with seed=1 and seed=2, assert traces are byte-non-identical for ≥ 80% of M08 samples (SC-001).
  - `single_thread_invariance`: run an M01 sample with seeds [0, 1, 42, 4294967295], assert all four traces are byte-identical.
  - `deadlock_detection`: a hand-crafted Mutex-cycle sample that deadlocks under at least one seed; assert the trace ends with `Deadlock` and the player stops.
- **Rationale**: avoids snapshotting concrete event indices that would re-baseline every time the scheduler changes. The invariant assertions catch real regressions (replay determinism, single-thread purity) without rigidifying the trace shape.

## R-008 — Pedagogical Notes around scheduler decisions

- **Decision**: when the scheduler picks thread T (out of >1 Ready threads), emit a `MemEvent::Note { kind: Info, message: "Scheduler picked thread #{T} (seed={seed}, {N} other threads were also Ready)." }`. The Note coalesces with the NEXT event from thread T via the existing SlotAlloc→Note + adjacent-Note coalescer, so the cursor doesn't gain a dead step.
- **Decision sub-rule**: do NOT emit a Note when there's only one Ready thread (the choice is forced, no pedagogy). This avoids cluttering single-threaded samples and uncontended-multi-thread programs.
- **Lock release Note**: when the lock holder drops the guard and the scheduler picks among waiting threads, emit a Note "Lock released; thread #{T} was selected to acquire next (seed={seed}, threads {OTHERS} were also waiting)." This makes the parking → unparking transition explicit pedagogy.
- **Rationale**: the scheduler decision is the LEARNING moment for "thread interleaving is non-deterministic." Surfacing it via existing Note infrastructure keeps the visual vocabulary consistent with the rest of the project.

## Open questions deferred to implementation

None blocking. The architectural choices above are sufficient to start implementation; minor details (exact debounce ms, exact wording of Notes) will be tuned during build.
