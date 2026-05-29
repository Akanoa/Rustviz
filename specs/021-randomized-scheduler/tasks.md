---
description: "Task list for the randomized (seeded) thread scheduler — replaces M08 v1's strict-deferred scheduling, absorbs M08.1 (real Mutex parking)"
---

# Tasks: Randomized (seeded) thread scheduler

**Input**: Design documents from `/specs/021-randomized-scheduler/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/m08_2-api.md ✓, contracts/seed-ui-contract.md ✓, quickstart.md ✓

**Tests**: New Rust snapshot tests under a `[[test]]` target `m08_2` covering same-seed determinism, different-seed divergence, single-thread invariance, deadlock detection. Existing M08 snapshot tests re-baseline ONCE (strict-deferred → randomized). Existing M01–M07.7 tests stay byte-identical (single-thread).

**Organization**: 3 user stories (US1 P1 default seeded scheduler, US2 P1 user-controlled seed, US3 P2 re-roll button). Sized M — ~500-700 LOC across `src/eval.rs` + `src/event.rs` + `src/ui.rs` + `src/pipeline.rs` + web UI strip.

**No UX checkpoint expected**: seed input + dice button are standard widgets. If the toolbar placement or pedagogical-Note wording reveals ambiguity, a checkpoint can be inserted between Phase 4 and Phase 5.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1/US2/US3 tag, mandatory on user-story phases only
- File paths relative to repo root

## Path Conventions

5 source files touched (`src/eval.rs`, `src/event.rs`, `src/ui.rs`, `src/pipeline.rs`, `Cargo.toml`), 3 web files (`web/index.html`, `web/index.js`, `web/style.css`), 1 new sample (`web/samples/m08_2_deadlock.rs`), 1 new test target (`tests/m08_2.rs`). Existing M01–M07.7 sources untouched.

---

## Phase 1: Setup

**Purpose**: pre-flight — confirm baseline + register the new test target.

- [X] T001 Verify pre-conditions: branch `021-randomized-scheduler` checked out; `cargo test` from `main` passes (181 baseline post-020); WASM bundle size baseline noted (~440 KB post-020); existing M08 sample traces noted (so the re-baseline diff is reviewable later).

- [X] T002 In `Cargo.toml`, register a new test target: `[[test]] name = "m08_2" path = "tests/m08_2.rs"`. Create empty stub `tests/m08_2.rs` with `#[test] fn placeholder() {}` so `cargo test --test m08_2` compiles before Phase 2 lands.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: PRNG + scheduler skeleton + ThreadStatus extension + Deadlock event variant + seed plumbing through the pipeline. Required by all user stories. Lands the architectural changes without yet changing observable behavior for existing samples.

