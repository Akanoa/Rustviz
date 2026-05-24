# Implementation Plan: M08 — Level 4: threads (`thread::spawn`, `Arc`, `Mutex`)

**Branch**: `019-m08-threads-arc-mutex` | **Date**: 2026-05-24 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/019-m08-threads-arc-mutex/spec.md`

## Summary

Introduce concurrency end to end: `thread::spawn` (with `move ||` closures), `Arc::new`/`clone`/drop (shared ownership with refcount), `Mutex::new`/`lock` (exclusive access with parked-thread visualization). **Closes Level 4** — after M08 ships, the entire CLAUDE.md Level 4 surface (M07 heap, M07.1 slices, M07.2 strings, M07.3 arrays, M07.4 structs, M07.5 generics, M07.6 traits, M07.7 trait objects, M08 concurrency) is complete.

**Headline pedagogy** — making **shared state under exclusive lock** visible: the canonical `Arc<Mutex<T>>` pattern with two stack columns side by side, two dashed-purple Arc arrows pointing at the shared mutex heap block, and a visible parked-thread state when contention happens. The headline US4 sample renders ALL M08 visualizations simultaneously.

**12th invocation of the closed-enum-with-revisions rule** — additive `Value::Arc`, `Value::Mutex`, `Value::MutexGuard`, new `ThreadId` newtype (analog of M07.7's `VtableAddr`), new `MemEvent::ThreadSwitch` variant (drives the UI's current-thread routing). The 7 thread+sync MemEvent variants pre-declared at M03 (`ThreadSpawn`, `ThreadJoin`, `ThreadPark`, `ArcClone`, `ArcDrop`, `LockAcquire`, `LockRelease`) finally get their payload semantics filled in — honors M03's "closed enum" promise: the variants existed all along, M08 emits them.

**Single-milestone strategy** — MILESTONES.md flags a possible M08a/M08b split if scope reveals XL during implementation; plan ships M08 as one milestone with TWO embedded UX checkpoints (one after multi-column stacks first cut, one after parked-thread + dashed-purple + refcount first cut). If iteration cost exceeds appetite mid-implementation, the user can call to defer US3/US4 to a follow-up M08b.

Authority chain: `MILESTONES.md` › M08 → `spec.md` (this feature) → this plan.

## Technical Context

**Language/Version**: Rust 2024 edition (same toolchain as M01–M07.7). No new toolchain requirements.
**Primary Dependencies**: existing `indexmap`, `serde`, `serde_json`, `wasm-bindgen`, `js-sys`, `console_error_panic_hook`. **No new Rust deps**. **No JS deps changes**.
**Storage**: in-memory; no new files. Thread state lives in `Evaluator.threads: IndexMap<ThreadId, ThreadState>` + `current_thread_id`. Arc refcount + Mutex holder state live in extended `HeapObject` variants (`HeapObject::Arc { value, strong_count }`, `HeapObject::Mutex { value, holder: Option<ThreadId> }`). M01/M02/M03 snapshot tests should stay byte-identical (additive Value variants + `ThreadSwitch` MemEvent variant + serde-default-empty on new fields preserves wire shape for non-threaded programs).
**Testing**: existing `cargo test --test m01 / m02 / m03` should pass byte-identical for non-threaded programs. New `cargo test --lib pipeline::tests` covering: basic spawn+join (multi-column visible in trace), Arc::clone + ArcDrop with refcount transitions, Mutex lock + LockRelease + parked-thread, Arc<Mutex<T>> end-to-end with contention, deterministic event ordering across runs, and at least 2 rejection tests (unknown captured binding, non-move closure if rejected at parse). **≥ 10 new tests**. Manual M08 QA per the quickstart procedure with TWO UX checkpoints (multi-column first cut, parked+dashed+refcount first cut).
**Target Platform**: same as M01–M07.7 (host + `wasm32-unknown-unknown`).
**Project Type**: Rust library + companion UI. Touches ~6 source modules (parse/{token, lexer, ast, parser} for `thread`/`Arc`/`Mutex`/closure surface, resolve, typeck, eval, ui) + JS for multi-column rendering + parked visual + dashed-purple arrows + CSS. Sized XL — comparable to or slightly larger than M07.4 (struct view) and M07.7 (trait objects).
**Performance Goals**: same pipeline latency budget. Multi-thread scheduling adds O(N_threads) overhead per yield-point lookup; trivial for the 2-3 thread samples M08 targets.
**Constraints**: M03 byte-identical for non-threaded programs; M01/M02 byte-identical; WASM bundle ≤ +25% vs M07.7 baseline (398,847 B → ≤ ~498 KB raw post-staged) per SC-011; zero warnings under `-D warnings` (SC-012); existing M01–M07.7 features preserved.
**Scale/Scope**: ~6 source modules + 3-4 sample pairs + ≥ 10 new unit tests + multi-column stacks UI overhaul. **Estimated ~1800-2200 LOC net change**. Sizing: **XL** per the rubric — comparable to or slightly larger than M07.7 (the multi-column stack rendering + parked-thread visual + new closure surface push the upper bound).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

`.specify/memory/constitution.md` is still the unfilled speckit template. Same vacuous PASS as features 001–018.

**Post-design re-check**: still vacuous, still PASS.

## Project Structure

### Documentation (this feature)

```text
specs/019-m08-threads-arc-mutex/
├── plan.md                          # This file
├── spec.md                          # Feature spec
├── research.md                      # Phase 0: 14 design decisions (R-008 = scheduler; R-014 = first UX checkpoint; R-018 = second UX checkpoint)
├── data-model.md                    # Phase 1: Value::Arc / Mutex / MutexGuard, ThreadId, ThreadState, HeapObject::Arc/Mutex, MemEvent::ThreadSwitch, ThreadColumnView, ParkedThreadView
├── quickstart.md                    # Phase 1: dev workflow + manual QA procedure with TWO UX checkpoints
├── contracts/
│   └── m08-protocol-delta.md        # Phase 1: 12th closed-enum invocation
└── checklists/
    └── requirements.md              # From /speckit-specify (16/16 PASS)
