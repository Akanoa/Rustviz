---

description: "Task list for M08 — Level 4: threads (`thread::spawn`, `Arc`, `Mutex`)"
---

# Tasks: M08 — Level 4: threads (`thread::spawn`, `Arc`, `Mutex`)

**Input**: Design documents from `/specs/019-m08-threads-arc-mutex/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/m08-protocol-delta.md ✓, quickstart.md ✓

**Tests**: M01/M02/M03 snapshots stay byte-identical for non-threaded programs (additive `Value`/`Ty`/`MemEvent` variants + AST `Expr::Closure` + new `ThreadId` newtype; no existing sample constructs threads/Arc/Mutex/closures). New `cargo test --lib pipeline::tests` covering: spawn+join visible, Arc clone + refcount transitions, Arc last-drop frees, Mutex lock + release, Mutex contention (parked-thread), Arc<Mutex<T>> end-to-end, deterministic event ordering across runs, closure-outside-spawn rejection, non-move closure rejection, Arc::clone on non-Arc rejection. **≥ 10 new tests**. Manual M08 QA per the quickstart procedure with **TWO** explicit UX checkpoints.

**Organization**: 4 user stories (US1+US2+US3 P1 foundational; US4 P2 — the canonical `Arc<Mutex<T>>` headline). Sized XL — comparable to or slightly larger than M07.7 (the multi-column UI refactor + parked viz + dashed-purple Arc arrows + refcount display + new closure surface push the upper bound).

**TWO UX CHECKPOINTS**:
- 🎨 **#1 after Phase 3 US1 first cut**: multi-column stacks layout + slide-in animation.
- 🎨 **#2 after Phase 5 US3 first cut**: parked-thread visual + dotted line + dashed-purple Arc arrows + refcount display.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1/US2/US3/US4 tag, mandatory on user-story phases only
- File paths relative to repo root

## Path Conventions

~6 existing source files modified (eval.rs is the most-touched, ui.rs the second-most) + new closure surface in parse/ + 4 sample pairs. See `specs/019-m08-threads-arc-mutex/plan.md` Project Structure.

---

## Phase 1: Setup

**Purpose**: pre-flight — confirm starting state.

- [X] T001 Verify pre-conditions: branch `019-m08-threads-arc-mutex` checked out; `cargo test` from `main` passes (baseline 173 tests post-M07.7); WASM bundle baseline 398,847 B noted.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Closure surface + `ThreadId` + `MemEvent::ThreadSwitch` + `Ty`/`Value`/`HeapObject` additions + eval thread refactor (single-stack `Vec<Frame>` → multi-thread `IndexMap<ThreadId, ThreadState>`) + UI snapshot refactor (`StateSnapshot.frames` → `StateSnapshot.threads: Vec<ThreadColumnView>`). The eval+UI refactors are the most invasive changes since M07's heap introduction — they touch every frame/scope/slot path. Single-pass to keep the diff coherent; all subsequent user-story phases build on this.

- [X] T002 Amend M03's contract in `specs/004-m03-event-eval/contracts/m03-api.md` — append entry under the closed-enum-with-revisions section noting M08 as the 12th invocation (additive `Ty::Arc/Mutex/MutexGuard/JoinHandle` + `Value::Arc/Mutex/MutexGuard/JoinHandle` + `ThreadId` + `MemEvent::ThreadSwitch` + payload-formalization of the 7 pre-declared thread+sync MemEvent variants + AST `Expr::Closure`). Reference `specs/019-m08-threads-arc-mutex/contracts/m08-protocol-delta.md`.

- [X] T003 [P] In `src/parse/lexer.rs`, extend KEYWORDS with `"move"` → `TokenKind::Move`. `thread`, `Arc`, `Mutex` stay as plain `TokenKind::Ident` (path-call semantics are typeck/eval-side). Verify the existing `||` two-char `OrOr` token also serves as empty closure params (no lexer change needed — parser-side disambiguates by position).

- [X] T004 [P] In `src/parse/token.rs`, add `TokenKind::Move` variant. Update `TokenKind::describe()` to return `"`move`"`.

- [X] T005 In `src/parse/ast.rs`, add `Expr::Closure { is_move: bool, body: Block, span: Span }`. Update `Expr::span()` to cover `Closure { span, .. } => *span`.

- [X] T006 In `src/parse/parser.rs`, extend `parse_atom`:
  - When seeing `Move` keyword, consume it AND expect `OrOr` (the `||` empty closure params), parse a `Block` body, build `Expr::Closure { is_move: true, body, span }`.
  - When seeing `OrOr` at atom-position-start (not after an operand), parse a `Block` body, build `Expr::Closure { is_move: false, body, span }`. Existing infix `OrOr` handling runs in the binary-op loop AFTER `parse_atom` returns — atom-position `||` is grammar-distinct.

- [X] T007 In `src/resolve.rs`:
  - Add a `closure_captures: IndexMap<Span, Vec<BindingId>>` side-table to `Resolution` (serde-default-empty so existing snapshots stay byte-identical).
  - Add an `Expr::Closure { body, span, .. }` arm to `resolve_expr`: push a fresh scope, walk `body`, recording each `Expr::Ident` whose resolved binding is OUTSIDE the closure's own scope as a captured binding. Pop the scope. Insert the captures into `closure_captures[span]`.

- [X] T008 In `src/event.rs`, add:
  - `ThreadId(pub u32)` newtype (analog of `VtableAddr` — `#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]`).
  - `Value::Arc { addr: HeapAddr }`, `Value::Mutex { addr: HeapAddr }`, `Value::MutexGuard { addr: HeapAddr }`, `Value::JoinHandle { thread_id: ThreadId }` variants.
  - `MemEvent::ThreadSwitch { thread_id: ThreadId, span: Span }` variant.
  - Update `Value::type_name()` for new variants (return `"Arc"`, `"Mutex"`, `"MutexGuard"`, `"JoinHandle"`).
  - Update exhaustive `Value` matches (eval.rs, ui.rs) — compiler will flag them.
  - Update exhaustive `MemEvent` matches (eval.rs, ui.rs, tests/m03.rs) — handle `VtableAlloc`-style new variant.

