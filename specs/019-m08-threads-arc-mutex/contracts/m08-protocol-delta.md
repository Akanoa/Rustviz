# Contract — M08 Protocol Delta

**12th invocation** of the closed-enum-with-revisions rule. M08 adds AST surface + new `Ty` variants + new `Value` variants + a new `ThreadId` addressing namespace + a new `MemEvent::ThreadSwitch` event + new `HeapObject` variants (eval-internal). The 7 thread+sync MemEvent variants pre-declared at M03 (`ThreadSpawn`, `ThreadJoin`, `ThreadPark`, `ArcClone`, `ArcDrop`, `LockAcquire`, `LockRelease`) finally get their payload semantics emitted — variant SHAPES stay byte-identical to M03's declarations.

## Closed-enum rule — twelfth invocation

| Milestone | Change |
|---|---|
| M03.1 | Added `MemEvent::ReturnValue`; removed `FrameEnter.params`. |
| M03.2 | Restructured `Ty` and `Value` (kind-based). |
| M06 | Added `Ty::Ref`, `Value::Ref`. |
| M07 | Added `Ty::Box/Vec/String`, `Value::Box/Vec/String`. Restructured `Value::Ref`. |
| M07.1 | Added `Ty::Slice`, `Value::Slice`. |
| M07.2 | Added `StaticAddr`, `Pointee::Static`, `Ty::Str`, `MemEvent::StaticAlloc/BytesCopy`. |
| M07.3 | Added `Ty::Array`, `Value::Array`. |
| M07.4 | Added `Ty::Struct`, `Value::Struct`. Extended `Value::Ref` with `field_path`. |
| M07.5 | Added `Ty::Param`. Extended `Ty::Struct` with `type_args`. |
| M07.6 | AST-only additions; no event-protocol changes. |
| M07.7 | Added `Ty::DynRef/BoxDyn`, `Value::DynRef/BoxDyn`, `VtableAddr`, `MemEvent::VtableAlloc`. |
| **M08** | Added `Ty::Arc/Mutex/MutexGuard/JoinHandle`, `Value::Arc/Mutex/MutexGuard/JoinHandle`, `ThreadId`, `MemEvent::ThreadSwitch`. Pre-declared M03 thread+sync MemEvent variants get their payloads emitted (no shape change). AST: `Expr::Closure`. |

## `Ty` — additive variants

```rust
pub enum Ty {
    // ... existing variants
    // NEW in M08:
    Arc(Box<Ty>),
    Mutex(Box<Ty>),
    MutexGuard(Box<Ty>),
    JoinHandle,
}
```

JSON shapes (analog of `Ty::Box` / `Ty::Vec`):
- `Arc(Ty::Int(I32))`: `{ "Arc": { "Int": "I32" } }`
- `Mutex(Ty::Int(I32))`: `{ "Mutex": { "Int": "I32" } }`
- `MutexGuard(Ty::Int(I32))`: `{ "MutexGuard": { "Int": "I32" } }`
- `JoinHandle`: `"JoinHandle"` (unit-like)

## `Value` — additive variants

```rust
pub enum Value {
    // ... existing variants
    // NEW in M08:
    Arc { addr: HeapAddr },
    Mutex { addr: HeapAddr },
    MutexGuard { addr: HeapAddr },
    JoinHandle { thread_id: ThreadId },
}
```

JSON shapes:
- `Arc`: `{ "Arc": { "addr": N } }`
- `Mutex`: `{ "Mutex": { "addr": N } }`
- `MutexGuard`: `{ "MutexGuard": { "addr": N } }`
- `JoinHandle`: `{ "JoinHandle": { "thread_id": N } }`

## New addressing namespace: `ThreadId`

```rust
pub struct ThreadId(pub u32);
```

JSON: bare integer (the `u32` payload). Distinct from `HeapAddr`, `StaticAddr`, `SlotId`, `FrameId`, `BorrowId`, `VtableAddr`. Main thread is always `ThreadId(0)`.

## `MemEvent` — additive variant (`ThreadSwitch`)

```rust
pub enum MemEvent {
    // ... existing variants (including 7 pre-declared thread+sync variants)
    ThreadSwitch {
        thread_id: ThreadId,
        span: Span,
    },
}
```

JSON shape: `{ "ThreadSwitch": { "thread_id": N, "span": <Span> } }`. Fires whenever the cooperative scheduler picks a different thread. Initial state (cursor 0) implies main is current — no leading ThreadSwitch needed.

## `MemEvent` — pre-declared variant payloads finalized (no shape change)

The 7 thread+sync variants declared in M03 with concrete field types are finally emitted in M08. SHAPES UNCHANGED from M03:

```rust
ThreadSpawn { thread_id: u32, span: Span }
ThreadJoin { thread_id: u32, span: Span }
ThreadPark { thread_id: u32, lock: HeapAddr, span: Span }
ArcClone { addr: HeapAddr, span: Span }
ArcDrop { addr: HeapAddr, span: Span }
LockAcquire { addr: HeapAddr, span: Span }
LockRelease { addr: HeapAddr, span: Span }
```

NOTE: `thread_id` on the 3 pre-declared thread events stays `u32` (M03 declaration). The new `ThreadId` newtype is used internally + on `MemEvent::ThreadSwitch.thread_id`. The eval-side passes `tid.0` when constructing these legacy events to preserve M03 byte-identity.

## `Pointee` — no changes

M08 doesn't introduce new pointee categories. Arc/Mutex/MutexGuard borrows (if any) target the underlying heap allocation via `Pointee::Heap(addr)`.

## `HeapObject` — additive variants (eval-internal, not in wire protocol)