```

### Source Code (repository root) — files M08 touches

```text
src/
├── parse/
│   ├── token.rs                # MODIFIED — add `TokenKind::Move` for `move ||` closure keyword (verify if absent; needed for `thread::spawn(move || { ... })`). `Pipe` for `|`/`||` may also need adding if not present.
│   ├── lexer.rs                # MODIFIED — extend KEYWORDS with `"move"` → `TokenKind::Move`. Add `|`/`||` tokens (one-/two-char paths) if absent. `thread`, `Arc`, `Mutex` are identifiers (already lex as `Ident`); they only get path-call semantics in typeck/eval.
│   ├── ast.rs                  # MODIFIED — add `Expr::Closure { is_move: bool, body: Block, span: Span }` AST node for the minimal closure surface. Bound to `thread::spawn` arg site only (no general closure type inference; parser accepts the syntax everywhere but typeck only allows it as the `thread::spawn` arg). M08 doesn't add closure parameter syntax (`|x| { body }`) — only no-arg closures (`|| { body }`).
│   └── parser.rs               # MODIFIED — `parse_atom` extension: when seeing `Move` keyword followed by `||` (or just `||`), parse `Expr::Closure { is_move, body }`. The `||` two-char token IS `OrOr` from M01 — but in atom position it's parsed as "empty closure params" (lookahead disambiguates: `||` at atom start = closure params; `||` between two expressions = logical-or).
├── resolve.rs                  # MODIFIED — `resolve_expr` adds `Expr::Closure { body, .. }` arm. The closure body has its own scope; identifiers used inside that aren't declared locally are captured from the enclosing scope. Free-variable analysis: walk the body, record each `Expr::Ident` whose binding is in the enclosing scope (not in the closure's local scope). The captured set drives `move`-binding setup in eval.
├── typeck.rs                   # MODIFIED — recognize `thread::spawn(closure)` path-call pattern. Accept `Expr::Closure` ONLY as the `thread::spawn` arg (other positions → error "closures are only supported as `thread::spawn` arguments in M08"). The closure's body is typechecked with the captured bindings in its environment scope. The return type of `thread::spawn` is `Ty::JoinHandle` (new variant) — calling `.join()` on it returns `Ty::Unit`. Path-call recognition for `Arc::new(v)` → `Ty::Arc(Box<Ty>)`, `Arc::clone(&a)` → `Ty::Arc(_)` (matches arg's inner Ty), `Mutex::new(v)` → `Ty::Mutex(Box<Ty>)`. Method-call dispatch for `mutex.lock()` returns `Ty::MutexGuard(Box<Ty>)`; `*guard` derefs to the inner Ty (M06.1 deref machinery extended). `.join()` on `Ty::JoinHandle` returns `Ty::Unit`.
├── event.rs                    # MODIFIED — add `Value::Arc { addr }`, `Value::Mutex { addr }`, `Value::MutexGuard { addr }` (all carry the heap addr; the actual metadata — refcount, holder — lives in the HeapObject). Add `ThreadId(u32)` newtype (analog of `VtableAddr`). Add `MemEvent::ThreadSwitch { thread_id, span }` (NEW MemEvent variant — drives the UI's current-thread routing). The 7 pre-declared thread+sync MemEvent variants get their payload field semantics formalized (e.g. `ThreadPark.lock` typed as `HeapAddr` per M03's stub).
└── eval.rs                     # MODIFIED — **major surface**. Refactor `Evaluator.frames: Vec<Frame>` → `Evaluator.threads: IndexMap<ThreadId, ThreadState>` (each ThreadState wraps `frames: Vec<Frame>`, `status: ThreadStatus`, `closure_body: Option<&Block>` for queued threads). Add `current_thread_id: ThreadId` for "who's executing now". `Evaluator::new` creates the main thread (id 0). `thread::spawn(closure)` ENQUEUES a new ThreadState with status `Queued(body, captured_bindings)`. `.join()` on a JoinHandle SWITCHES to the joined thread (emits `ThreadSwitch`), runs it to completion, switches back. `Arc::new` allocates a `HeapObject::Arc { value, strong_count: 1 }`; `Arc::clone` increments the count + emits `ArcClone`. Scope-exit Drop on Arc emits `ArcDrop` + decrements count; only emits `HeapFree` when count reaches 0. `Mutex::new` allocates a `HeapObject::Mutex { value, holder: None }`; `mutex.lock()` either acquires (emits `LockAcquire`, sets `holder = Some(current_thread)`, returns guard) or parks (emits `ThreadPark`, switches to next ready thread via cooperative scheduler). Guard Drop emits `LockRelease` + clears holder. Cooperative scheduler: when current thread parks/joins, dequeue next ready thread; if all blocked, panic (shouldn't happen for well-formed samples). Captured-binding handling: at `move ||` closure construction, snapshot the captured bindings' VALUES into the queued thread's initial scope. When the queued thread runs, its initial frame pre-populates the captured bindings as locals (each a `SlotAlloc` + `SlotWrite` at thread start). The captured Arc bindings carry the Arc value (refcount NOT incremented at capture — `move` is a transfer of the binding, not a clone).