- [X] T009 In `src/typeck.rs`, add `Ty::Arc(Box<Ty>)`, `Ty::Mutex(Box<Ty>)`, `Ty::MutexGuard(Box<Ty>)`, `Ty::JoinHandle` variants. Update `Ty::name()` (returns `"Arc<T>"` / `"Mutex<T>"` / `"MutexGuard<T>"` / `"JoinHandle"`). Update `Ty::is_copy()` — all four return `false`. Update every existing `Ty` exhaustive match (typeck.rs internal helpers, eval.rs apply_subst_ty + ty_size_bytes, ui.rs ty_size_bytes_ui) — typically adding `Ty::Arc(_) | Ty::Mutex(_) | Ty::MutexGuard(_) | Ty::JoinHandle => { .. }` arms.

- [X] T010 In `src/typeck.rs`, extend `ty_from_ast_resolving_structs` to lower:
  - `Type::Generic { "Arc", [T] }` → `Ty::Arc(Box::new(ty_from_ast_resolving_structs(T)))`
  - `Type::Generic { "Mutex", [T] }` → `Ty::Mutex(...)`
  - `Type::Path { ["JoinHandle"], .. }` → `Ty::JoinHandle` (bare path lookup — JoinHandle is typically inferred, not annotated)
  - `Type::Generic { "MutexGuard", [T] }` → `Ty::MutexGuard(...)` (rarely annotated; typeck mostly produces it as method-call return type)

- [X] T011 In `src/typeck.rs`, extend `typecheck_path_call` with new path-callee entries:
  - `Arc::new(v: T) -> Arc<T>` — single arg, inner T inferred from arg's typecheck
  - `Arc::clone(arc: &Arc<T>) -> Arc<T>` — single arg must typecheck to `Ty::Ref { Ty::Arc(_), .. }`; return matches inner Arc
  - `Mutex::new(v: T) -> Mutex<T>` — single arg, inner T inferred
  - `thread::spawn(f: Closure) -> JoinHandle` — accepts only `Expr::Closure` (any other arg type → typeck error)

- [X] T012 In `src/typeck.rs`, extend `typecheck_method_call` with new built-in entries:
  - `(Ty::Mutex(inner), "lock")` → `Ty::MutexGuard(inner.clone())`
  - `(Ty::JoinHandle, "join")` → `Ty::Unit`
  - **Auto-deref through Arc**: when receiver is `Ty::Arc(inner)`, recurse-dispatch on `inner` (so `arc_of_mutex.lock()` works).
  - **`*guard` deref**: extend M06.1's typecheck_expr_inner Deref arm — when inner is `Ty::MutexGuard(inner_t)`, return `inner_t`.

- [X] T013 In `src/typeck.rs`, extend `typecheck_expr_inner` with `Expr::Closure { body, span, .. }` arm:
  - Reject by default with "closures are only supported as `thread::spawn` arguments in M08".
  - Closures embedded in `thread::spawn(closure)` are handled by `typecheck_path_call` (T011): typecheck the body within an environment containing the captured bindings' types, return `Ty::JoinHandle`.
  - Closure body typeck uses `Resolution.closure_captures[closure_span]` to seed the body's local-scope binding-types from the captures (no special-handling — captured bindings get their existing recorded type, just made visible inside the closure scope).
  - `move` requirement: typeck rejects non-`move` closures with "thread::spawn requires a `move` closure in M08".

- [X] T014 In `src/eval.rs`, add new HeapObject variants:
  - `HeapObject::Arc { value: Box<Value>, strong_count: u32 }`
  - `HeapObject::Mutex { value: Box<Value>, holder: Option<crate::event::ThreadId>, waiters: Vec<crate::event::ThreadId> }`
  - Update `heap_object_bytes(obj)` for both new variants — Arc returns `(8 + 4, 8 + 4)` (pointer + count); Mutex returns `(8 + 8, 8 + 8)` (pointer + lock state, generous).

- [X] T015 In `src/eval.rs`, refactor `Evaluator`:
  - Replace `frames: Vec<Frame>` with `threads: indexmap::IndexMap<crate::event::ThreadId, ThreadState<'a>>` (IndexMap preserves spawn order).
  - Add `current_thread_id: crate::event::ThreadId` field.
  - Add `next_thread_id: u32` counter.
  - Add helper `current_thread(&self) -> &ThreadState<'a>` / `current_thread_mut(&mut self) -> &mut ThreadState<'a>` that panic if `current_thread_id` is missing.
  - Define `struct ThreadState<'a> { frames: Vec<Frame>, status: ThreadStatus<'a>, queued_body: Option<QueuedBody<'a>> }`.
  - Define `enum ThreadStatus<'a> { Ready, Running, Parked { lock: HeapAddr }, JoinWait { target: ThreadId }, Done, _Phantom(PhantomData<&'a ()>) }`.
  - Define `struct QueuedBody<'a> { body: &'a ast::Block, captures: Vec<(String, Value, Ty)>, span: Span }`.
  - In `Evaluator::new`, initialize threads with `ThreadId(0)` (status `Running`); `current_thread_id = ThreadId(0)`; `next_thread_id = 1`.

- [X] T016 In `src/eval.rs`, refactor every `self.frames.last_mut()` / `self.frames.push()` / `self.frames.pop()` / `self.frames.last()` / `self.frames.iter()` etc. to route through `self.current_thread_mut().frames` / `self.current_thread().frames`. Touch surface: ~15-20 call sites. **Critical**: use the `current_thread()` / `current_thread_mut()` accessors uniformly; don't bypass them via direct `self.threads[id]` indexing inside hot paths. Verify with `cargo test` — M01-M07.7 baseline tests must stay green (every single-threaded program runs in thread 0 throughout, so the refactor must be behaviorally transparent).

