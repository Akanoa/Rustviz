# Data Model — M08 Entities

Thread + Arc + Mutex additions: **1 new AST expr** (`Expr::Closure`), **4 new `Ty` variants** (`Arc`, `Mutex`, `MutexGuard`, `JoinHandle`), **4 new `Value` variants** (`Arc`, `Mutex`, `MutexGuard`, `JoinHandle`), **1 new addressing newtype** (`ThreadId`), **1 new `MemEvent` variant** (`ThreadSwitch`), **2 new `HeapObject` variants** (`Arc`, `Mutex`), **eval-side scheduler state** (`Evaluator.threads: IndexMap<ThreadId, ThreadState>`, `current_thread_id`), **UI views** (`ThreadColumnView`, `ParkedThreadView`, extended `HeapView`).

The 7 thread+sync MemEvent variants pre-declared at M03 (`ThreadSpawn`, `ThreadJoin`, `ThreadPark`, `ArcClone`, `ArcDrop`, `LockAcquire`, `LockRelease`) get their payload semantics formalized here — variant SHAPES stay byte-identical to M03's declarations.

## New (AST expr): `Expr::Closure`

```rust
// In src/parse/ast.rs

pub enum Expr {
    // ... existing variants
    /// **M08**: closure expression `|| { body }` or `move || { body }`.
    /// No-arg only in M08. The only typeck-valid position is the single
    /// argument to `thread::spawn(closure)` — other positions error.
    Closure {
        /// `true` for `move ||`, `false` for plain `||`.
        is_move: bool,
        /// Body block.
        body: Block,
        /// Span from `move`/`||` through body's closing `}`.
        span: Span,
    },
}
```

### Validation rules