src/ui.rs                       # MODIFIED — **major surface**. Refactor `World.frames: Vec<FrameInProgress>` → `World.threads: IndexMap<ThreadId, ThreadColumnState>` (each ThreadColumnState wraps `frames: Vec<FrameInProgress>`, `parked_on: Option<u32>` heap-addr, `is_active: bool` for "this thread is currently executing"). Add `current_thread_id: ThreadId` updated from `MemEvent::ThreadSwitch` events. Apply_event routes frame/slot events to the current thread. Add `ThreadSpawn` arm to create a new ThreadColumnState; `ThreadJoin` arm to mark column closed; `ThreadPark` arm to set `parked_on = Some(heap_addr)`; `LockAcquire` to update HeapAllocState's holder display; `LockRelease` to clear it AND clear any parked thread that was waiting on it; `ArcClone` to bump the HeapAllocState's refcount display; `ArcDrop` to decrement. Snapshot's `frames` field becomes `threads: Vec<ThreadColumnView>` carrying per-column state. New `ParkedThreadView { thread_id, parked_on_heap_addr }` for the dotted-line viz. Existing single-thread programs render with one thread column (id 0) — back-compat preserved.

tests/
├── m01.rs / m02.rs / m03.rs        # Should stay byte-identical (no existing sample constructs threads / Arc / Mutex).
└── samples/
    ├── (existing)                  # Unchanged.
    └── m08_*.rs                    # NEW (3-4 files): m08_thread_spawn (US1), m08_arc_clone (US2), m08_mutex_contention (US3 — explicit two-thread Mutex contention), m08_arc_mutex (US4, the headline).