- [X] T017 In `src/eval.rs`, add cooperative scheduler helper:
  - `fn switch_to(&mut self, target_thread: ThreadId, switch_span: Span)`: emits `MemEvent::ThreadSwitch { thread_id: target_thread, span: switch_span }` AND updates `self.current_thread_id = target_thread`.
  - `fn schedule_next_ready(&mut self, blocking_span: Span) -> Option<ThreadId>`: scans `self.threads` IndexMap in spawn order, finds first thread with status `Ready` or status `Running` post-unpark; emits ThreadSwitch + returns the picked id. None if all threads blocked (deadlock — panic with diagnostic message).

- [X] T018 In `src/eval.rs`, extend `eval_path_call` for thread/Arc/Mutex:
  - `["thread", "spawn"]`: evaluate `args[0]` — must be `Expr::Closure`. Allocate `ThreadId(next_thread_id++)`. Build captures from `Resolution.closure_captures[closure_span]` by reading current bindings' values. Insert a `Ready` ThreadState with `queued_body = Some(QueuedBody { body, captures, span })`. Emit `MemEvent::ThreadSpawn { thread_id, span }`. Return `Value::JoinHandle { thread_id }`.
  - `["Arc", "new"]`: evaluate `args[0]` → Value. Allocate `HeapObject::Arc { value: Box::new(v), strong_count: 1 }`; emit `MemEvent::HeapAlloc`. Return `Value::Arc { addr }`. The Arc's HeapView should carry `refcount: Some(1)` from this allocation onwards (UI handles via apply_event).
  - `["Arc", "clone"]`: evaluate `args[0]` (must be `Value::Ref { target: Slot(_), .. }` where the slot holds `Value::Arc { addr }`). Look up the addr; increment HeapObject's `strong_count`; emit `MemEvent::ArcClone { addr, span }`. Return a new `Value::Arc { addr }` (same addr as the source).
  - `["Mutex", "new"]`: evaluate `args[0]` → Value. Allocate `HeapObject::Mutex { value: Box::new(v), holder: None, waiters: vec![] }`; emit `MemEvent::HeapAlloc`. Return `Value::Mutex { addr }`.

- [X] T019 In `src/eval.rs`, extend `eval_method_call` for `Value::Mutex` / `Value::JoinHandle` / `Value::Arc` (auto-deref):
  - `(Value::Mutex { addr }, "lock")`: check HeapObject's holder:
    - `None`: set `holder = Some(current_thread_id)`, emit `MemEvent::LockAcquire { addr, span }`, return `Value::MutexGuard { addr }`.
    - `Some(other)`: push current_thread_id to waiters, emit `MemEvent::ThreadPark { thread_id: current_id.0, lock: addr, span }`, set current thread's status to `Parked { lock: addr }`, call `schedule_next_ready(span)`. The scheduler switches; when this thread is later unparked, the `lock()` call returns the guard (eval-side: a "retry" mechanism — the unparking path sets holder + emits LockAcquire + the parked thread's `lock()` Future completes with the guard).
  - `(Value::JoinHandle { thread_id: target }, "join")`: check target's status. If `Done`, emit `MemEvent::ThreadJoin { thread_id: target.0, span }`, return `Value::Unit`. Otherwise, set current thread's status to `JoinWait { target }`, call `schedule_next_ready(span)`. When the target completes and switches back, this thread resumes and emits ThreadJoin.
  - **Auto-deref through Arc**: when recv is `Value::Arc { addr }`, look up the HeapObject's `value` field, recursively dispatch on that value. So `arc_of_mutex.lock()` → auto-deref Arc → Mutex.lock().

- [X] T020 In `src/eval.rs`, add helper `start_queued_thread(&mut self, tid: ThreadId)`:
  - Takes the thread's `queued_body` (transitions Option to None).
  - Pushes a fresh outer scope onto its `frames`.
  - For each capture `(name, value, ty)`, emit `SlotAlloc { slot_id, name, ty, span: queued.span }` + `SlotWrite { slot_id, value, span: queued.span }`; push a LocalSlot into the outer scope.
  - Sets status to `Running`.
  - Called by `switch_to` when the target is in `Ready` state.

- [X] T021 In `src/eval.rs`, extend `drop_current_scope` for `Value::Arc` / `Value::MutexGuard` / `Value::Mutex` / `Value::BoxDyn`:
  - For `Value::Arc { addr }`: emit `MemEvent::ArcDrop { addr, span: end_span }`; decrement HeapObject's `strong_count`; if count reaches 0, also emit `HeapFree` + remove from heap.
  - For `Value::MutexGuard { addr }`: emit `MemEvent::LockRelease { addr, span: end_span }`; clear HeapObject's `holder`. If waiters non-empty, pop first waiter, set holder to waiter's tid, emit `LockAcquire { addr, span: end_span }` for the unparked thread. Change unparked thread's status from `Parked` to `Running` (the scheduler will pick it on next switch).
  - For `Value::Mutex { addr }` (direct heap-owning, not wrapped in Arc): emit `HeapFree` + remove from heap (just like Box).
  - For `Value::BoxDyn { addr, .. }`: existing M07.7 behavior (HeapFree + remove). Verify still correct after refactor.

- [X] T022 In `src/eval.rs`, refactor `evaluate(...)` (or the entry-point that calls main):
  - Initialize main thread (id 0); evaluate main fn normally.
  - After main completes, run the scheduler: while any thread has status `Ready`, switch to it, run it to `Done`. Loop until all threads are `Done`. This handles spawned threads whose join is implicit-via-fall-off-end-of-program.

- [X] T023 In `src/ui.rs`, add UI types:
  - `pub struct ThreadColumnView { pub thread_id: u32, pub label: String, pub frames: Vec<FrameCardView>, pub is_current: bool, pub status: ThreadStatusView }`
  - `pub enum ThreadStatusView { Running, Parked { lock_addr: u32 }, Joined, Ready }`
  - `pub struct ParkedThreadView { pub thread_id: u32, pub lock_heap_addr: u32 }`
  - Extend `HeapView` with `refcount: Option<u32>` (serde-default-skip-if-none) and `mutex_holder: Option<u32>` (serde-default-skip-if-none).
  - Replace `StateSnapshot.frames` with `pub threads: Vec<ThreadColumnView>` (always non-empty; thread 0 = main).
  - Add `pub parked_threads: Vec<ParkedThreadView>` (serde-default-skip-if-empty) to `StateSnapshot`.

