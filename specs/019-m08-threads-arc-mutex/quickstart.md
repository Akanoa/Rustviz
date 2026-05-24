# Quickstart — M08 development + verification

Audience: maintainer + contributors working on M08 or extending it.

## Run the page

```bash
cd web && trunk serve --open
```

After M08 ships, the dropdown gains 4 entries: `Thread spawn`, `Arc clone`, `Mutex contention`, `Arc<Mutex<T>>`. The stacks panel layout grows from one column to N (one per live thread); new columns slide in from the right at spawn.

## Run all tests

```bash
cargo test                                              # full suite

cargo test --lib pipeline::tests::run_pipeline_thread_spawn              # US1
cargo test --lib pipeline::tests::run_pipeline_thread_join_visible       # US1: join sequence
cargo test --lib pipeline::tests::run_pipeline_arc_clone                 # US2
cargo test --lib pipeline::tests::run_pipeline_arc_drop_decrement        # US2: refcount transitions
cargo test --lib pipeline::tests::run_pipeline_arc_last_drop_frees       # US2: count → 0 triggers HeapFree
cargo test --lib pipeline::tests::run_pipeline_mutex_lock                # US3: basic lock + release
cargo test --lib pipeline::tests::run_pipeline_mutex_contention          # US3: parked-thread visual
cargo test --lib pipeline::tests::run_pipeline_arc_mutex                 # US4: headline
cargo test --lib pipeline::tests::run_pipeline_determinism               # FR-013/SC-010: byte-identical across runs
cargo test --lib pipeline::tests::run_pipeline_closure_outside_spawn     # rejection: closure outside thread::spawn
cargo test --lib pipeline::tests::run_pipeline_non_move_closure          # rejection: || without move
```

M01/M02/M03 should stay byte-identical (no existing sample constructs threads / Arc / Mutex).

## Manual QA procedure (with TWO UX checkpoints)

~15 minutes. UX checkpoint #1 after step 2 (multi-column layout); UX checkpoint #2 after step 4 (parked-thread visual + dashed-purple arrows + refcount).