web/
├── samples/                    # MODIFIED — add 3-4 m08_*.rs mirrors.
├── index.html                  # MODIFIED — dropdown grows 3-4 entries (m08 entries placed LAST, after m07_7_*). Stacks `<section id="stacks">` needs its inner container changed to a horizontal flex (one child per thread column) — the existing `flex-direction: column-reverse` moves to the inner per-column `<div class="thread-column">` divs.
├── index.js                    # MODIFIED — `renderStacks` overhauled to render `state.threads` as N columns (one per ThreadColumnView). Each column gets its own `<div class="thread-column" data-thread-id="{id}">` containing the existing per-frame rendering. Parked thread columns get `.thread-parked` class; dotted-line render iterates `state.parked_threads` and draws lines from each parked column's header to the corresponding mutex's heap-block element. Arc-arrow rendering in `renderArrows` adds a new `ArrowKind::Arc` case (dashed purple, hover-only per the post-M07.7 Rule 1).
├── style.css                   # MODIFIED — `.thread-column` (per-thread vertical stack container; equal-width with horizontal scroll past 3), `.thread-parked` (low-opacity treatment + dotted-line origin styling), `.parked-line` (dotted purple SVG line class). `.arrow-arc` (dashed purple — distinct from M07.7's orange dispatch). `.heap-refcount` (small `[refs: N]` annotation appended to heap-block addr line). Multi-column flex container rules for `#stacks` + slide-in animation on column add (CSS `@keyframes` from `transform: translateX(100%)` to `translateX(0)`).
└── Trunk.toml                  # Unchanged.