- [X] T024 In `src/ui.rs`, refactor `World`:
  - Replace `frames: Vec<FrameInProgress>` with `threads: indexmap::IndexMap<u32, ThreadColumnState>`.
  - Define `struct ThreadColumnState { thread_id: u32, label: String, frames: Vec<FrameInProgress>, status: ThreadStatusView }`.
  - Add `current_thread_id: u32` field (defaults to 0).
  - In `World::default()`, initialize `threads` with thread 0 (main) and empty frames; `current_thread_id = 0`.

- [X] T025 In `src/ui.rs`, refactor `apply_event`:
  - All FrameEnter / FrameLeave / SlotAlloc / SlotWrite / SlotDrop / ReturnValue arms route through `world.threads.get_mut(&world.current_thread_id).unwrap().frames` instead of `world.frames`. Use a helper `current_frames_mut(&mut World) -> &mut Vec<FrameInProgress>` to keep the change minimal.
  - Add `MemEvent::ThreadSwitch { thread_id, .. }` arm: update `world.current_thread_id = thread_id.0`.
  - Add `MemEvent::ThreadSpawn { thread_id, .. }` arm: insert a new ThreadColumnState into `world.threads` with status `Ready`, label `format!("thread #{}", thread_id)` (or `"main"` for id 0).
  - Add `MemEvent::ThreadJoin { thread_id, .. }` arm: set the target thread's status to `Joined`.
  - Add `MemEvent::ThreadPark { thread_id, lock, .. }` arm: set the target thread's status to `Parked { lock_addr: lock.0 }`.

- [X] T026 In `src/ui.rs`, extend `apply_event` arms for new heap events:
  - `MemEvent::ArcClone { addr, .. }`: find HeapAllocState with addr, increment its (new) refcount field. Surface via `HeapView.refcount`.
  - `MemEvent::ArcDrop { addr, .. }`: find HeapAllocState, decrement refcount. Surface change in HeapView.refcount.
  - `MemEvent::LockAcquire { addr, .. }`: find HeapAllocState, set its (new) `mutex_holder = Some(world.current_thread_id)`. Surface via `HeapView.mutex_holder`.
  - `MemEvent::LockRelease { addr, .. }`: clear mutex_holder. Also clear any parked thread waiting on this addr (status `Parked { lock_addr: addr }` → `Running`).
  - Adapt `HeapAllocState` struct to carry `refcount: Option<u32>` + `mutex_holder: Option<u32>` fields.

- [X] T027 In `src/ui.rs`, refactor `state_snapshot`:
  - Build `threads: Vec<ThreadColumnView>` from `world.threads` IndexMap (preserves spawn order). Each `ThreadColumnView.frames` comes from the column's frames mapped via `frame_to_view`. `is_current` = `(thread_id == world.current_thread_id)`.
  - Build `parked_threads: Vec<ParkedThreadView>` by walking `world.threads` for entries with status `Parked { lock_addr }` — one entry per `(thread_id, lock_addr)` pair.
  - Build `HeapView` entries with the new `refcount` + `mutex_holder` fields populated from HeapAllocState.

- [X] T028 Run `cargo test` — verify all 173 M01-M07.7 baseline tests still pass byte-identical. The refactor MUST be behaviorally transparent for single-threaded programs (no spawn events, no ThreadSwitch events, no new MemEvent variants emitted; `threads.len() == 1` with thread 0 carrying the existing frame data; UI renders one column visually identical to pre-M08). Snapshot tests covered automatically (no source diff in M01/M02/M03 snapshots).

**Checkpoint**: `cargo build` clean; `cargo test` 173 passing (baseline preserved). The pipeline now accepts the closure surface + path-call additions + new Value/Ty/MemEvent variants but doesn't yet exercise them (no UI for multi-column, no Arc/Mutex/dispatch yet). All M01-M07.7 programs render unchanged.

---

## Phase 3: User Story 1 — `thread::spawn` + `.join()` (multi-column stacks) (Priority: P1) 🎯 MVP + 🎨 UX CHECKPOINT #1

**Goal**: `let h = thread::spawn(|| { ... }); h.join();` typechecks AND renders the spawned closure body in a SECOND stack column that slides in from the right at the spawn step. Inner thread emits its own FrameEnter/SlotAlloc/etc.; spawning thread sees `h: JoinHandle`. Join switches focus back.

**Independent Test**: load `m08_thread_spawn.rs`, step past `thread::spawn`, observe a second stack column slide in. Step past `h.join()`, observe the spawned column close.

### Implementation (UI surface — Phase 2 already landed the eval plumbing)

- [X] T029 [US1] In `web/index.html`, restructure `<section id="stacks">` to host one `<div class="thread-columns">` outer container (horizontal flex); per-thread columns are rendered as children of this container by `renderStacks`.

- [X] T030 [US1] In `web/index.js`, refactor `renderStacks`:
  - Find or create `.thread-columns` container inside `#stacks`.
  - For each `ThreadColumnView` in `state.threads`, find-or-create a `<div class="thread-column" data-thread-id="{id}">` child of the container. New columns get the `.thread-new` class (drives the slide-in animation); the class is removed after `animationend` fires.
  - Inside each column, render a `<header class="thread-header">{label}</header>` followed by the existing per-frame rendering (FrameCardView mapping).
  - Apply `.thread-current` class to the column matching `is_current`.
  - Apply `.thread-parked` class when `status` is `Parked`; `.thread-joined` when `status` is `Joined`; `.thread-ready` when `status` is `Ready` (column visible but empty until first ThreadSwitch into it).
  - Remove DOM columns whose thread_id no longer appears in `state.threads` (rewind handling).