1. **Page loads** with the default sample. No console errors. Existing M01–M07.7 samples render unchanged (one stack column for thread #0 / main).

2. **🎨 UX CHECKPOINT #1 — US1 first cut**:
   - Select `Thread spawn (M08)`. Editor shows:
     ```rust
     fn main() {
         let h = thread::spawn(|| {
             let x = 5;
         });
         h.join();
     }
     ```
   - Step past `let h = thread::spawn(...)`:
     - The stacks panel now has TWO columns: `Thread #0 (main)` on the left, `Thread #1 (spawned)` slides in from the right.
     - Thread #0's frame card shows `h: JoinHandle`. Thread #1's column is empty (queued, body not yet started).
   - Step past `h.join()`:
     - A `ThreadJoin` event fires; Thread #1's body executes (closure frame opens with `x: i32 = 5`); execution returns to Thread #0; Thread #1's column grays out (or fades).
   - **PAUSE.** Present the visualization. Discuss tweaks: column width, slide-in timing, label format ("main" vs "Thread #0"), spawned-column emptiness while queued, joined-column treatment (gray vs fade vs remove). Iterate.

3. **US2 — Arc clone + drop**:
   - Select `Arc clone (M08)`. Editor shows `let a = Arc::new(5); let b = Arc::clone(&a);`.
   - Step past `Arc::new(5)`: heap block appears `heap #0 [Arc<i32> = 5, refs: 1]`; hover `a`'s slot to reveal a dashed-purple arrow to the block.
   - Step past `Arc::clone(&a)`: refcount updates to `[refs: 2]`; an `ArcClone` event fires; hover `b`'s slot to reveal a second dashed-purple arrow to the same block.
   - At scope exit of b: refcount decrements to `[refs: 1]`; `ArcDrop` fires; heap block is NOT freed.
   - At scope exit of a (last Arc): refcount decrements to `[refs: 0]`; `ArcDrop` + `HeapFree` fire; heap block grayed.

4. **🎨 UX CHECKPOINT #2 — US3 first cut**:
   - Select `Mutex contention (M08)`. Editor shows a two-thread mutex contention sample.
   - Step until Thread A holds the lock (`LockAcquire` event; heap block shows `[Mutex<i32> = 0, locked by #0]`).
   - Step past Thread B's `lock()` attempt:
     - A `ThreadPark` event fires; Thread B's column greys out + low-opacity.
     - A dotted-purple line is drawn from Thread B's column header to the mutex's heap block.
     - A `ThreadSwitch` event fires; control returns to Thread A.
   - Step past Thread A's guard drop:
     - `LockRelease` fires; mutex's `[locked by #0]` suffix clears.
     - Thread B unparks; `LockAcquire` for Thread B fires; mutex now shows `[locked by #1]`; B's column resumes normal opacity.
   - **PAUSE.** Present the visualization. Discuss tweaks: parked opacity level, dotted-line color/style, refcount/holder display format, Arc-arrow color saturation, hover-only vs always-on for parked-line. Iterate.

5. **US4 — `Arc<Mutex<T>>` (THE HEADLINE)**:
   - Select `Arc<Mutex<T>> (M08)`. Editor shows the canonical pattern: `let m = Arc::new(Mutex::new(0)); let m2 = Arc::clone(&m); thread::spawn(move || { let mut g = m2.lock(); *g += 1; }); let mut g = m.lock(); *g += 1;`.
   - Step through:
     - Heap block `heap #0 [Arc<Mutex<i32>> = 0, refs: 2]` after both Arc::new + Arc::clone.
     - Spawn fires; Thread #1 column slides in (queued).
     - Main locks: `LockAcquire` for thread 0; mutex shows `[locked by #0]`.
     - Switch to Thread #1; Thread #1 tries to lock; parks (greys + dotted line).
     - Main mutates `*g += 1` (heap value updates to 1).
     - Main's guard drops: `LockRelease`. Thread #1 unparks; LockAcquire for thread 1; mutex shows `[locked by #1]`.
     - Thread #1 mutates `*g += 1` (heap value updates to 2); guard drops.
     - Main joins; cleanup; refcount goes to 0; HeapFree.

6. **Error UX** — live editing:
   - Try `let f = || { let x = 5; };` (closure NOT inside thread::spawn) → typeck error "closures are only supported as `thread::spawn` arguments in M08".
   - Try `thread::spawn(|| { let x = 5; });` (without `move`) → typeck error "thread::spawn requires a `move` closure in M08 (borrowing captures across thread boundaries is out of scope)".
   - Try `Arc::clone(&5)` (Arc::clone on a non-Arc) → typeck error.

7. **No regressions**:
   - Cycle through M01–M07.7 samples. Each renders correctly. Stacks panel shows ONE column (thread 0 / main) for all single-threaded programs.

## Developer notes

### Why a cooperative scheduler instead of OS threads?

The visualizer needs deterministic event streams (FR-013/SC-010). Real OS threads would produce non-deterministic interleaving — the same source would produce different traces across runs depending on scheduling decisions. The cooperative scheduler runs everything sequentially with rule-driven yield points; output is reproducible byte-for-byte.

### Why yield only at lock contention + join, not per-step?

Yielding per cursor step (round-robin) would produce alternating events from random threads without semantic reason — confusing pedagogy. Yielding ONLY at contention points (lock-held, join-wait) means: "main runs until something forces it to wait, then spawned runs, etc." This matches the learner's mental model of "a thread is doing work until it gets blocked".

### Why no `Send`/`Sync` typeck?

Per MILESTONES.md Deferred entry: full auto-trait inference is rustc-grade work, out of scope for a pedagogical visualizer. The visualizer accepts programs that capture non-Send data (incorrect Rust, but the visualizer doesn't enforce). The headline pedagogy is the runtime behavior of `Arc<Mutex<T>>`, not the type-system proof.

### Why `move ||` only?

Non-move closures borrow their captures — cross-thread borrows would need lifetime tracking that's well beyond M08's scope. `move` semantically transfers the binding, making the cross-thread story tractable: the value is copied into the spawned thread, full stop.

### Why ThreadId(0) is always main + no leading ThreadSwitch?

Saves one event per trace. The UI defaults to thread 0 on cursor 0; the first ThreadSwitch fires only when execution actually leaves main. Single-threaded programs never emit ThreadSwitch — preserves M01-M07.7 byte-identity.

### Why `Value::Arc` has no refcount field?

Refcount lives in `HeapObject::Arc.strong_count`. Multiple `Value::Arc { addr }` instances share the same addr; bundling the count INTO the Value would force cloning the count alongside the addr, which would corrupt the invariant. The HeapObject is the single source of truth.

### Why `MemEvent::ThreadSwitch`?

The alternative — adding a `thread_id` field to EVERY MemEvent variant — would be invasive (every existing variant restructures, breaks compat). A separate ThreadSwitch event lets the UI maintain `current_thread_id` as a stateful side-table and route subsequent events to that thread's column. Minimal protocol surface, clean separation.

### How does the UI render multi-column stacks?

`#stacks`'s inner becomes a horizontal flex container. Each thread gets a `.thread-column[data-thread-id="{id}"]` child with the existing per-frame rendering inside. Columns are equal-width when ≤ 3 active; horizontal scroll past 3. The slide-in animation (`@keyframes thread-slide-in`) triggers on first render of a new column.

### How does the dotted line for parked threads draw?

SVG overlay path from the parked column's header (top-left bbox) to the held mutex's heap-block bbox. Stroke: dotted purple. Lifecycle: visible while the parking persists; removed on LockRelease.

### Refcount display

`HeapView.refcount: Option<u32>` — present for Arc heap blocks. JS renders as `[refs: N]` suffix on the heap-block addr line. Updated via apply_event's ArcClone (increment) / ArcDrop (decrement) arms.

### Mutex holder display

`HeapView.mutex_holder: Option<u32>` — present for held Mutex heap blocks. JS renders as `[locked by #N]` suffix. Updated by LockAcquire (set) / LockRelease (clear).

## When extending in future milestones

Future Level 5+ work (closures with parameters, generic closure types via `Fn`/`FnMut`/`FnOnce`, async/await, channels, scoped threads, atomics) would build on M08's thread scheduler + Value variants. The cooperative scheduler can extend to support `mpsc` channels (queue-based wakeup), `Condvar` (manual unpark), `try_lock` (non-blocking probe), etc. The closure surface generalizes when needed.

After M08, Level 4 is complete. CLAUDE.md doesn't currently define Level 5+.

## What this milestone does NOT add

- **`Send`/`Sync` typeck** — deferred.
- **Poisoned mutex / panic propagation** — out of scope.
- **`Mutex::try_lock`** — out of scope.
- **`RwLock`, `Condvar`, atomics** — out of scope.
- **Channels (`mpsc`)** — out of scope.
- **`async`/`await`** — out of scope.
- **Scoped threads (`thread::scope`)** — out of scope.
- **Non-move closures** — out of scope (cross-thread borrowing).
- **Closures with parameters** — out of scope.
- **`Weak<T>`** — out of scope.
- **`thread::current()`, `thread::sleep()`** — out of scope.
- **fn-pointer `thread::spawn(some_fn)`** — out of scope.
- **General `Fn`/`FnMut`/`FnOnce` trait dispatch** — out of scope.
