# Research — M08 Implementation Decisions

14 design decisions across parser (closure surface), AST (Expr::Closure), resolve (capture analysis), typeck (Arc/Mutex/Guard type dispatch), eval (thread scheduler + Arc refcount + Mutex state), UI (multi-column stacks + parked-thread visual + dashed-purple arrows + refcount display), and protocol amendment.

## Parser

### R-001 — Minimal closure surface: `|| { body }` and `move || { body }`

- **Decision**: closures are no-arg only in M08; body is a `Block`. `move` is an optional keyword prefix. The two-char `||` token (M01's `OrOr`) is RECOGNIZED as the empty closure-param list when seen at expression-atom start; the existing `OrOr` infix path stays unchanged (it runs after `parse_atom` returns, so atom-position `||` is grammar-distinct).
- **AST**: `Expr::Closure { is_move: bool, body: Block, span: Span }`.
- **Parser**: `parse_atom` extension — at expression start, if peek is `Move` keyword OR `OrOr` (two-char `||`), consume + parse body block + build `Expr::Closure`.
- **Out of scope**: closure params (`|x| { body }`), explicit return type (`|| -> i32 { body }`), single-pipe-with-args (`|x, y| { body }`). M08 only needs no-arg bodies for `thread::spawn` arguments.
- **Rationale**: minimal AST surface; thread::spawn closures in real Rust often have no args anyway (capture via `move` is the primary mechanism). Future closure expansion can layer on.

### R-002 — `Move` keyword lexed; `|`/`||` already covered by M01

- **Decision**: add `move` as a keyword (`TokenKind::Move`). The `||` two-char token (`TokenKind::OrOr`) already exists from M01 and serves double duty: infix logical-or AND empty closure params (disambiguated by parser position). Single-pipe `|` is NOT needed in M08 (no closure args).
- **Rationale**: smallest possible lexer extension. Avoids introducing `|` until closure args become a feature.

## AST / resolve

### R-003 — Closure body captures determined by free-variable analysis at resolve

- **Decision**: in `resolve_expr`'s new `Expr::Closure` arm: push a fresh scope for the closure body; resolve the body's statements + tail; pop the scope. Any `Ident` use whose resolved binding lives in an ENCLOSING scope (not the closure's own scope) is recorded as a captured binding. Store the capture list on a side-table (`Resolution.closure_captures: IndexMap<Span, Vec<BindingId>>` keyed by closure span).
- **Rationale**: captures are scope-relative, not type-driven. Resolve already tracks scopes, so this is a one-pass extension. Eval reads `closure_captures` at `thread::spawn` evaluation to know what to snapshot into the spawned thread's initial scope.

### R-004 — `Expr::Closure` only valid as `thread::spawn` arg in M08

- **Decision**: typeck rejects `Expr::Closure` everywhere EXCEPT as the single argument to `thread::spawn(...)`. Other positions → "closures are only supported as `thread::spawn` arguments in M08".
- **Rationale**: avoid introducing general closure types (`Fn`/`FnMut`/`FnOnce`). The narrow special-case carries all M08 needs without opening Pandora's box.

## Typeck

### R-005 — `Ty::Arc(Box<Ty>)`, `Ty::Mutex(Box<Ty>)`, `Ty::MutexGuard(Box<Ty>)`, `Ty::JoinHandle`

- **Decision**: four additive Ty variants. `Ty::Arc(inner)` for `Arc<T>` (analog of `Ty::Box(inner)`); `Ty::Mutex(inner)` for `Mutex<T>`; `Ty::MutexGuard(inner)` for the guard returned by `lock()` (carries the protected type); `Ty::JoinHandle` for `thread::spawn`'s return value (M08 doesn't model the JoinHandle's inner type since spawned closures return unit only — keep it variant-less for simplicity).
- **`Ty::name()`**: `format!("Arc<{}>", inner.name())`, `format!("Mutex<{}>", inner.name())`, `format!("MutexGuard<{}>", inner.name())`, `"JoinHandle".to_owned()`.
- **`Ty::is_copy()`**: all four are `false` (heap-owning / thread-resource types).

### R-006 — Path-call dispatch for `Arc::new` / `Arc::clone` / `Mutex::new` / `thread::spawn`

- **Decision**: extend `typecheck_path_call`'s static-fn table with four new entries:
  - `Arc::new(v: T) -> Arc<T>` — inner type inferred from arg
  - `Arc::clone(arc: &Arc<T>) -> Arc<T>` — inner type matches arg's inner
  - `Mutex::new(v: T) -> Mutex<T>` — inner type from arg
  - `thread::spawn(f: Closure) -> JoinHandle` — accepts only `Expr::Closure`
- **Rationale**: mirrors M07's `Box::new(v)` / `Vec::new()` / `String::from(s)` path-call dispatch pattern. No new typeck infrastructure.

### R-007 — Method dispatch: `mutex.lock()`, `guard.deref()`, `handle.join()`

- **Decision**: extend `typecheck_method_call`'s hardcoded built-ins:
  - `Mutex::lock(&self) -> MutexGuard<T>` (where T is the mutex's inner)
  - `MutexGuard::deref(&self) -> &T` — implicit, drives `*guard` derefs (M06.1 deref machinery extended to recognize MutexGuard receivers)
  - `JoinHandle::join(self) -> ()` — consumes the handle, returns unit
- **Auto-deref through Arc**: `Arc<Mutex<T>>::lock()` works because typeck auto-derefs `Arc<T>` to `T` for method-call dispatch (extension of M07.6/M07.7's `&T` auto-deref). The eval path follows: when `recv` is `Value::Arc { addr }`, look up the HeapObject's `value`, recursively dispatch on that.
- **Rationale**: matches Rust ergonomics — `arc.lock()` works without manual deref. Keeps sample code natural.

## Eval

### R-008 — Cooperative scheduler with FIFO-by-spawn-order + yield points

- **Decision**: SINGLE scheduler thread (the evaluator); all "threads" are cooperative. `Evaluator.threads: IndexMap<ThreadId, ThreadState>` (IndexMap preserves spawn order). `Evaluator.current_thread_id: ThreadId`. State machine: `Ready` (queued, body not yet started) → `Running` (current_thread_id == this) → `Parked(HeapAddr)` (waiting on mutex) → `Running` (mutex released) → `Joined` (body completed).
- **Yield points** (where the scheduler may switch threads):
  - `thread::spawn`: enqueues the new ThreadState as `Ready`, does NOT switch (current thread continues running greedily).
  - `mutex.lock()` when held: change current thread to `Parked(heap_addr)`, emit `ThreadPark`, scheduler picks next ready thread + emits `ThreadSwitch`.
  - `handle.join()` if target not yet started/finished: change current thread to `Parked(thread_id_as_pseudo_addr)` waiting on the joined thread's completion, scheduler picks next ready thread.
  - Guard Drop (LockRelease): unparks the first parked thread waiting on this mutex (FIFO by spawn order). If current thread continues (no switch), no `ThreadSwitch` event.
- **Scheduler picks**: when current thread parks, scan `threads` IndexMap in spawn order, pick first thread in `Ready` or `Running` state (`Running` only possible if it just unparked); emit `ThreadSwitch`.
- **Alternatives considered**:
  - Round-robin per cursor step: artificial — events from random threads interleave without semantic reason. Rejected.
  - Random with fixed seed: deterministic but unpredictable for the learner. Rejected.
  - Eager (run spawned body inline at spawn site): can't demonstrate contention. Rejected.
- **Rationale**: yield-point-based scheduling makes the contention pedagogy explicit. The learner sees "main locks; main mutates; main drops guard; THEN spawned thread runs and locks". The interleaving is rule-driven, not arbitrary.

### R-009 — `move ||` capture semantics: snapshot bindings into spawned thread's initial scope

- **Decision**: at `thread::spawn(closure)` evaluation:
  1. Read `Resolution.closure_captures[closure_span]` → list of captured BindingIds.
  2. For each captured BindingId, look up its current Value in the spawning thread's locals.
  3. Build the spawned ThreadState with status `Ready`; queue the closure body PLUS the captured (binding_id, name, value) tuples.
- At first switch into the spawned thread:
  1. Emit `FrameEnter` for the closure body (display_name = `"<closure>"` or similar).
  2. For each captured (binding_id, name, value): emit `SlotAlloc` (with the binding's typeck-recorded type) + `SlotWrite` (with the captured value).
  3. Proceed with normal body evaluation.
- **`move` vs non-`move`**: M08 only supports `move`. The semantics above ARE move semantics — the captured value is copied/moved into the spawned thread. The original binding in the spawning thread is NOT removed (visualization simplification — Rust would remove it). Future non-move closures would need borrow-tracking across thread boundaries (out of scope).
- **Arc captures**: a captured `Value::Arc { addr }` carries the heap addr. Multiple captures of the same Arc (e.g. main has `a`, spawned captures `a`) all share the addr. Refcount is NOT bumped at capture — `move` is a transfer, not a clone. The user must explicitly `Arc::clone(&a)` BEFORE the spawn to share.
- **Rationale**: simplest mental model. Captures are values; values are copied. Refcount mutations happen via explicit ArcClone — not as a side effect of capture.

### R-010 — `Arc` refcount semantics + count-aware Drop

- **Decision**: `HeapObject::Arc { value: Box<Value>, strong_count: u32 }` (Box because Value is non-Copy). `Arc::new(v)` allocates with strong_count = 1; `Arc::clone(&a)` increments + emits `ArcClone { addr }`. Scope-exit Drop on a binding holding `Value::Arc { addr }`: decrement strong_count; emit `ArcDrop { addr }`; if count reaches 0, ALSO emit `HeapFree { addr }` + remove from heap.
- **Drop path extension**: `drop_current_scope` currently handles `Value::Box/Vec/String/BoxDyn` uniformly (emit HeapFree). For `Value::Arc`, branch: decrement count, conditionally emit HeapFree.
- **Rationale**: matches Rust's reference-counting semantics. The visualization shows the count on the heap block; the learner sees `[refs: 2]` → `[refs: 1]` → `[refs: 0]` → block-freed.

### R-011 — `Mutex` state + parked-thread queue

- **Decision**: `HeapObject::Mutex { value: Box<Value>, holder: Option<ThreadId>, waiters: Vec<ThreadId> }`. `lock()` checks holder:
  - `None` → set holder = current_thread_id, emit `LockAcquire { addr }`, return Value::MutexGuard { addr }.
  - `Some(other)` → push current_thread_id to waiters, emit `ThreadPark { thread_id, lock: addr }`, change current thread's status to `Parked(addr)`, scheduler picks next ready thread + emits `ThreadSwitch`. The lock() call effectively retries when the parked thread is later un-parked.
- Guard Drop:
  - Emit `LockRelease { addr }`, clear holder.
  - If waiters non-empty: pop the first waiter (FIFO), change its status from `Parked` to `Running`, set holder = waiter_id, emit `LockAcquire` for the unparked thread. The unparked thread resumes from inside its `lock()` call with the guard now bound.
- **Rationale**: standard FIFO mutex semantics. Single waiter is the M08 sample case; the FIFO queue handles N-waiter scenarios for completeness.

### R-012 — `MemEvent::ThreadSwitch { thread_id, span }` — drives UI current-thread routing

- **Decision**: emit `ThreadSwitch` whenever the cooperative scheduler picks a different thread (i.e. `self.current_thread_id` is about to change). The UI's apply_event uses ThreadSwitch to update its `current_thread_id` field; subsequent SlotAlloc/SlotWrite/FrameEnter events route to that thread's column.
- **Initial state**: no ThreadSwitch needed at trace start — the UI defaults to thread 0 (main).
- **Span**: the span of the operation that caused the switch (e.g. `lock()` call site for park-driven switches, `join()` for join-driven, body's closing brace for completion-driven).
- **Rationale**: explicit context-switch events keep the UI's routing logic simple. Alternative — every MemEvent carries a `thread_id` field — would be invasive (every existing variant restructures, breaking compat).

## UI

### R-013 — Multi-column stacks: horizontal flex container, per-column vertical stack

- **Decision**: `<section id="stacks">`'s inner becomes a horizontal flex container (`display: flex; flex-direction: row;`). Each thread gets a `<div class="thread-column" data-thread-id="{id}">` child. Inside each column, the existing `flex-direction: column-reverse` rule applies (innermost frame on top). Columns are equal-width when ≤ 3 active; horizontal scroll past 3 (overflow-x: auto on the outer container).
- **Column header**: each column has a small `<header class="thread-header">Thread #{id}</header>` so the learner can identify columns. Main is `Thread #0` (or labeled "main" specifically).
- **Slide-in animation**: CSS `@keyframes thread-slide-in { from { transform: translateX(100%); opacity: 0; } to { transform: translateX(0); opacity: 1; } }`. Applied to `.thread-column.thread-new` (class added on first render of a new column, removed after animation completes via JS `animationend` handler).
- **Closed/joined column**: `.thread-column.thread-joined` — grayed out (low opacity, or removed entirely depending on UX checkpoint).
- **Plan-phase recommendation**: equal-width columns; horizontal scroll past 3; slide-in animation on spawn; grayed-out treatment on join (keep visible briefly, then fade).
- **UX checkpoint after US1 first cut**.

### R-014 — UX checkpoint #1: multi-column layout + slide-in animation

This is the first iterative UI piece. **Locked-in data shape**:

```rust
pub struct ThreadColumnView {
    pub thread_id: u32,
    pub label: String,           // "main" or "thread #{id}"
    pub frames: Vec<FrameCardView>,
    pub is_current: bool,        // for visual emphasis on the actively-executing column
    pub status: ThreadStatusView, // Running | Parked(heap_addr) | Joined
}

pub enum ThreadStatusView {
    Running,
    Parked { lock_addr: u32 },
    Joined,
}
```

**Recommended visual** (Proposal A for the UX checkpoint):

```text
┌─────────────────────────────────────────────────────────────────┐
│ Stacks                       │ Heap     │ VTABLES │ Static (RO) │
├──────────────┬───────────────┼──────────┼─────────┼─────────────┤
│ Thread #0    │ Thread #1     │          │         │             │
│ (main)       │ (current)     │          │         │             │
│              │               │          │         │             │
│ ┌──────────┐ │ ┌──────────┐  │          │         │             │
│ │ main()   │ │ │<closure> │  │          │         │             │
│ │  h: JH   │ │ │  x: i32  │  │          │         │             │
│ └──────────┘ │ └──────────┘  │          │         │             │
└──────────────┴───────────────┴──────────┴─────────┴─────────────┘
```

Two columns visible; main on the left, spawned on the right; new column slides in from right on spawn.

**UX checkpoint procedure** (mirrors M07.4 + M07.7):
1. Land all non-UI plumbing first (parser closure, eval thread scheduler, ThreadSwitch event).
2. Implement Proposal A's first cut (multi-column layout, slide-in animation).
3. **PAUSE** for user review.
4. Iterate on tweaks (column width, scroll behavior, label format, slide-in timing).
5. Continue with US2 + US3.

### R-015 — UX checkpoint #2: parked-thread visual + dashed-purple Arc arrows + refcount display

The second iterative UI piece. **Locked-in data shapes**:

```rust
pub struct HeapView {
    // existing fields ...
    /// **M08**: present for Arc heap blocks; renders as `[refs: N]` suffix on the addr line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refcount: Option<u32>,
    /// **M08**: present for Mutex heap blocks when held; renders as `[locked by #N]` suffix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mutex_holder: Option<u32>,
}

pub struct StateSnapshot {
    // existing fields ...
    /// **M08**: parked-thread connections. One entry per `(parked_thread, held_lock_heap_addr)` pair.
    /// Drives the dotted-purple line from each parked column's header to the held mutex's heap block.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parked_threads: Vec<ParkedThreadView>,
}

pub struct ParkedThreadView {
    pub thread_id: u32,
    pub lock_heap_addr: u32,
}
```

**Recommended visual** (Proposal A for the UX checkpoint):

- **Parked column**: low opacity (0.5), grayscale filter
- **Dotted line**: from parked column header → held mutex heap block. Color: muted purple. Stroke: dotted (1px gap pattern).
- **Arc arrows**: dashed purple (color: `#8a4fb4`), 2px stroke, dash pattern `4 3` (same as M07.7 dispatch arrows for visual family). Hover-only per Rule 1.
- **Refcount display**: `heap #N [refs: 2]` text on the heap block's addr line (small muted-color suffix).
- **Mutex holder display**: `heap #N [Mutex<i32>, locked by #1]` text on the heap block.

**Alternative — Proposal B (compact)**: parked thread shows a small "[parked on #N]" badge in its header; no dotted line. Smaller visual footprint but loses the "this thread is waiting on this specific lock" spatial pedagogy.

**Plan-phase recommendation: Proposal A.** The dotted line is the headline visual of US3 — losing it deflates the parking pedagogy.

**UX checkpoint procedure** (after US3 first cut):
1. Land US3 plumbing (Mutex eval, ThreadPark event, parked status tracking).
2. Implement Proposal A's first cut (parked viz, dashed-purple Arc arrows, refcount + holder display).
3. **PAUSE** for user review.
4. Iterate on tweaks (parked opacity, dotted-line color, dashed-arrow color, refcount text format).
5. Continue with US4.

### R-016 — Stacks panel back-compat: single-thread programs render unchanged

- **Decision**: all M01-M07.7 programs run in thread 0 (main), no spawn events, no ThreadSwitch events. The UI's `World.threads` map has one entry (id 0); `renderStacks` renders one `.thread-column` containing the existing frame stack. Visually IDENTICAL to pre-M08.
- **Rationale**: no regression for existing samples. The multi-column flex only "spreads" when there's more than one column.

## Protocol

### R-017 — 12th invocation of the closed-enum-with-revisions rule

- **Decision**: amend M03's contract. Additions:
  - **New `Ty` variants**: `Ty::Arc(Box<Ty>)`, `Ty::Mutex(Box<Ty>)`, `Ty::MutexGuard(Box<Ty>)`, `Ty::JoinHandle`.
  - **New `Value` variants**: `Value::Arc { addr }`, `Value::Mutex { addr }`, `Value::MutexGuard { addr }`, `Value::JoinHandle { thread_id }`.
  - **New addressing newtype**: `ThreadId(u32)` (analog of `VtableAddr` from M07.7).
  - **New `MemEvent` variant**: `ThreadSwitch { thread_id, span }`.
  - **Payload finalization (no shape change)**: the 7 pre-declared thread+sync MemEvent variants (`ThreadSpawn`, `ThreadJoin`, `ThreadPark`, `ArcClone`, `ArcDrop`, `LockAcquire`, `LockRelease`) get their fields emitted with concrete semantics — payloads stay shaped exactly as M03 declared.
  - **New HeapObject variants**: `HeapObject::Arc { value, strong_count }`, `HeapObject::Mutex { value, holder, waiters }` (eval-internal, not in wire protocol).
- **Precedent chain**: M03.1 → M03.2 → M06 → M07 → M07.1 → M07.2 → M07.3 → M07.4 → M07.5 → M07.6 → M07.7 → **M08**.
- **Snapshot byte-identity**: M03 stays byte-identical for programs that don't construct threads/Arc/Mutex (additive variants + serde-default-empty on new HeapView/StateSnapshot fields).

### R-018 — Determinism guarantee + test class

- **Decision**: FR-013/SC-010 — multi-threaded programs produce byte-identical event streams across runs. Implementation guarantee: the scheduler is fully rule-driven (R-008); no randomness, no system clocks, no hash-map iteration order (use IndexMap for thread store).
- **New test class**: `run_pipeline_determinism` — run a multi-threaded sample twice; assert `events_run1 == events_run2`. Catches scheduler non-determinism if introduced later.
- **Rationale**: deterministic event streams are a non-negotiable invariant for the visualizer's snapshot tests AND for the "rewind any step" pedagogy.