- [X] T031 [US1] In `web/style.css`, add multi-column stacks layout:
  - `#stacks .thread-columns { display: flex; flex-direction: row; gap: 0.75rem; overflow-x: auto; min-height: 100%; }`
  - `.thread-column { flex: 1 1 0; min-width: 240px; max-width: 360px; display: flex; flex-direction: column-reverse; gap: 0.5rem; padding: 0.5rem; }` (per-column flex-direction:column-reverse keeps innermost frame on top, matching pre-M08 single-column behavior).
  - `.thread-header { font-family: ui-monospace, monospace; font-size: 11px; color: var(--muted); text-transform: uppercase; letter-spacing: 0.5px; padding: 0.2rem 0; border-bottom: 1px dashed var(--border); }`
  - `.thread-current .thread-header { color: var(--accent); border-bottom-color: var(--accent); font-weight: 600; }`
  - `.thread-parked { opacity: 0.5; filter: grayscale(0.5); }`
  - `.thread-joined { opacity: 0.4; }`
  - `@keyframes thread-slide-in { from { transform: translateX(40%); opacity: 0; } to { transform: translateX(0); opacity: 1; } }`
  - `.thread-column.thread-new { animation: thread-slide-in 280ms ease-out; }`

- [X] T032 [US1] In `web/samples/m08_thread_spawn.rs` and `tests/samples/m08_thread_spawn.rs`, create the basic spawn+join sample:
  ```rust
  fn main() {
      let h = thread::spawn(move || {
          let x = 5;
          let y = x + 1;
      });
      h.join();
  }
  ```

- [X] T033 [US1] In `web/index.html`, add a dropdown `<option>` for `m08_thread_spawn`. Place AFTER all m07_7_* entries (M08 entries cluster at the end of the dropdown).

- [X] T034 [US1] In `src/pipeline.rs` `mod tests`, add unit tests:
  - `run_pipeline_thread_spawn`: asserts the trace contains:
    - A `ThreadSpawn { thread_id: 1, .. }` event.
    - A `ThreadSwitch { thread_id: ThreadId(1), .. }` event (when join triggers the switch).
    - Two `FrameEnter` events with `fn_name` containing `"main"` AND a `<closure>` frame for thread 1's body.
    - A `ThreadJoin { thread_id: 1, .. }` event.
    - A final `ThreadSwitch` back to thread 0 (main).
  - `run_pipeline_thread_join_visible`: asserts the spawned thread's `SlotAlloc` for `x: i32` and `SlotWrite` for value 5 land BETWEEN the `ThreadSwitch` and `ThreadJoin` events (i.e. while thread 1 is current).

- [X] T035 [US1] Verify US1 renders cleanly: `cd web && trunk serve`, load `Thread spawn (M08)`, step through. Take screenshot for UX checkpoint #1.

**🎨 UX CHECKPOINT #1**: pause and present the rendered visualization. Discuss:
- Multi-column layout (equal-width vs flexible, gap size, horizontal scroll threshold).
- Slide-in animation (duration, easing, distance — currently 280ms ease-out from translateX(40%)).
- Thread label format (`"main"` vs `"Thread #0"`, current-column emphasis style).
- Joined-column treatment (gray-out vs fade-out vs remove).
- Header styling (border-bottom dashed vs solid, color for current vs paused vs joined).

Iterate until the user signs off. Do NOT proceed to Phase 4 until approved.

---

## Phase 4: User Story 2 — `Arc::new` + `Arc::clone` (shared ownership) (Priority: P1)

**Goal**: `let a = Arc::new(5); let b = Arc::clone(&a);` typechecks; heap block shows `[refs: 2]`; hovering each slot reveals a dashed-purple arrow to the SAME block. Drop transitions decrement the count visibly; HeapFree only at count 0.

**Independent Test**: load `m08_arc_clone.rs`, step past `Arc::new(5)`, observe heap block with `[refs: 1]`. Step past `Arc::clone(&a)`, observe `[refs: 2]`. Step past closing braces, observe count → 1 → 0 → freed.

### Implementation

- [ ] T036 [US2] In `web/index.js`, extend `renderArrows` with a new `ArrowKind::Arc` case:
  - Dashed-purple stroke (CSS class `arrow-arc`); hover-only (same `hover_only: true` default as all other arrows per post-M07.7 polish).
  - Routes through the same gutter machinery as other arrows; dash pattern matches M07.7 dispatch arrow's `stroke-dasharray: 4 3` for visual family consistency.

- [ ] T037 [US2] In `src/ui.rs`, when an Arc-owning relationship is added (eval-side emits ArcClone or HeapAlloc-for-Arc), the snapshot's owning-arrows list includes one `ArrowView { kind: ArrowKind::Arc, ... }` per Arc binding. Each Arc binding pointing at the same heap addr produces a separate arrow (same source slot → same target heap addr); the UI's existing per-target distribution handles the visual spread.