```rust
enum HeapObject {
    // ... existing variants (Box, Vec, Str)
    Arc { value: Box<Value>, strong_count: u32 },
    Mutex { value: Box<Value>, holder: Option<ThreadId>, waiters: Vec<ThreadId> },
}
```

## AST — additive Expr (parser-side, not in wire protocol)

```rust
pub enum Expr {
    // ... existing
    Closure { is_move: bool, body: Block, span: Span },
}
```

## Behavioral guarantees (post-M08)

- **B-M08-1**: `Evaluator::new` initializes the main thread (id 0) with status `Running`. No `ThreadSwitch` event fires at trace start (UI defaults to thread 0).
- **B-M08-2**: `thread::spawn(closure)` allocates a fresh `ThreadId` (monotonic, never reused), inserts a `Ready` ThreadState, emits `MemEvent::ThreadSpawn { thread_id, span }`. Does NOT switch the current thread.
- **B-M08-3**: When the cooperative scheduler picks a different thread (current thread parks or completes), `MemEvent::ThreadSwitch { thread_id, span }` fires BEFORE any subsequent events from the new thread.
- **B-M08-4**: `Arc::new(v)` allocates `HeapObject::Arc { value: Box::new(v), strong_count: 1 }`, emits `MemEvent::HeapAlloc`. Returns `Value::Arc { addr }`.
- **B-M08-5**: `Arc::clone(&a)` increments the HeapObject's strong_count, emits `MemEvent::ArcClone { addr, span }`. Returns a new `Value::Arc { addr }` sharing the original's addr.
- **B-M08-6**: Scope-exit Drop on a `Value::Arc { addr }` binding decrements strong_count + emits `MemEvent::ArcDrop { addr, span }`. If strong_count reaches 0, ALSO emits `MemEvent::HeapFree { addr, span }`.
- **B-M08-7**: `Mutex::new(v)` allocates `HeapObject::Mutex { value, holder: None, waiters: vec![] }`, emits `MemEvent::HeapAlloc`. Returns `Value::Mutex { addr }`.
- **B-M08-8**: `mutex.lock()` checks the HeapObject's `holder`:
  - `None`: set `holder = Some(current_thread_id)`, emit `MemEvent::LockAcquire { addr, span }`, return `Value::MutexGuard { addr }`.
  - `Some(other)`: push current_thread_id to waiters, emit `MemEvent::ThreadPark { thread_id, lock: addr, span }`, scheduler switches to next ready thread (with `ThreadSwitch`).
- **B-M08-9**: Scope-exit Drop on a `Value::MutexGuard { addr }` binding clears the HeapObject's `holder`, emits `MemEvent::LockRelease { addr, span }`. If waiters non-empty, pop the first (FIFO), set holder to that thread, emit `LockAcquire` for the unparked thread, change unparked thread's status to `Running` (will be selected on next scheduler switch).
- **B-M08-10**: `handle.join()` checks the joined thread's status:
  - `Done`: emit `MemEvent::ThreadJoin { thread_id, span }`, return unit.
  - Other: change current thread's status to `JoinWait { target }`, scheduler switches to a ready thread.
- **B-M08-11**: Event stream is deterministic — same input source produces byte-identical `Vec<MemEvent>` across runs (FR-013/SC-010). The cooperative scheduler uses rule-driven selection (IndexMap iteration order = spawn order); no system clocks, randomness, or hash-iteration variance.
- **B-M08-12**: `Value::Arc` carries no refcount field; the count lives in `HeapObject::Arc.strong_count`. The UI reads the count from the heap-block apply_event arm (updates `HeapView.refcount` on ArcClone/ArcDrop).
- **B-M08-13**: Single-threaded programs (no `thread::spawn`) produce ZERO new MemEvent variants (no ThreadSwitch, no Thread*, no Arc*, no Lock*). M01–M07.7 traces stay byte-identical.
- **B-M08-14**: M01-M07.7 snapshot tests stay byte-identical: additive Value variants don't change serialization of existing Value variants; new HeapView/StateSnapshot fields use serde-default-empty so existing JSON omits them.
- **B-M08-15**: The `StateSnapshot.threads: Vec<ThreadColumnView>` field replaces the implicit single-stack rendering. For single-threaded programs (no spawn events), `threads.len() == 1`, `threads[0].label == "main"`, `threads[0].frames` mirrors the prior single-frame-list shape.

## What this contract does NOT cover (deferred)

- **`Send`/`Sync` auto-trait inference** — out of scope. M08 accepts programs that capture non-Send data.
- **Poisoned mutex** — `Mutex::lock()` returns `MutexGuard` directly, not `Result<MutexGuard, PoisonError>`. No panic propagation across threads.
- **`Mutex::try_lock`** — out of scope (would short-circuit the parking pedagogy).
- **`RwLock`, `Condvar`, atomics** — out of scope.
- **Channels (`mpsc`)** — out of scope.
- **`async`/`await`** — out of scope.
- **`thread::scope`** — scoped threads with borrowing captures — out of scope.
- **Non-move closures** (`||` without `move` — borrowing captures) — out of scope.
- **Closures with parameters** (`|x| { body }`) — out of scope; M08 only needs no-arg closures for `thread::spawn`.
- **Multiple threads spawning more threads** (transitive spawn) — IN scope (eval supports nested spawn since each thread runs in its own context).
- **Arc<Mutex<T>> with N > 2 threads** — IN scope; FIFO waiter queue handles N waiters.
- **`Weak<T>`** — out of scope.
- **`thread::current()` / `thread::sleep()`** — out of scope.
- **fn-pointer `thread::spawn(some_fn)`** — out of scope; only `move ||` closures.
- **General `Fn`/`FnMut`/`FnOnce` trait dispatch** — out of scope; M08's closure surface is `thread::spawn`-only.