- **VR-1**: `Expr::Closure` typeck-accepts ONLY as the single argument to `thread::spawn(...)`. Other positions → "closures are only supported as `thread::spawn` arguments in M08".
- **VR-2**: M08 supports only `move ||` (with body). Non-move closures parse the same shape but typeck-rejects (M08 needs `move` semantics for cross-thread captures — borrowing captures are out of scope per spec's edge case list).
- **VR-3**: closure body's captured bindings (free vars in the body resolving to enclosing-scope bindings) are recorded in `Resolution.closure_captures[closure_span]: Vec<BindingId>`.

## Modified: `Ty` — adds 4 variants

```rust
pub enum Ty {
    // ... existing variants
    /// **M08**: shared-ownership heap pointer. Multiple Arcs share one
    /// HeapObject::Arc; refcount lives in the heap object. Distinct from
    /// `Ty::Box(_)` — Box has unique ownership, Arc has shared.
    Arc(Box<Ty>),
    /// **M08**: lock-protected heap value. The lock state (holder) lives
    /// in the HeapObject::Mutex. Accessing the inner value requires `.lock()`
    /// which returns a `MutexGuard<T>`.
    Mutex(Box<Ty>),
    /// **M08**: the guard returned by `mutex.lock()`. Carries the protected
    /// type for `*guard` deref. Drop emits `LockRelease`.
    MutexGuard(Box<Ty>),
    /// **M08**: `thread::spawn`'s return value. M08 doesn't model the
    /// closure's return type (always unit in M08 samples) — variant-less.
    /// Calling `.join()` on a JoinHandle returns `Ty::Unit`.
    JoinHandle,
}
```

### Validation rules

- **VR-4**: `Ty::name()` — `format!("Arc<{}>", inner.name())`, `format!("Mutex<{}>", inner.name())`, `format!("MutexGuard<{}>", inner.name())`, `"JoinHandle".to_owned()`.
- **VR-5**: `Ty::is_copy()` — all four return `false`.
- **VR-6**: equality is structural for Arc/Mutex/MutexGuard (recursing on inner); JoinHandle is unit-like.

## Modified: `Value` — adds 4 variants

```rust
pub enum Value {
    // ... existing variants
    /// **M08**: shared-ownership heap pointer. Multiple Value::Arc instances
    /// can carry the same `addr`; the HeapObject::Arc at that addr stores
    /// the refcount. Cloning bumps the count; dropping decrements.
    Arc {
        /// Heap address of the shared allocation.
        addr: HeapAddr,
    },
    /// **M08**: lock-protected heap value. Carries the heap address; the
    /// HeapObject::Mutex stores the value + lock holder.
    Mutex {
        /// Heap address of the lock-protected allocation.
        addr: HeapAddr,
    },
    /// **M08**: lock guard returned by `mutex.lock()`. Scope-exit Drop emits
    /// `LockRelease` for the held mutex.
    MutexGuard {
        /// Heap address of the mutex the guard holds.
        addr: HeapAddr,
    },
    /// **M08**: handle returned by `thread::spawn(closure)`. Calling
    /// `.join()` waits for the thread (drives a `ThreadJoin` event).
    JoinHandle {
        /// The spawned thread's id.
        thread_id: ThreadId,
    },
}
```

### Validation rules

- **VR-7**: multiple `Value::Arc` instances may share the same `addr` (each `Arc::clone` produces a new Value with the same addr); the HeapObject's `strong_count` field counts these instances.
- **VR-8**: `Value::MutexGuard` is a transient binding — typically bound by `let g = m.lock();`, dropped at the binding's scope exit. Cannot be moved across threads (a real-Rust restriction; M08 doesn't enforce typeck-side but samples don't construct this).
- **VR-9**: `Value::JoinHandle` is consumed by `.join()`; M08 doesn't track post-join handles.

## New addressing newtype: `ThreadId`

```rust
// In src/event.rs

/// **M08**: identifier for an evaluator thread. The main thread is always
/// `ThreadId(0)`; spawned threads get ids 1, 2, ... in spawn order.
/// Monotonic; never reused. Analog of `VtableAddr` from M07.7.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct ThreadId(pub u32);
```

### Validation rules

- **VR-10**: `ThreadId(0)` is always the main thread. M08 reserves no other ids — spawn order determines the rest.
- **VR-11**: `ThreadId` is the FOURTH addressing namespace after `HeapAddr` / `StaticAddr` / `SlotId` / `FrameId` / `BorrowId` / `VtableAddr`. (Counts vary — point is it's its own newtype.)

## New (event): `MemEvent::ThreadSwitch`

```rust
pub enum MemEvent {
    // ... existing variants (the 7 thread+sync stubs from M03 stay shape-identical)
    /// **M08**: the scheduler switched the current thread. Subsequent
    /// SlotAlloc/SlotWrite/FrameEnter/etc. events belong to this thread
    /// until the next ThreadSwitch fires. Initial state (cursor 0) implies
    /// thread 0 (main) is current.
    ThreadSwitch {
        /// The thread that is now current.
        thread_id: ThreadId,
        /// Source location of the operation that caused the switch
        /// (e.g. the `lock()` call site for park-driven switches).
        span: Span,
    },
}
```

### Validation rules

- **VR-12**: at trace start, the UI defaults to `current_thread_id = ThreadId(0)` without needing a leading ThreadSwitch — saves one event in single-threaded traces (M01-M07.7 byte-identity).
- **VR-13**: ThreadSwitch fires whenever the scheduler picks a different thread than the previous event's thread. Coalesce-able: consecutive switches without intervening events would be redundant; eval emits a switch only when the new thread is about to emit its first post-switch event.
- **VR-14**: `span` is the operation's span (e.g. the `lock()` call that parked the previous thread); used for editor-highlight pedagogy at the switch step.

## Pre-declared MemEvent payloads (formalized in M08, no shape change)

The 7 thread+sync variants declared in M03 finally get emitted. Shapes are unchanged from M03:

- `ThreadSpawn { thread_id: u32, span: Span }` — fires at `thread::spawn(closure)` evaluation. `thread_id` is the SPAWNED thread's id.
- `ThreadJoin { thread_id: u32, span: Span }` — fires at `handle.join()` evaluation. `thread_id` is the joined thread's id.
- `ThreadPark { thread_id: u32, lock: HeapAddr, span: Span }` — fires when a thread parks on a mutex.
- `ArcClone { addr: HeapAddr, span: Span }` — fires at `Arc::clone(&a)` evaluation.
- `ArcDrop { addr: HeapAddr, span: Span }` — fires at scope-exit drop of a `Value::Arc` binding (refcount decremented; HeapFree may or may not follow depending on count).
- `LockAcquire { addr: HeapAddr, span: Span }` — fires when `mutex.lock()` succeeds (either immediately on a free mutex, or after unparking from a wait).
- `LockRelease { addr: HeapAddr, span: Span }` — fires at scope-exit drop of a `Value::MutexGuard` binding.

NOTE: `thread_id` on the 3 pre-declared thread events is typed as `u32` in M03 (raw, not the new `ThreadId` newtype). M08 keeps `u32` to preserve M03 byte-identity; the eval-side passes `thread_id.0` when constructing events.

## New HeapObject variants (eval-internal)

```rust
// In src/eval.rs

enum HeapObject {
    // ... existing variants
    /// **M08**: shared-ownership boxed value. `strong_count` is incremented
    /// by `Arc::clone`, decremented by `ArcDrop`. The block is freed only
    /// when count reaches 0.
    Arc {
        /// The wrapped value.
        value: Box<Value>,
        /// Number of live `Value::Arc { addr: this_addr }` instances.
        strong_count: u32,
    },
    /// **M08**: lock-protected boxed value. `holder` is `Some(tid)` when
    /// the mutex is held by thread `tid`, `None` when free. `waiters` is
    /// a FIFO queue of threads parked on this mutex.
    Mutex {
        /// The wrapped value.
        value: Box<Value>,
        /// Thread currently holding the lock, or None if free.
        holder: Option<ThreadId>,
        /// FIFO queue of parked threads waiting to acquire the lock.
        waiters: Vec<ThreadId>,
    },
}
```

### Validation rules

- **VR-15**: `HeapObject::Arc.strong_count` is `u32`, monotonic-ally aligned to the number of live `Value::Arc { addr: this }` instances. Starts at 1 (from `Arc::new`); incremented per `Arc::clone`; decremented per scope-exit Drop of an `Arc` binding.
- **VR-16**: `HeapObject::Mutex.holder` is `Some(thread_id)` iff a `Value::MutexGuard { addr: this }` is live in some thread's scope; otherwise `None`.
- **VR-17**: `HeapObject::Mutex.waiters` is FIFO by spawn order; the first waiter unparks when the holder releases.

## Eval-side scheduler state

```rust
// In src/eval.rs

struct Evaluator<'a> {
    // ... existing fields (`fn_decls`, `methods`, etc.)
    /// **M08**: per-thread state. IndexMap preserves spawn order.
    /// Always contains ThreadId(0) (main).
    threads: indexmap::IndexMap<ThreadId, ThreadState<'a>>,
    /// **M08**: which thread is currently executing. Used to route
    /// SlotAlloc/SlotWrite/FrameEnter to the right thread's frame stack.
    current_thread_id: ThreadId,
    /// **M08**: monotonic counter for ThreadIds.
    next_thread_id: u32,
}

struct ThreadState<'a> {
    /// This thread's call stack — innermost frame last.
    frames: Vec<Frame>,
    /// Current scheduler status.
    status: ThreadStatus<'a>,
    /// Pre-queued closure body + captured bindings, populated at
    /// `thread::spawn` and consumed when the thread first runs.
    /// `None` after the body starts executing.
    queued_body: Option<QueuedBody<'a>>,
}

enum ThreadStatus<'a> {
    /// Closure body queued, not yet started.
    Ready,
    /// Currently executing (matches Evaluator.current_thread_id).
    Running,
    /// Parked on a mutex.
    Parked { lock: crate::event::HeapAddr },
    /// Parked waiting for another thread's join.
    JoinWait { target: ThreadId },
    /// Body completed; ready to be joined.
    Done,
    /// Marker to satisfy lifetime parametrization (unused — `'a` is for QueuedBody).
    _Phantom(std::marker::PhantomData<&'a ()>),
}

struct QueuedBody<'a> {
    body: &'a ast::Block,
    /// Captured bindings: (target binding name, captured value, target ty).
    captures: Vec<(String, Value, Ty)>,
    /// Span of the closure expression (for the first FrameEnter).
    span: Span,
}
```

### Validation rules

- **VR-18**: `Evaluator::new` initializes `threads` with `ThreadId(0)` (main, status `Running`); `current_thread_id = ThreadId(0)`.
- **VR-19**: `thread::spawn` allocates a fresh `ThreadId(next_thread_id++)`, inserts a `Ready` ThreadState with the queued body+captures, emits `ThreadSpawn`. Does NOT switch.
- **VR-20**: when current thread parks/joins, scheduler scans `threads` IndexMap in spawn order, picks first thread in `Ready` or `Running` (post-unpark) state, emits `ThreadSwitch`, updates `current_thread_id`. The newly-current thread runs (if `Ready`, prepares initial frame from queued_body; if `Running`/unparked, resumes).
- **VR-21**: `current_thread()` / `current_thread_mut()` accessor methods on Evaluator panic if `current_thread_id` is not in `threads` (invariant violation).

## Modified: `HeapView` — adds `refcount` + `mutex_holder`

```rust
// In src/ui.rs

pub struct HeapView {
    // ... existing fields
    /// **M08**: present for Arc heap blocks; renders as `[refs: N]` suffix
    /// on the addr line. Updated on ArcClone (increment) / ArcDrop (decrement).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refcount: Option<u32>,
    /// **M08**: present for Mutex heap blocks when held; carries the
    /// holder's ThreadId.0 for the `[locked by #N]` suffix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mutex_holder: Option<u32>,
}
```

### Validation rules

- **VR-22**: `refcount` is `Some(n)` iff the heap block is `HeapObject::Arc`; otherwise `None`. Updated by `ArcClone` / `ArcDrop` apply_event arms.
- **VR-23**: `mutex_holder` is `Some(tid)` iff the heap block is `HeapObject::Mutex` AND the mutex is currently held; `None` for free mutexes or non-mutex blocks. Updated by `LockAcquire` / `LockRelease` arms.

## New (UI): `ThreadColumnView` + `ParkedThreadView` + extended `StateSnapshot`

```rust
pub struct ThreadColumnView {
    pub thread_id: u32,
    /// Human-readable label: `"main"` for thread 0, `"thread #{id}"` for spawned.
    pub label: String,
    /// Per-thread frame stack (innermost last, same as existing FrameCardView ordering).
    pub frames: Vec<FrameCardView>,
    /// `true` for the currently-executing thread; drives a visual emphasis.
    pub is_current: bool,
    /// Thread lifecycle status: Running / Parked / Joined / Ready.
    pub status: ThreadStatusView,
}

pub enum ThreadStatusView {
    /// Running or just unparked (waiting for execution).
    Running,
    /// Parked on a mutex; `lock_addr` identifies the held mutex's heap block.
    Parked { lock_addr: u32 },
    /// Body completed; column may render grayed out.
    Joined,
    /// Queued, never executed yet (transient — between `thread::spawn` and the first ThreadSwitch into this thread).
    Ready,
}

pub struct ParkedThreadView {
    /// Identifier of the parked thread (matches a `ThreadColumnView.thread_id`).
    pub thread_id: u32,
    /// Heap addr of the mutex the thread is parked on.
    pub lock_heap_addr: u32,
}

pub struct StateSnapshot {
    // existing fields ...
    /// **M08**: per-thread columns (replaces the implicit single-stack rendering).
    /// For single-threaded programs (no spawn events), contains one entry
    /// for thread 0 — visually IDENTICAL to pre-M08 single-column rendering.
    pub threads: Vec<ThreadColumnView>,
    /// **M08**: active parking connections (dotted-line viz).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parked_threads: Vec<ParkedThreadView>,
}
```

NOTE: `StateSnapshot.frames` (the existing per-program single-stack field) is REPLACED by `StateSnapshot.threads`. This is a contract change for the UI snapshot but doesn't affect the wire-protocol (`Vec<MemEvent>`) — only the JS rendering side. The replacement is staged: for single-threaded programs, `threads[0].frames` carries the same data as the old `frames`.

### Validation rules

- **VR-24**: `threads` always non-empty (main thread is always present). For M01-M07.7 programs, `threads.len() == 1` and `threads[0].label == "main"`.
- **VR-25**: `parked_threads` populated by walking `World.threads` for entries with `status == Parked { .. }`. Cleared as soon as LockRelease unparks.

## New: M08 reference samples

| File | Content | Pedagogy |
|---|---|---|
| `tests/samples/m08_thread_spawn.rs` | `let h = thread::spawn(\|\| { let x = 5; }); h.join();` | Multi-column stacks; slide-in animation; join visible. |
| `web/samples/m08_thread_spawn.rs` | Mirror. | |
| `tests/samples/m08_arc_clone.rs` | `let a = Arc::new(5); let b = Arc::clone(&a);` | Dashed-purple arrows; refcount `[refs: 2]` visible. |
| `web/samples/m08_arc_clone.rs` | Mirror. | |
| `tests/samples/m08_mutex_contention.rs` | Two-thread Mutex contention sample. | Parked-thread visual (column greys + dotted line to mutex). |
| `web/samples/m08_mutex_contention.rs` | Mirror. | |
| `tests/samples/m08_arc_mutex.rs` | Canonical `Arc<Mutex<T>>` shared between two threads. | 🎯 HEADLINE — ALL M08 viz layers engaged simultaneously. |
| `web/samples/m08_arc_mutex.rs` | Mirror. | |