- [X] T003 In `src/eval.rs`, add a private `Prng { state: u64 }` struct with `new(seed: u32) -> Self` (splats `seed` to `u64` via `((seed as u64) << 32) | (seed as u64)` so seed=0 doesn't degenerate) and `next_u64(&mut self) -> u64` (xorshift64\* per R-001). Add a `pick<T: Copy>(&mut self, choices: &[T]) -> T` helper that panics on empty input, returns the sole element without advancing state for single-element input (VR-S2 — preserves single-thread byte-identical traces), and otherwise uses `next_u64() % len as u64` to pick uniformly.

- [X] T004 In `src/eval.rs`, add a `Scheduler { prng: Prng, seed: u32 }` substruct as a sibling field of `Evaluator`. Add `Scheduler::new(seed: u32) -> Self`. Wire `Evaluator::new_with_seed(ast, types, seed: u32) -> Self` that constructs the scheduler; keep the existing `Evaluator::new(ast, types) -> Self` as a thin shim that calls `new_with_seed(ast, types, 0)` so M01–M07.7 callers compile unchanged.

- [X] T005 [P] In `src/event.rs`, add a new MemEvent variant `Deadlock { thread_ids: Vec<ThreadId>, span: Span }`. Add the `#[serde(...)]` annotations matching the existing tag-by-key pattern (`{"Deadlock": {...}}`). Update any `#[non_exhaustive]` match arms in `src/ui.rs` and `src/eval.rs` to handle the new variant (most will be no-ops in display logic at this phase; the actual emission lands in Phase 3).

- [X] T006 [P] In `src/eval.rs`, extend the existing `ThreadStatus` enum with two new variants: `BlockedOnLock(HeapAddr)` and `BlockedOnJoin(ThreadId)`. Both implement `Debug` + `Clone` + `PartialEq` matching the existing variants. No call sites yet emit these — the FIFO `pending_thread_runs` model still drives scheduling in this phase (T007 wires the random pick).

- [X] T007 In `src/eval.rs`, change `pending_thread_runs: Vec<ThreadId>` semantics from FIFO drain to a Ready set drained via `scheduler.pick(&ready_set)`. At every existing drain point in `eval_stmt`'s post-stmt block: collect Ready thread IDs into a sorted Vec (sorted by `tid.0` for determinism — VR-S3), call `pick` to choose one, advance that thread. Repeat until Ready set is empty. For single-Ready cases, behavior is byte-identical to FIFO (VR-S2 ensures no PRNG state advance). For multi-Ready, the order now depends on the seed.

- [X] T008 In `src/pipeline.rs`, change the public `run` signature from `run(source: &str) -> Result<...>` to `run(source: &str, seed: u32) -> Result<...>`. Thread `seed` into `Evaluator::new_with_seed`. Update the 3 existing call sites: `Player::new`, `Player::set_source`, and test helpers in `tests/m03.rs` / `tests/m04.rs` etc. — each callsite passes `seed=0` until US2 lands the parameterization.

- [X] T009 In `src/ui.rs`, change `Player::set_source(source: &str) -> String` to `Player::set_source(source: &str, seed: u32) -> String`. Add a new `Player::set_seed(seed: u32) -> String` method that re-runs the pipeline with the player's stored source and the new seed (re-uses the stored AST cache if source unchanged — defer caching optimization, just re-parse for now). Add `seed: u32` field to `StateSnapshot` and surface it in the serialized JSON.

- [X] T010 In `web/index.js`, update the existing `player.set_source(source)` call site (in `recompile`) to `player.set_source(source, currentSeed)`. Add module-level `let currentSeed = 0;`. Plumb `currentSeed` through `render(state)` — on every successful render, sync `currentSeed` from `state.seed` (single source of truth — VR-SF2). No UI changes yet; this is a behaviorally-transparent JS plumbing landing.

- [X] T011 [P] Run `cargo build` and confirm zero warnings, zero errors. Run `cargo test` and confirm 181 baseline tests still pass byte-identical (this is THE critical Phase 2 hygiene gate: scheduler infrastructure landed without breaking any existing behavior). Run `cd web && trunk build` and confirm dev build succeeds.

**Checkpoint**: scaffolding live, behaviorally transparent — all existing samples render identically; scheduler exists but only changes behavior when multi-Ready sets occur (which strict-deferred queues never produced).

---

## Phase 3: User Story 1 — Seeded scheduling produces interleaved trace (Priority: P1)

**Goal**: replace the strict "spawned thread runs to completion at end of stmt" pattern with cooperative statement-boundary scheduling + Mutex parking. Default seed `0` produces ONE specific deterministic interleaving across reloads. Existing M08 samples re-baseline.

**Independent Test**: Load `Arc<Mutex> (M08)`, step through, observe interleaving where the closure runs SOME steps before main reaches `h.join()`. Reload, observe same trace. Re-run with seed=0 in a test harness, byte-identical event stream.

### Implementation

- [ ] T012 [US1] In `src/eval.rs`, replace `run_queued_thread_to_done(tid)` with `run_queued_thread_one_stmt(tid) -> StepProgress` where `enum StepProgress { Continue, Done, ParkedLock(HeapAddr), ParkedJoin(ThreadId) }`. The new helper runs the thread for ONE statement (the current stmt in its body) and returns. State for "where am I in the body" is captured in `QueuedBody` via a `next_stmt_idx: usize` field (initialize to 0 on first run). On `ParkedLock`/`ParkedJoin`, do NOT increment `next_stmt_idx` — re-running the stmt is safe because the lock-check is the first observable action (R-003 constraint).

- [ ] T013 [US1] In `src/eval.rs`, modify `mutex_lock(addr, span)` to check `holder.is_some() && holder != Some(current)` BEFORE emitting any events. If contended: flip `self.threads[current].status` to `BlockedOnLock(addr)`, return early via a new internal `Result<Value, ParkSignal>` (or equivalent panicking-with-catch_unwind sentinel — pick one in research, default to `Result`). Caller in `run_queued_thread_one_stmt` catches the park signal and propagates as `ParkedLock(addr)`. Update the existing panic message ("M08 v1 doesn't support parking") to fire ONLY on the lock-pattern-restriction path (R-003): if `mutex.lock()` is detected mid-expression (not as RHS of let-binding), panic with the new message `M08.2 lock pattern restriction: wrap m.lock() in a let-binding before using the guard`.

- [ ] T014 [US1] In `src/eval.rs`, modify `join_thread(target, span)` to use the same parking model: if `target.status != Done`, flip `current.status` to `BlockedOnJoin(target)` and propagate `ParkedJoin(target)` up to the scheduler loop. Remove the existing "switch_to(target), run_queued_thread_to_done, switch_back" inline-run pattern — the scheduler now drives all thread progress.

- [ ] T015 [US1] In `src/eval.rs`, in the place currently calling `MutexGuard` drop (in `eval_stmt`'s scope-drop pass), after emitting `LockRelease`, scan `self.threads` for all entries with `status == BlockedOnLock(addr)` and flip them back to `Ready`. Symmetric: when a thread's status flips to `Done`, scan for `BlockedOnJoin(self_tid)` and flip those to `Ready`. Both scans iterate `self.threads` in `IndexMap` key order (deterministic).

- [ ] T016 [US1] In `src/eval.rs`, rewrite the top-level scheduler loop (currently the stmt-loop inside `eval_block` + the pending-runs drain in `eval_stmt`). New shape: a single loop that picks the next thread via `scheduler.pick(&ready_sorted)`, calls `run_queued_thread_one_stmt(tid)`, handles the `StepProgress` return, and terminates when all threads are `Done`. On `Ready` set empty + at least one `BlockedOn*` thread present, emit `MemEvent::Deadlock { thread_ids: blocked_ids_sorted, span: last_decision_span }` and break (R-005, FR-011).

- [ ] T017 [US1] In `src/eval.rs`, add pedagogical-Note emission for multi-Ready scheduler decisions (R-008): when `ready.len() > 1` before `pick`, emit `MemEvent::Note { kind: Info, message: format!("Scheduler picked thread #{tid} to advance next (seed={seed}, {n} other ready threads).", n = ready.len() - 1), span }` AFTER the pick but BEFORE handing off to the chosen thread. The Note coalesces with the first event of that thread's next stmt via existing rules. Skip emission when `ready.len() == 1` (forced choice, no pedagogy).

- [ ] T018 [US1] In `src/eval.rs`, on `LockRelease` when multiple threads were blocked on the same addr and all flip to Ready simultaneously, emit a pedagogical Note: `Lock released; the scheduler will pick which waiting thread acquires next.` (The actual pick happens on the next scheduler tick; this Note explains the unparking event itself.)

- [ ] T019 [US1] Re-baseline existing M08 snapshot tests. Run `INSTA_UPDATE=always cargo test --test m08` (or `tests/m08*`). Inspect the updated `.snap` files. Confirm the new traces are PLAUSIBLE — closure runs some steps before main's join, no duplicate event sequences, lock acquire/release pairs match. Commit the re-snapped traces with a clear message: "M08 re-baseline post-021: scheduler now picks via seed=0; strict-deferred order replaced with cooperative random." T020 / T021 add new tests that don't rely on the specific trace shape.

- [X] T020 [US1] In `tests/m08_2.rs`, add `#[test] fn same_seed_determinism()`: run the M08 Arc<Mutex> sample through `pipeline::run(source, 42)` twice; assert the two event streams are bytewise-identical (`assert_eq!` on `Vec<MemEvent>`). Covers B-M082-1, SC-003.

- [X] T021 [US1] In `tests/m08_2.rs`, add `#[test] fn single_thread_invariance()`: run an M01 sample (`let x = 42; let y = x + 1;`) through `pipeline::run(source, X)` for X in `[0, 1, 42, 4294967295]`; assert all four event streams are bytewise-identical. Covers B-M082-2, SC-002.

- [ ] T022 [US1] Run `cargo test` and confirm: 181 baseline tests pass; m08_2 target's `same_seed_determinism` + `single_thread_invariance` pass; M08 re-baseline snapshots are committed. Run `cd web && trunk serve` and confirm the M08 Arc<Mutex> sample renders interactively with the new interleaving.

- [ ] T023 [US1] Manual QA for US1: load M08 Arc<Mutex>, step through, confirm closure runs SOME steps before main's join (NOT the M08 v1 "everything at join" pattern). Reload, confirm byte-identical trace at every step. Confirm pedagogical Notes appear for scheduler decisions (e.g. "Scheduler picked thread #1 (seed=0, 1 other ready thread)").

**Checkpoint**: US1 fully functional. Default-seed traces are interleaved and reproducible. Existing samples' behavior changes once (re-baseline committed). Single-threaded samples byte-identical.

---

## Phase 4: User Story 2 — Learner can change the seed (Priority: P1)

**Goal**: expose seed input in the toolbar; typing a new seed re-runs the trace with that seed. Same seed gives same trace; different seed gives different trace.

**Independent Test**: enter seed 1, observe trace A. Enter seed 2, observe trace B. Verify A ≠ B (events differ in order or count). Verify seed 1 → trace A reproduces.

### Implementation

- [X] T024 [US2] In `web/index.html`, add to the `<footer id="toolbar">` between `btn-play-pause` and `step-indicator`: `<label for="seed-input" class="seed-label">seed</label>` + `<input id="seed-input" type="number" min="0" max="4294967295" value="0" step="1" />`. Per contracts/seed-ui-contract.md.

- [X] T025 [P] [US2] In `web/style.css`, add `.seed-label` (font ui-monospace, font-size 11px, color var(--muted), margin-left 1rem) + `#seed-input { width: 6em; padding: 2px 6px; font ui-monospace 12px; border: 1px solid var(--border); border-radius: 3px; background: white; }`. Per the styling contract.

- [X] T026 [US2] In `web/index.js`, add module-level `const SEED_DEBOUNCE_MS = 300; let seedDebounceTimer = null;` (currentSeed already added in T010). Add `wireSeedControls()` function and call it from `main()` after `wireControls()`. `wireSeedControls` attaches an `input` listener to `seed-input` that: clears `seedDebounceTimer`, sets a new timer for 300ms that parses `ev.target.value` as int, validates `0..=0xFFFFFFFF`, on invalid reverts `ev.target.value` to `String(currentSeed)` without re-running, on valid sets `currentSeed = raw` and calls `rerunWithSeed(raw)`.

- [X] T027 [US2] In `web/index.js`, add `function rerunWithSeed(seed)`: gets current source from `editorView.state.doc.toString()`, calls `player.set_source(source, seed)`, parses JSON, calls `render(state)` on ok or `renderError(error)` on failure. Reuses the existing pipeline-error UX from T010.

- [X] T028 [US2] In `web/index.js`, in `render(state)`, sync `document.getElementById("seed-input").value = String(state.seed);` AFTER all other render work (so the input field always matches the rendered trace's seed — VR-SF2, B-UI-5). Also update `currentSeed = state.seed`.

- [X] T029 [US2] In `web/index.js`, in `setControlsEnabled(enabled)`, also toggle `disabled` on `seed-input` (B-UI-9). Re-roll button is wired in Phase 5, not yet relevant.

- [ ] T030 [US2] In `tests/m08_2.rs`, add `#[test] fn different_seed_divergence()`: run the M08 Arc<Mutex> sample through `pipeline::run(source, X)` for X in `0..100`; collect event streams; assert at least 80 of the 100 distinct from the seed=0 trace (SC-001, B-M082-3). Use bytewise comparison via `Vec<MemEvent>` equality.

- [ ] T031 [US2] Manual QA for US2: load M08 Arc<Mutex>, enter seed `1`, wait 300ms, confirm re-render. Note step count + event order. Enter seed `2`, confirm DIFFERENT re-render. Enter `1` again, confirm matches first observation. Enter `-1` (invalid), confirm input reverts without re-render. Enter `4294967295` (max), confirm valid render. Tab navigation: click play button, tab into seed-input, tab into step-indicator (or whatever comes next) — confirm seed-input is in the tab order (B-UI-8).

**Checkpoint**: US2 fully functional. Learner can manually explore alternate schedules. The full pedagogical loop ("same code, many valid executions") works end-to-end.

---

## Phase 5: User Story 3 — Re-roll button (Priority: P2)

**Goal**: a 🎲 button in the toolbar generates a fresh seed and re-runs. One-click exploration of variants.

**Independent Test**: click 🎲, observe seed input updates to a new value AND trace re-renders. Click again, get a different value. Manually re-enter a previous value, reproduce that earlier trace.

### Implementation

- [X] T032 [US3] In `web/index.html`, add immediately after the `seed-input`: `<button id="btn-reroll-seed" type="button" aria-label="Generate new random seed" title="New random seed">🎲</button>`.

- [X] T033 [P] [US3] In `web/style.css`, add `#btn-reroll-seed { background: transparent; border: 1px solid var(--border); border-radius: 3px; padding: 2px 6px; font-size: 14px; cursor: pointer; line-height: 1; } #btn-reroll-seed:hover { background: var(--frame-bg); }`.

- [X] T034 [US3] In `web/index.js`, extend `wireSeedControls()` to also attach a `click` listener on `btn-reroll-seed`. The handler clears `seedDebounceTimer`, generates `const fresh = Math.floor(Math.random() * 0x1_0000_0000);`, sets `document.getElementById("seed-input").value = String(fresh);`, sets `currentSeed = fresh;`, and calls `rerunWithSeed(fresh)` IMMEDIATELY (no debounce — B-UI-3).

- [X] T035 [US3] In `web/index.js`, in `setControlsEnabled(enabled)`, also toggle `disabled` on `btn-reroll-seed` (B-UI-9).

- [ ] T036 [US3] Manual QA for US3: click 🎲 three times in quick succession. Confirm: (a) seed input value visibly updates each click, (b) trace re-renders each click, (c) each re-render shows a (typically) different interleaving. Note one of the seeds (e.g., `1742368512`); manually enter it later and confirm reproducibility (B-UI-3, US3 acceptance scenario 2).

**Checkpoint**: US3 fully functional. One-click exploration loop closes the pedagogical UX.

---

## Phase 6: Deadlock detection + sample (cross-cutting)

**Purpose**: deliver SC-007 (deadlock surfaces clearly) and FR-011 with a sample that actually exercises it. Independently testable.

- [ ] T037 In `web/samples/m08_2_deadlock.rs`, add a hand-crafted deadlock sample: two `Arc<Mutex<i32>>` (m1, m2); thread A locks m1 then m2; thread B locks m2 then m1. Under SOME seed, the scheduler will pick A-then-B which locks m1 then m2 successfully — no deadlock. Under OTHER seeds, A grabs m1 and yields, B grabs m2 and yields, A tries m2 (blocked), B tries m1 (blocked) — deadlock. Add the sample to `web/index.html`'s `<select id="sample-selector">` as `<option value="m08_2_deadlock">Deadlock (M08.2)</option>`.

- [ ] T038 In `src/ui.rs`, when serializing `MemEvent::Deadlock`, surface a status-bar message via the existing pedagogical-note + status pattern: `Deadlock: threads #{a}, #{b} waiting on each other's locks. The trace ends here.` Player stops at this step (no further steps).

- [ ] T039 In `web/index.js`, in `render(state)`, detect if the LAST event in the snapshot's events is `Deadlock` (or equivalently: `state.deadlock` flag exposed by the snapshot). Show the deadlock message in the status bar with a clear visual (e.g., status-bar class `status-deadlock` with red-ish background). Disable the step-forward button at the deadlock step.

- [ ] T040 In `tests/m08_2.rs`, add `#[test] fn deadlock_detection()`: run `m08_2_deadlock.rs` through `pipeline::run(source, deadlock_seed)` where `deadlock_seed` is hand-picked (or discovered by iterating seeds in the test until the trace ends with `Deadlock`); assert the trace's last event is `MemEvent::Deadlock { thread_ids, span }` with `thread_ids` containing both thread IDs. Cover B-M082-4, SC-007.

- [ ] T041 Manual QA for deadlock: load `Deadlock (M08.2)`. Enter several seeds (or click 🎲 repeatedly) until a deadlock occurs. Confirm: (a) status bar shows the deadlock message, (b) player stops at the last step, (c) step-back works to inspect prior state, (d) entering a different seed that DOESN'T deadlock makes the sample complete normally.

---

## Phase 7: Polish & cross-cutting

**Purpose**: bundle-size check, warnings hygiene, full QA, doc updates, commit prep.

- [ ] T042 [P] Run `cargo test` and confirm all tests pass: 181 baseline + new `m08_2` target tests (4 tests: same_seed_determinism, single_thread_invariance, different_seed_divergence, deadlock_detection). M08 snapshot re-baselines from T019 already committed.
- [ ] T043 [P] Build WASM release and measure bundle size: `cd web && trunk build --release`. Compare to the post-020 baseline (~440 KB). Acceptable if ≤ +5% (~462 KB). Expected delta: ~2-3 KB for the scheduler + ~30 bytes for the PRNG = ~3 KB total. Well under budget.
- [ ] T044 [P] Run `RUSTFLAGS="-D warnings" cargo build --release`. Confirm zero warnings (SC-009).
- [ ] T045 Full manual QA per `specs/021-randomized-scheduler/quickstart.md` — ~12-minute walk covering all 3 user stories + edge cases (seed 0, max, deadlock, single-thread invariance, lock-pattern-restriction error, regression sweep on M01–M07.7).
- [ ] T046 In `CLAUDE.md`, add a "Threading roadmap" sub-section under "Notes for Claude" briefly noting: M08 v1 (strict-deferred) replaced by 021 (seeded random scheduler with real Mutex parking, absorbed M08.1); deadlock detection live; memory-ordering modeling deferred to a future milestone (separate visualization design problem); Loom-style exhaustive search out of scope (pedagogical tool, not bug-finder).
- [ ] T047 Final commit prep. MR note: "Replace M08 v1's strict-deferred scheduling with a seeded random scheduler. Absorbs M08.1 (real Mutex parking + contention handling). Same seed → same trace (deterministic). Different seed → different valid interleaving (pedagogical payoff). New UI: seed input + 🎲 re-roll button + seed display. New MemEvent::Deadlock variant with detection + status-bar surfacing. Existing M08 snapshot tests re-baselined ONCE (deliberate behavior change). M01–M07.7 byte-identical (single-thread invariance). Bundle ≤ +5%. 181 baseline tests + 4 new m08_2 invariant tests."

---

## Dependencies

```text
Phase 1 (Setup)
  └─ T001 (baseline verify)
  └─ T002 (register m08_2 test target)

Phase 2 (Foundational) — blocks ALL user stories
  ├─ T003 (Prng struct)
  ├─ T004 (Scheduler substruct — depends on T003)
  ├─ T005 [P] (Deadlock event variant — independent of T003/T004 in eval)
  ├─ T006 [P] (BlockedOn* ThreadStatus variants — independent of T005)
  ├─ T007 (replace FIFO drain with scheduler pick — depends on T004)
  ├─ T008 (pipeline seed plumbing — depends on T004)
  ├─ T009 (Player API change — depends on T008)
  ├─ T010 (JS plumbing — depends on T009)
  └─ T011 [P] (Phase 2 hygiene gate — depends on T002-T010)

Phase 3 (US1) — depends on Phase 2
  ├─ T012 (run_queued_thread_one_stmt + StepProgress — depends on T007)
  ├─ T013 (mutex_lock parking — depends on T006, T012)
  ├─ T014 (join_thread parking — depends on T006, T012)
  ├─ T015 (unpark on LockRelease / Done — depends on T013, T014)
  ├─ T016 (top-level scheduler loop + deadlock — depends on T012-T015)
  ├─ T017 (pedagogical Notes on pick — depends on T016)
  ├─ T018 (Note on lock release unpark — depends on T015)
  ├─ T019 (re-baseline M08 snapshots — depends on T012-T018)
  ├─ T020 [US1] (same_seed_determinism test — depends on T016)
  ├─ T021 [US1] (single_thread_invariance test — depends on T016)
  ├─ T022 [US1] (verify + dev build — depends on T019-T021)
  └─ T023 [US1] (manual QA US1 — depends on T022)

Phase 4 (US2) — depends on Phase 3
  ├─ T024 (HTML seed input)
  ├─ T025 [P] (CSS seed input — independent of T024 logically; same file dep)
  ├─ T026 (JS wireSeedControls — depends on T024)
  ├─ T027 (rerunWithSeed — depends on T026)
  ├─ T028 (sync seed-input on render — depends on T027)
  ├─ T029 (disable seed-input on error — depends on T026)
  ├─ T030 [US2] (different_seed_divergence test — depends on T021)
  └─ T031 [US2] (manual QA US2 — depends on T028, T030)

Phase 5 (US3) — depends on Phase 4
  ├─ T032 (HTML re-roll button)
  ├─ T033 [P] (CSS re-roll button)
  ├─ T034 (JS re-roll handler — depends on T026, T032)
  ├─ T035 (disable re-roll on error — depends on T032)
  └─ T036 (manual QA US3 — depends on T034)

Phase 6 (Deadlock) — depends on Phase 3 (T016 emits Deadlock; UI render handled here)
  ├─ T037 (deadlock sample + dropdown entry)
  ├─ T038 (status-bar surfacing in ui.rs)
  ├─ T039 (JS deadlock render + disable step-forward)
  ├─ T040 (deadlock_detection test — depends on T016, T037)
  └─ T041 (manual QA deadlock — depends on T037-T039)

Phase 7 (Polish) — depends on Phases 3-6
  └─ T042–T047 (test/build/warnings/QA/docs/commit)
```

---

## Parallel execution opportunities

- **Phase 2**: T005 + T006 are file-disjoint (event.rs vs eval.rs ThreadStatus enum); T011 is hygiene-only [P].
- **Phase 4 / 5**: CSS-only tasks (T025, T033) [P] vs HTML+JS chains.
- **Phase 7**: T042 / T043 / T044 [P] (independent verification tasks).
- **Across user stories**: US2 and US3 share the toolbar UI — could be implemented in parallel but the seed-input HTML + handler is a prereq for the re-roll button's wiring (since re-roll updates the input). Strict sequence US2 → US3 is cleaner.

---

## Implementation strategy

**MVP scope** = **US1 only** (Phases 1+2+3). Lands the foundational scheduler refactor with the default seed=0. Existing M08 samples re-baseline once. Learner sees interleaved traces but can't change the seed yet — pedagogical value is partial (one new interleaving, not exploration). ~350-450 LOC.

**Incremental delivery**:
1. **MVP (US1)**: Phases 1+2+3. Default-seeded interleaved trace, reproducible. Single-thread byte-identical.
2. **+US2 (manual seed)**: Phase 4. Learner enters seed values, explores alternate schedules.
3. **+US3 (re-roll button)**: Phase 5. One-click exploration.
4. **+Deadlock (cross-cutting)**: Phase 6. Surfaces deadlock as a first-class outcome. Could land between US1 and US2 if a deadlock-aware sample is desired earlier.
5. **+Polish**: Phase 7. Bundle/warnings/QA/docs/commit.

**Recommended landing order**: ship Phases 1-7 as a single merge. The scheduler refactor (Phase 2-3) is the heaviest piece; splitting it from the UI affordances (Phase 4-5) would expose users to a re-baseline without the explanation (seed input) for why traces look different. Single-merge avoids the in-between explanation burden.

**Sequence note**: this absorbs M08.1 (real Mutex parking) by design — landing parking without random scheduling would deliver M08.1 with an artificially-deterministic order. The two go together.

**No UX checkpoint planned**: seed input + dice button are standard widgets and the pedagogical Notes wording can be tuned at QA time. If the toolbar placement reveals ambiguity at first use, a checkpoint can be inserted between Phase 4 and Phase 5.