- [ ] T038 [US2] In `src/event.rs` (or `src/ui.rs`'s ArrowKind), add `ArrowKind::Arc` variant (serde-renamed to `"Arc"` for JSON wire compat).

- [ ] T039 [US2] In `web/style.css`, add `.arrow-arc { stroke: #8a4fb4; stroke-width: 2; stroke-dasharray: 4 3; fill: none; }` and the matching `<marker id="arrow-head-arc">` in `web/index.html` (dashed-purple variant of the existing markers).

- [ ] T040 [US2] In `web/index.js`'s `renderHeap`, extend the heap-block rendering to display `[refs: N]` suffix on the addr line when `heap.refcount` is present. Format: `heap #{addr} (Arc, refs: {N})` or similar — concise, doesn't overflow the addr-line width. Also `[locked by #N]` when `heap.mutex_holder` is present (for US3 — landing the rendering now means US3 only needs to flip the value).

- [ ] T041 [US2] Add the US2 sample `web/samples/m08_arc_clone.rs` + `tests/samples/m08_arc_clone.rs`:
  ```rust
  fn main() {
      let a = Arc::new(5);
      let b = Arc::clone(&a);
  }
  ```

- [ ] T042 [US2] In `web/index.html`, add a dropdown `<option>` for `m08_arc_clone`.

- [ ] T043 [US2] In `src/pipeline.rs` `mod tests`, add unit tests:
  - `run_pipeline_arc_clone`: asserts a `HeapAlloc` for the Arc, a `SlotWrite` of `Value::Arc { addr }` for `a`, an `ArcClone { addr }` event, a `SlotWrite` of `Value::Arc { addr }` for `b` (same addr).
  - `run_pipeline_arc_drop_decrement`: at scope-exit, asserts TWO `ArcDrop` events fire (one per binding) AND exactly one `HeapFree` (at the second ArcDrop when count → 0).
  - `run_pipeline_arc_last_drop_frees`: asserts the HeapFree event's `addr` matches the Arc's HeapAlloc addr (no rouge addrs).

**Checkpoint**: US2 fully functional. Arc clones and drops correctly; hover reveals dashed-purple arrows; refcount visible.

---

## Phase 5: User Story 3 — `Mutex::lock` + parked-thread visual (Priority: P1) 🎨 UX CHECKPOINT #2

**Goal**: a two-thread contention sample produces a `ThreadPark` event; the parked column greys out + a dotted-purple line is drawn from the parked column's header to the held mutex's heap block. Lock release unparks the waiter.

**Independent Test**: load `m08_mutex_contention.rs`, step until thread A locks; step thread B's `lock()`; observe B's column greys + dotted line. Step past A's guard drop; observe B unpark.

### Implementation

- [ ] T044 [US3] In `web/index.js`, add `renderParkedLines(state.parked_threads, state.heap)`:
  - For each `ParkedThreadView`, find the parked column's header element AND the mutex's heap-block element.
  - Draw an SVG dotted-purple path from the column header (bottom-center) to the heap block (top-center, or left edge if the heap panel is to the right).
  - CSS class: `.parked-line` (stroke-dasharray for dots, stroke: muted purple, opacity: 0.7).
  - Cleared on next render (transient — re-derived from snapshot each frame; if the thread unparks, no line gets drawn).
  - Called from `renderUi` AFTER `renderArrows` so the parked-line lives in the same SVG overlay.

- [ ] T045 [US3] In `web/style.css`, add:
  - `.parked-line { stroke: #8a4fb4; stroke-width: 1.5; stroke-dasharray: 2 3; fill: none; opacity: 0.7; }`
  - `.thread-column.thread-parked .thread-header { color: #8a4fb4; }` (parked column's header takes the mutex color so the dotted line "starts from" the styled header).

- [ ] T046 [US3] Verify the existing `.thread-parked` class from Phase 3 styles properly mutes the column AND that the heap block displays `[locked by #N]` from Phase 4's T040 work. Both should already be in place from Phase 2/3/4 — this task is just a re-verify with a Mutex sample.

- [ ] T047 [US3] Add the US3 sample `web/samples/m08_mutex_contention.rs` + `tests/samples/m08_mutex_contention.rs`. The sample needs to construct contention deterministically — main thread holds the lock; spawned thread tries to lock and parks. Sample content:
  ```rust
  fn main() {
      let m = Arc::new(Mutex::new(0));
      let m2 = Arc::clone(&m);
      let h = thread::spawn(move || {
          let mut g = m2.lock();
          *g += 1;
      });
      let mut g = m.lock();
      *g += 10;
      // g drops here; spawned thread unparks and runs
      h.join();
  }
  ```
  (NOTE: the cooperative scheduler runs main FIRST until it parks/joins; the spawned thread is queued. When main locks, mutex is free → main holds. When main `g` drops at the `}`, mutex frees AND scheduler picks the queued spawned thread. The spawned thread locks (free now), runs, drops, completes. Main resumes at `h.join()`; thread 1 is `Done`, ThreadJoin fires immediately.)

- [ ] T048 [US3] In `web/index.html`, add a dropdown `<option>` for `m08_mutex_contention`.

- [ ] T049 [US3] In `src/pipeline.rs` `mod tests`, add unit tests:
  - `run_pipeline_mutex_lock`: trivial single-thread `let m = Mutex::new(5); let g = m.lock(); *g += 1;` — asserts LockAcquire fires, MutexGuard slot bound, LockRelease at scope exit, no parking.
  - `run_pipeline_mutex_contention`: the US3 sample — asserts: ThreadSpawn fires for thread 1; main acquires the lock (LockAcquire on the inner Mutex's heap addr); when scheduler switches to thread 1, thread 1's `lock()` parks → ThreadPark event fires for thread 1 with the correct lock addr; ThreadSwitch back to thread 0 (or to whoever's next ready — pin down based on scheduler rule). Main releases → LockRelease + LockAcquire-for-thread-1; thread 1 finishes; ThreadJoin fires.

- [ ] T050 [US3] Verify US3 renders cleanly: `cd web && trunk serve`, load `Mutex contention (M08)`, step through. Take screenshot for UX checkpoint #2.

**🎨 UX CHECKPOINT #2**: pause and present the rendered visualization. Discuss:
- Parked-column opacity / grayscale level (currently opacity 0.5 + grayscale 0.5).
- Dotted-line color and style (currently muted purple `#8a4fb4`, stroke-width 1.5, dasharray `2 3`).
- Dashed-purple Arc arrow visibility on hover (any tweaks?).
- Refcount display format (`[refs: 2]` vs `(refs: 2)` vs `· 2 refs ·`).
- Holder display format (`[locked by #N]` vs `(locked: #N)` etc).
- Where the dotted-line attaches on the column header vs the heap block (currently bottom-center → top-center).

Iterate until the user signs off. Do NOT proceed to Phase 6 until approved.

---

## Phase 6: User Story 4 — `Arc<Mutex<T>>` end-to-end (Priority: P2) 🎯 HEADLINE

**Goal**: the canonical `Arc<Mutex<T>>` pattern with two threads renders ALL M08 visualizations simultaneously — multi-column stacks, dashed-purple Arc arrows, parked-thread visual, refcount + holder display.

**Independent Test**: load `m08_arc_mutex.rs`, step through both threads' operations, observe every M08 layer engaged.

### Implementation

- [ ] T051 [US4] Add the US4 sample `web/samples/m08_arc_mutex.rs` + `tests/samples/m08_arc_mutex.rs`. The headline sample exercises ALL three foundational US together:
  ```rust
  fn main() {
      let m = Arc::new(Mutex::new(0));
      let m2 = Arc::clone(&m);
      let h = thread::spawn(move || {
          let mut g = m2.lock();
          *g += 1;
      });
      {
          let mut g = m.lock();
          *g += 10;
      }
      h.join();
  }
  ```
  (Inner block scope around main's lock guarantees the guard drops before `h.join()`, which guarantees the spawned thread gets the lock before being joined.)

- [ ] T052 [US4] In `web/index.html`, add a dropdown `<option>` for `m08_arc_mutex`. **Place LAST** in the M08 entries so learners see the foundational samples first.

- [ ] T053 [US4] In `src/pipeline.rs` `mod tests`, add `run_pipeline_arc_mutex` — the headline US4 test:
  - Asserts: 1 HeapAlloc for the Mutex (with Arc wrapping), refcount transitions 1 → 2 via ArcClone, ThreadSpawn for thread 1, LockAcquire for thread 0, ThreadSwitch to thread 1, ThreadPark for thread 1 (contention), ThreadSwitch back to thread 0 (or some ready thread per scheduler), LockRelease from thread 0, LockAcquire for thread 1 (unparked), thread 1's `*g += 1` write, LockRelease from thread 1, thread 1 Done, ThreadJoin for thread 1, final ArcDrop events bringing refcount to 0 + HeapFree.
  - Assert final value in the heap block is `11` (i32) — 0 + 10 + 1.

**Checkpoint**: US4 fully functional. Headline pedagogy intact.

---

## Phase 7: Cross-cutting tests

**Purpose**: determinism guarantee, rejection cases, additional coverage not in US happy paths.

- [ ] T054 In `src/pipeline.rs` `mod tests`:
  - `run_pipeline_determinism`: run the `m08_arc_mutex` source TWICE through the pipeline; assert `events_run1 == events_run2`. FR-013/SC-010 — catches scheduler non-determinism if regressed.

- [ ] T055 In `src/pipeline.rs` `mod tests`:
  - `run_pipeline_closure_outside_spawn`: source `let f = move || { let x = 5; };` (closure NOT inside thread::spawn) → typeck error mentioning "closures are only supported as `thread::spawn` arguments".
  - `run_pipeline_non_move_closure`: source `fn main() { thread::spawn(|| { let x = 5; }); }` (no `move`) → typeck error mentioning "`move` closure".

- [ ] T056 In `src/pipeline.rs` `mod tests`:
  - `run_pipeline_arc_clone_non_arc`: source `let a = 5; let b = Arc::clone(&a);` → typeck error (Arc::clone expects `&Arc<T>`, got `&i32`).

**Checkpoint**: ≥ 10 new M08 tests total (T034 US1: 2; T043 US2: 3; T049 US3: 2; T053 US4: 1; T054 determinism: 1; T055 rejections: 2; T056 rejection: 1 = **12 tests**). Exceeds the SC floor.

---

## Phase 8: Polish & Cross-Cutting

**Purpose**: snapshot verify, bundle-size check, warnings, manual QA, doc updates.

- [ ] T057 [P] Run `cargo test`. Verify M01/M02/M03 byte-identical (no existing sample constructs threads/Arc/Mutex). New M08 tests pass. Total should be ~185 tests (173 baseline + 12 M08).
- [ ] T058 [P] Build WASM release and measure bundle size: `cd web && trunk build --release` (wasm-opt may fail per the pre-existing tooling issue; use the staged size at `dist/.stage/*.wasm`). Compare to M07.7 baseline (398,847 B). Acceptable if ≤ +25% (~498 KB).
- [ ] T059 [P] Run `RUSTFLAGS="-D warnings" cargo build --release` AND `cargo test`. Both clean. Fix any NEW warnings introduced by M08.
- [ ] T060 [P] Run `cargo clippy --all-targets`. Fix any NEW lints (pre-existing lints out of scope — diff against `git stash && cargo clippy && git stash pop`).
- [ ] T061 Manual M08 QA per `specs/019-m08-threads-arc-mutex/quickstart.md` — ~15-minute walk, includes both UX checkpoints' post-iteration verification (multi-column, parked viz). Verify error UX via live editing: closure outside spawn, non-move closure, Arc::clone on non-Arc.
- [ ] T062 Verify `CLAUDE.md` "Active Technologies" footer includes M08 (the `update-agent-context.sh` script handles this; verify the M08 line is present).
- [ ] T063 Final commit prep. MR note: "12th invocation of the closed-enum-with-revisions rule. First milestone to introduce a new MemEvent variant since M07.7 (`ThreadSwitch`). The 7 pre-declared M03 thread+sync MemEvent variants (`ThreadSpawn`, `ThreadJoin`, `ThreadPark`, `ArcClone`, `ArcDrop`, `LockAcquire`, `LockRelease`) finally get their payloads emitted. Major eval refactor: single-stack `Vec<Frame>` → multi-thread `IndexMap<ThreadId, ThreadState>`. Major UI refactor: `StateSnapshot.frames` → `StateSnapshot.threads: Vec<ThreadColumnView>`. New closure surface (minimal: `move ||` for `thread::spawn` arg only). Closes Level 4 — every CLAUDE.md L4 mechanism is now visualized."

---

## Dependencies

```text
Phase 1 (Setup)
  └─ T001 (verify baseline)

Phase 2 (Foundational) — blocks ALL user-story phases
  ├─ T002 (contract amendment, can run anytime)
  ├─ T003 [P] (lexer keyword `move`)
  ├─ T004 [P] (token variant)
  ├─ T005 (AST Expr::Closure — depends on T004)
  ├─ T006 (parser closure — depends on T005)
  ├─ T007 (resolve capture analysis — depends on T005)
  ├─ T008 (ThreadId + Value/MemEvent variants)
  ├─ T009 (Ty variants — depends on T008 for Ty exhaustiveness)
  ├─ T010 (ty_from_ast lowering — depends on T009)
  ├─ T011 (path-call dispatch — depends on T010)
  ├─ T012 (method-call dispatch — depends on T009)
  ├─ T013 (closure constraint — depends on T011)
  ├─ T014 (HeapObject variants — depends on T008)
  ├─ T015 (Evaluator struct refactor — depends on T014)
  ├─ T016 (frames-routing refactor — depends on T015) ← critical, wide touch surface
  ├─ T017 (scheduler helpers — depends on T015 + T008)
  ├─ T018 (eval path-call — depends on T011 + T014 + T017)
  ├─ T019 (eval method-call — depends on T012 + T014 + T017)
  ├─ T020 (eval start_queued_thread — depends on T015 + T017)
  ├─ T021 (eval drop_current_scope — depends on T014 + T019)
  ├─ T022 (eval entry-point scheduler loop — depends on T017 + T020)
  ├─ T023 [P] (UI types — depends on T008)
  ├─ T024 (UI World refactor — depends on T023)
  ├─ T025 (UI apply_event refactor — depends on T024)
  ├─ T026 (UI new event arms — depends on T025)
  ├─ T027 (UI state_snapshot refactor — depends on T025)
  └─ T028 (verify baseline — depends on T015-T027)

Phase 3 (US1) — depends on Phase 2 — 🎨 UX CHECKPOINT #1
  ├─ T029 (HTML restructure)
  ├─ T030 (renderStacks JS refactor)
  ├─ T031 (multi-column CSS)
  ├─ T032 (sample pair)
  ├─ T033 (dropdown)
  ├─ T034 (2 unit tests)
  └─ T035 (visual verification + 🎨 UX CHECKPOINT #1)

🎨 PAUSE for user review before Phase 4.

Phase 4 (US2) — depends on Phase 3 (UI shell approved)
  ├─ T036 (renderArrows Arc case)
  ├─ T037 (snapshot owning-arrow Arc)
  ├─ T038 (ArrowKind::Arc variant)
  ├─ T039 (.arrow-arc + marker CSS)
  ├─ T040 (heap display refcount + holder)
  ├─ T041 (sample pair)
  ├─ T042 (dropdown)
  └─ T043 (3 unit tests)

Phase 5 (US3) — depends on Phase 4 — 🎨 UX CHECKPOINT #2
  ├─ T044 (renderParkedLines JS)
  ├─ T045 (parked-line CSS)
  ├─ T046 (re-verify thread-parked + holder rendering)
  ├─ T047 (sample pair)
  ├─ T048 (dropdown)
  ├─ T049 (2 unit tests)
  └─ T050 (visual verification + 🎨 UX CHECKPOINT #2)

🎨 PAUSE for user review before Phase 6.

Phase 6 (US4) — depends on Phases 3-5
  ├─ T051 (sample pair — headline)
  ├─ T052 (dropdown — placed LAST)
  └─ T053 (1 unit test)

Phase 7 (cross-cutting) — depends on Phase 2
  ├─ T054 (determinism)
  ├─ T055 (2 rejection tests)
  └─ T056 (1 rejection test)

Phase 8 (Polish) — depends on Phases 3-7
  └─ T057–T063 (snapshot/bundle/warnings/clippy/QA/docs/commit)
```

---

## Parallel execution opportunities

- **Phase 2**: T003 + T004 are file-disjoint [P]. T023 is independent of T015–T022 [P]. T002 (contract markdown) can run anytime [P].
- **Phases 4/5/6/7**: independent of each other after Phase 3 lands (each operates on different UI/sample/test files).
- **Phase 8**: T057/T058/T059/T060 all parallelizable [P].

---

## Implementation strategy

**MVP scope** = **US1 only** (multi-column stacks + spawn/join + UX checkpoint #1). Lands the foundational multi-thread visualization without yet exercising Arc/Mutex semantics. ~1200 LOC.

**Incremental delivery**:
1. **MVP (US1)**: Phases 1+2+3 (Setup + Foundational + US1). Multi-column stacks live; UI shell signed off. After this you have a defensible M08a if scope feels XL.
2. **+US2 (Arc clone/drop)**: Phase 4. Dashed-purple arrows + refcount.
3. **+US3 (Mutex + parked viz)**: Phase 5. Parked-thread visual + UX checkpoint #2.
4. **+US4 (Arc<Mutex<T>>)**: Phase 6. Headline synthesis sample.
5. **+Cross-cutting tests**: Phase 7. Determinism + rejections.
6. **+Polish**: Phase 8. Snapshot/bundle/QA/docs.

**Recommended landing order**: ship all 4 user stories + cross-cutting + polish in one merge. The eval thread refactor (Phase 2) is the most invasive change; splitting US2-US4 into a follow-up M08b would mean re-loading the foundational context. Single-merge matches M07.4/M07.5/M07.6/M07.7 pattern. The TWO UX checkpoints are natural pauses for user iteration but the implementation proceeds linearly between them.

**Mid-implementation split contingency**: if Phase 3 UX checkpoint reveals XL appetite mismatch (e.g. multi-column rendering itself takes 3-4 iteration rounds), defer Phases 4-6 to a follow-up M08b per MILESTONES.md's split contingency. The user can call this at the UX#1 pause point.

**Sequence note**: M08 closes Level 4. After this milestone, the project visualizes every Rust mechanism in CLAUDE.md's Levels 1-4. CLAUDE.md doesn't define Levels 5+; nothing is being deferred.