# M03's contract amended for the 12th closed-enum invocation:
specs/004-m03-event-eval/contracts/m03-api.md   # MODIFIED — note M08 as the 12th invocation. Adds `Value::Arc`, `Value::Mutex`, `Value::MutexGuard`, `ThreadId`, `MemEvent::ThreadSwitch`. Pure-additive Value variants + new addressing newtype + one new MemEvent variant. The 7 pre-declared thread+sync MemEvent variants finally get their payloads emitted (no shape change to the variant declarations).
```

**Structure Decision**: substantially XL surface. Two iterative UI pieces (multi-column stacks; parked-thread visual + dashed-purple + refcount) each warrant a UX checkpoint mid-implementation. Eval refactor (single-thread `Vec<Frame>` → multi-thread `IndexMap<ThreadId, ThreadState>`) is the most invasive change since M07's heap introduction — touches every frame/scope/slot path. UI refactor mirrors at the snapshot level. Parser closure surface is minimal (no-arg, body-only, optional `move`).

## Complexity Tracking

> No constitutional violations. Table omitted.

### Notable non-trivial complexity

- **Eval thread refactor**: every existing `self.frames.last_mut()` / `self.frames.push()` / etc. needs to route through `self.threads[self.current_thread_id].frames`. Touch surface is wide — ~15-20 call sites in eval.rs. Risk: missing one path leaves state going to the wrong thread silently. Mitigation: small `current_thread()` / `current_thread_mut()` accessor methods that panic if `current_thread_id` is missing from the map.
- **Cooperative scheduler**: when current thread parks/joins, the scheduler picks the next ready thread. Rule: FIFO by spawn order, skip blocked threads, panic if all blocked (deadlock — shouldn't happen for well-formed M08 samples). State machine: `Running` → `Parked(HeapAddr)` (on lock contention) → `Running` (on lock release); `Running` → `Joined` (on body completion). M08 doesn't model true concurrency; it's sequential interleaving.
- **`move ||` closure capture analysis**: free-variable scan over the closure body. For each `Expr::Ident` use whose resolved binding is OUTSIDE the closure's local scope, record it as a captured binding. At eval time, snapshot the captured bindings' values into the queued thread's initial scope. Subtle: nested scopes inside the closure body (`let x = ...; { let y = ...; }`) — only outer-binding references count as captures.
- **`Arc` refcount semantics**: `Arc::clone(&a)` returns a NEW `Value::Arc { addr }` with the SAME addr as `a`'s; the HeapObject's `strong_count` increments. `ArcDrop` decrements; HeapFree fires ONLY when count reaches 0. The drop path in `drop_current_scope` needs an Arc-specific branch: read HeapObject's count, decrement, emit ArcDrop + (maybe) HeapFree. The existing `Value::Box/Vec/String` Drop path always emits HeapFree unconditionally — Arc needs a count-aware variant.
- **Mutex state semantics**: HeapObject::Mutex carries `holder: Option<ThreadId>`. `lock()` checks holder: None → acquire (set holder, emit LockAcquire, return guard); Some(other) → park (emit ThreadPark, change current thread's status, switch to next ready thread, recurse). Guard Drop releases lock + unparks ALL parked threads on that mutex (in practice M08's contention sample has one parker; multi-parker semantics are: first-come-first-served by spawn order).
- **Mutex acquired through Arc**: `let m = Arc::new(Mutex::new(0)); let g = m.lock();` — `m` is `Value::Arc`; method-call dispatch needs to AUTO-DEREF through Arc to reach the inner Mutex. M07.6/M07.7 method-call dispatch already auto-derefs `&T` / `&mut T`; extend to `Arc<T>` and `Mutex<T>` (the latter for `.lock()` itself which is on Mutex, not Arc).
- **Multi-column stacks UI**: existing `#stacks` has `flex-direction: column-reverse` so innermost frame is on top. With multi-column, the OUTER container becomes a horizontal flex (`flex-direction: row`) and each column-wrapper retains the column-reverse rule for frames. Slide-in animation: CSS keyframe from `translateX(100%)` to `translateX(0)` triggered on column add. JS sets the keyframe class on first render; CSS handles the animation.
- **Parked-thread dotted line**: SVG overlay path from the parked column's header to the held mutex's heap block. Geometry: source = column's top-left corner (or column header bbox); target = mutex's heap-block bbox. Stroke: dotted purple, distinct from M07.7's dashed orange. Lifecycle: visible while the thread is parked; removed when LockRelease fires. Same hover-only treatment as other arrows? Probably not — parked-thread state is the headline pedagogy of US3, the dotted line should be always-on while the parking persists. UX checkpoint to confirm.
- **Refcount on heap block**: `HeapView` gains optional `refcount: Option<u32>` field. JS renders as `[refs: N]` suffix on the addr line (`heap #0 [refs: 2]`). Updates on ArcClone (increment) / ArcDrop (decrement).
- **`MemEvent::ThreadSwitch`**: drives the UI's current-thread routing. Emitted by eval whenever the cooperative scheduler picks a different thread. The UI updates `current_thread_id` from this event AND uses it to route subsequent SlotAlloc/SlotWrite/FrameEnter events to the right column. Adding ThreadSwitch is the 12th closed-enum invocation (one new variant since M07.7's `VtableAlloc`).
- **`HeapObject::Arc` shape**: `{ value: Value, strong_count: u32 }`. Cloning is shallow — the wrapped value lives in the HeapObject; multiple `Value::Arc` instances share the same addr. Sizing for the heap block: 16 bytes (8 for the wrapped value's ptr + 8 for the count metadata — pedagogical approximation).
- **`HeapObject::Mutex` shape**: `{ value: Value, holder: Option<ThreadId> }`. Lock state transitions live here; events emit on transitions.
- **Bundle growth ≤ +25%**: estimated +60-100 KB from Value/HeapObject/MemEvent additions + multi-column UI overhaul + parked viz + dashed-purple Arc arrows + refcount display. Verify post-merge.
- **TWO UX checkpoints expected**:
  1. After US1 first cut (multi-column stacks, slide-in animation). Pause for layout review.
  2. After US3 first cut (parked viz, dashed-purple arrows, refcount display). Pause for color/layout review.
- **Possible mid-implementation split**: per MILESTONES.md, if US1 alone proves XL, defer US2/US3/US4 to a follow-up M08b. The plan stages everything in one go but the user can call to split after the first UX checkpoint.
- **Tests strategy**: deterministic event ordering across runs (FR-013/SC-010) is a NEW test class — run the pipeline twice on the same source, assert byte-identical `Vec<MemEvent>`. Easy to write; catches scheduler non-determinism early.
- **Backward compat for single-threaded programs**: all M01-M07.7 samples don't construct threads. The eval's `Evaluator::new` creates main thread (id 0); single-threaded programs never spawn, so they stay in main forever. UI's apply_event routes everything to thread 0's column. M01/M02/M03 snapshots stay byte-identical (no new events fire; new MemEvent::ThreadSwitch never appears).
