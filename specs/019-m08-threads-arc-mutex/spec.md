# Feature Specification: M08 — Level 4: threads (`thread::spawn`, `Arc`, `Mutex`)

**Feature Branch**: `019-m08-threads-arc-mutex`
**Created**: 2026-05-24
**Status**: Draft
**Input**: User description: "M08"

**Authoritative scope source**: [`MILESTONES.md` › M08 — Level 4: threads (thread::spawn, Arc, Mutex)](../../MILESTONES.md). The In scope / Out of scope / Entry criteria / Exit criteria / Demo lines in that block are the contract for this feature. This spec elaborates user stories, requirements, and success criteria; it does not redefine scope.

## User Scenarios & Testing *(mandatory)*

M08 closes the Level 4 surface: after M07.5/M07.6/M07.7 shipped the polymorphism trilogy, M08 ships **concurrency** — `thread::spawn`, `Arc`, `Mutex`. This is the visualizer's first multi-execution-context milestone: the stacks panel grows from one column to N (one per live thread), execution branches and rejoins, and the heap acquires a new lifetime semantic (`Arc` keeps an allocation alive while any clone exists, decremented on each drop). The headline pedagogy is making **shared state under exclusive lock visible**: an `Arc<Mutex<T>>` value cloned across threads, with one thread holding the lock while another visibly parks waiting on it.

### User Story 1 - `thread::spawn` + `.join()` — multi-column stacks (Priority: P1)

A learner writes `let h = thread::spawn(|| { ... }); h.join();`. The stacks panel renders the new thread as a **second column** sliding in from the right at the spawn step. The new thread's body executes in its own column (own frames, own slots) concurrently with the spawning thread, then closes when `.join()` completes. Two `FrameEnter`/`FrameLeave` flows visible side by side.

**Why this priority**: this IS the foundational pedagogy of concurrency. Without multi-column stacks, threads are abstract; with them, "the program now has two stacks of frames" is immediate. P1.

**Independent Test**: load `m08_thread_spawn.rs`, step past `thread::spawn`, observe a second stack column slide in. Step through both columns' bodies; step past `.join()`, observe the spawned column close.

**Acceptance Scenarios**:

1. **Given** `let h = thread::spawn(|| { let x = 5; }); h.join();`, **When** the cursor passes `thread::spawn`, **Then** a new stack column appears for the spawned thread (with its own frame for the closure body); the main thread's column remains visible alongside.
2. **Given** the spawned closure runs to completion, **When** the cursor passes `.join()`, **Then** a `ThreadJoin` event fires; the spawned column visually closes (or grays out) and the main thread's column re-takes focus.
3. **Given** two `thread::spawn` calls in succession, **When** the cursor passes both, **Then** THREE stack columns are visible simultaneously (main + two spawned).

---

### User Story 2 - `Arc::new` + `Arc::clone` — shared ownership (Priority: P1)

A learner writes `let a = Arc::new(5); let b = Arc::clone(&a);`. Both `a` and `b` bindings point at the SAME heap allocation; visually this is two **dashed purple arrows** from each binding's slot to the same heap block. The heap block carries a visible reference count (e.g. `Arc<i32> = 5  [refs: 2]`). Dropping a binding decrements the count without freeing the allocation; the allocation is only freed when the last `Arc` drops.

**Why this priority**: `Arc` is the "share by reference-counting" half of `Arc<Mutex<T>>`. Without `Arc::clone` visualization (and a visible refcount), learners can't see the "this allocation has multiple owners" semantic that distinguishes `Arc` from `Box`. P1.

**Independent Test**: load `m08_arc_clone.rs`, step past `Arc::clone`, observe two dashed purple arrows from `a` and `b` to the same heap block + refcount = 2.

**Acceptance Scenarios**:

1. **Given** `let a = Arc::new(5);`, **When** the cursor passes the let, **Then** a heap block appears (`Arc<i32> = 5  [refs: 1]`) AND a dashed purple arrow from a's slot to the block.
2. **Given** `let b = Arc::clone(&a);`, **When** the cursor passes the let, **Then** the heap block's refcount increments to 2 AND a second dashed purple arrow from b's slot to the same block; an `ArcClone` event fires.
3. **Given** b goes out of scope, **When** the cursor passes the block's closing `}`, **Then** the heap block's refcount decrements to 1 (NOT freed); an `ArcDrop` event fires; b's arrow disappears (a's remains).
4. **Given** a also goes out of scope (last Arc), **When** the cursor passes that closing `}`, **Then** the refcount decrements to 0 AND `HeapFree` fires for the block.

---

### User Story 3 - `Mutex::lock` + `unlock` + parked-thread visual (Priority: P1)

A learner writes a two-thread program that contends on the same `Mutex`. Thread A locks first; thread B's `lock()` attempt **parks**: B's column visually greys out, with a **dotted line** drawn from B's column header to the slot holding the mutex (or to the mutex's heap block when used through Arc). When A releases the lock (guard scope ends), B unparks and proceeds.

**Why this priority**: parked-thread visualization is the headline UI investment of M08. Without it, mutex contention is invisible — `lock()` looks like a no-op. With it, the "B is waiting on A" relationship is concrete. P1.

**Independent Test**: load `m08_mutex_contention.rs`, step until thread A holds the lock; step thread B's `lock()` attempt; observe B's column greys + dotted line to mutex. Step past A's guard drop; observe B unpark.

**Acceptance Scenarios**:

1. **Given** thread A executes `let g = m.lock();`, **When** the cursor passes the `lock()`, **Then** a `LockAcquire` event fires; A holds the mutex.
2. **Given** thread B then executes `let g = m.lock();`, **When** the cursor passes B's `lock()`, **Then** a `ThreadPark` event fires; B's column greys out AND a dotted line is drawn from B's column header to the mutex's slot/heap-block.
3. **Given** A's guard goes out of scope, **When** the cursor passes the closing `}`, **Then** a `LockRelease` event fires; B unparks (column returns to normal opacity); B's `LockAcquire` fires immediately after.

---

### User Story 4 - `Arc<Mutex<T>>` end-to-end (Priority: P2) HEADLINE

A learner writes the canonical concurrency pattern: `let m = Arc::new(Mutex::new(0)); let m2 = Arc::clone(&m); thread::spawn(move || { let mut g = m2.lock(); *g += 1; });` (then main also locks). The visualization combines all three foundational pieces: two stack columns, two dashed-purple Arc arrows pointing at the shared `Mutex<i32>` heap block, and the parked-thread visual when contention happens.

**Why this priority**: this is the SHIP-DEFINING moment for M08. After this sample, the learner has seen the standard "share T between threads safely" pattern with every visualization layer engaged simultaneously. P2 (not P1 because US1-US3 are foundational; this is the synthesis cap).

**Independent Test**: load `m08_arc_mutex.rs`, step through both threads' operations, observe all M08 visualizations engaged together.

**Acceptance Scenarios**:

1. **Given** the canonical `Arc<Mutex<T>>` setup, **When** main spawns a thread that captures `m2`, **Then** the spawned column appears; two dashed-purple arrows (one from main's `m`, one from spawned's `m2`) point at the shared mutex heap block; the refcount on the block is 2.
2. **Given** both threads attempt to lock, **When** one succeeds and the other parks, **Then** the parked-thread visual fires as in US3; the lock-holder's mutation is visible inside the heap block.
3. **Given** both threads complete, **When** all Arc bindings are out of scope, **Then** the refcount drops to 0 AND `HeapFree` fires for the block.

---

### Edge Cases

- **Three or more threads spawned simultaneously** — IN scope. Stacks panel handles N columns; each thread gets its own column.
- **`thread::spawn` without `.join()`** — IN scope. The spawned thread runs to completion; the spawning thread doesn't wait. Visually: the spawned column closes when its body completes, regardless of what the spawner is doing.
- **`Arc::strong_count()` introspection** — out of scope. The refcount is visible on the heap block's display; programmatic access via `strong_count()` is not in M08.
- **`Arc::weak_count()` / `Weak<T>`** — out of scope.
- **Nested `Mutex<Arc<T>>`** — out of scope (M08 supports `Arc<Mutex<T>>`, the canonical shape; the reverse nesting is unusual and not part of the headline).
- **Poisoned mutex** (thread panics while holding) — out of scope (the project doesn't model panics).
- **`Mutex::lock()` returning `Result`** — simplified. M08 treats `lock()` as returning the guard directly (Rust's `Result<MutexGuard, PoisonError>` is collapsed to "guard" since poison is out of scope).
- **`Mutex::try_lock` / non-blocking acquisition** — out of scope (M08 visualizes the parking pedagogy, which `try_lock` would short-circuit).
- **Multi-thread `Arc::clone` race** — the event stream is deterministic per FR-013; thread scheduling is fixed (round-robin per step or interleaved per a deterministic rule), so events appear in a stable order across runs.
- **`Sync`/`Send` auto-trait checking** — out of scope per MILESTONES.md (explicit Deferred entry). The program assumes T satisfies the bounds without typeck-side proof.
- **Spawned closure capturing non-`Send` data** — out of scope (no Send checking). The visualizer accepts any closure body.
- **`thread::current()`, `thread::sleep()`, scoped threads (`thread::scope`)** — out of scope.
- **Channels (`std::sync::mpsc`)** — out of scope.
- **`RwLock`, `Condvar`, atomics** — out of scope.
- **`async`/`await`** — out of scope.
- **Closures in M08** — closures only appear as the argument to `thread::spawn` (their first appearance in the visualizer). The closure surface is minimal: `move || { body }` capturing local Arc bindings by clone. No closure type inference beyond this; no `Fn`/`FnMut`/`FnOnce` trait dispatch.
- **`thread::spawn` taking a non-closure (fn pointer)** — out of scope.
- **Panic propagation across `join()`** — out of scope.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST parse `thread::spawn(|| { ... })` and `thread::spawn(move || { ... })` — minimal closure surface (move closures only; the captured bindings flow through the existing local-binding mechanism).
- **FR-002**: System MUST parse `Arc::new(value)` and `Arc::clone(&arc)` as path-call expressions (same shape as M07's `Box::new`).
- **FR-003**: System MUST parse `Mutex::new(value)` and `mutex.lock()` (method call returning a guard whose Drop releases the lock).
- **FR-004**: System MUST extend the value representation with `Value::Arc { addr, ... }` and `Value::Mutex { addr, ... }` (heap-allocated like Box; Arc carries refcount metadata).
- **FR-005**: System MUST extend the value representation with `Value::MutexGuard { mutex_addr, ... }` — the lock guard whose scope-exit Drop releases the lock.
- **FR-006**: System MUST emit `MemEvent::ThreadSpawn { thread_id, span }` when a thread is spawned (filling M03's stub variant).
- **FR-007**: System MUST emit `MemEvent::ThreadJoin { thread_id, span }` when a thread is joined.
- **FR-008**: System MUST emit `MemEvent::ThreadPark { thread_id, lock, span }` when a thread parks on a mutex.
- **FR-009**: System MUST emit `MemEvent::ArcClone { addr, span }` on `Arc::clone(&a)` (Arc refcount increments).
- **FR-010**: System MUST emit `MemEvent::ArcDrop { addr, span }` when an `Arc` goes out of scope (Arc refcount decrements; `HeapFree` fires only when count reaches 0).
- **FR-011**: System MUST emit `MemEvent::LockAcquire { addr, span }` when `mutex.lock()` succeeds and the guard is bound.
- **FR-012**: System MUST emit `MemEvent::LockRelease { addr, span }` when the guard goes out of scope.
- **FR-013**: System MUST produce a deterministic event stream for any multi-threaded program — same input → byte-identical `Vec<MemEvent>` across runs. Thread scheduling is fixed (e.g. round-robin per step, or deterministic interleaving driven by the AST walk order).
- **FR-014**: System MUST render the stacks panel with one column per live thread. Spawning a thread slides a new column in from the right. Joining a thread closes (or grays) the column.
- **FR-015**: System MUST render the parked-thread visual: when a `ThreadPark` event fires for thread T on mutex M, T's column grays out AND a dotted line is drawn from T's column header to M's slot (or to M's heap block when M is held via Arc).
- **FR-016**: System MUST render `Arc::clone` arrows as **dashed purple** (CSS class distinct from blue/red/black/orange) — visually distinguishes shared-ownership from single-ownership (`Box`) and from borrowing (`&`/`&mut`).
- **FR-017**: System MUST display the Arc's refcount on the heap block's display (e.g. `Arc<i32> = 5 [refs: 2]`).
- **FR-018**: System MUST defer freeing an Arc's underlying allocation until the refcount reaches 0 (an `ArcDrop` event whose post-decrement count is > 0 does NOT trigger a `HeapFree`).
- **FR-019**: System MUST ship at least 3 new reference programs (`tests/samples/m08_*.rs` + `web/samples/`): basic spawn + join, Arc clone + drop, Arc<Mutex<T>> with contention.
- **FR-020**: System MUST preserve all M01–M07.7 existing tests byte-identical for programs that don't use threads (additive `Value` / event variants, serde-default-empty on new fields).

### Key Entities

- **`Value::Arc`** (runtime value): heap-allocated shared-ownership wrapper. Carries the heap address; the heap object stores both the wrapped value AND the strong refcount. Cloning bumps the count; dropping decrements.
- **`Value::Mutex`** (runtime value): heap-allocated lock-protected value. Carries the heap address; the heap object stores both the wrapped value AND the lock state (free / held-by-thread-N).
- **`Value::MutexGuard`** (runtime value): a transient binding produced by `mutex.lock()`. Its Drop releases the lock. Visually rendered as `MutexGuard<T> → heap[N]` with a distinctive treatment (highlighted while in scope).
- **Thread** (eval-side entity): execution context with its own frame stack, its own slot-id allocator, and its own current source span. `thread::spawn` creates a fresh thread; `.join()` waits for it.
- **`MemEvent::ThreadSpawn` / `ThreadJoin` / `ThreadPark`**: control-flow events that drive the multi-column stacks visualization.
- **`MemEvent::ArcClone` / `ArcDrop`**: refcount-changing events for the heap-block display refresh.
- **`MemEvent::LockAcquire` / `LockRelease`**: lock-state events for the parked-thread visualization.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After M08 ships, `let h = thread::spawn(|| { let x = 5; }); h.join();` parses, evaluates, and renders with TWO stack columns (main + spawned) visible during the spawned thread's lifetime.
- **SC-002**: A `ThreadSpawn` event fires at the spawn step; a `ThreadJoin` event fires at the join step; both columns are visible between those two cursor positions.
- **SC-003**: `let a = Arc::new(5); let b = Arc::clone(&a);` produces a heap block showing `[refs: 2]` AND two dashed-purple arrows (one from each of a, b) pointing at the same block.
- **SC-004**: When a binding holding an `Arc` goes out of scope, the heap block's refcount decrements (visible in the display); the block is freed only when the count reaches 0.
- **SC-005**: A two-thread `Mutex` contention sample parks the loser visibly: its column greys out AND a dotted line connects its column header to the mutex slot.
- **SC-006**: When the lock-holder's guard goes out of scope, a `LockRelease` event fires; the parked thread unparks (column resumes normal opacity) AND immediately acquires the lock (`LockAcquire` for the unparked thread fires on the next cursor step).
- **SC-007**: A canonical `Arc<Mutex<T>>` sample renders ALL M08 visualizations simultaneously: multi-column stacks, dashed purple Arc arrows, parked-thread visual, refcount display.
- **SC-008**: At least 3 new `m08_*.rs` reference programs ship.
- **SC-009**: Existing M01–M07.7 tests pass byte-identical (additive variants, serde-default-empty preserves existing snapshots).
- **SC-010**: Multi-threaded programs produce deterministic event streams — same input → byte-identical `Vec<MemEvent>` across two runs (FR-013).
- **SC-011**: WASM bundle growth ≤ +25% vs M07.7 baseline (~399 KB → ≤ ~498 KB raw post-staged). Substantial new surface: thread eval + Arc/Mutex value variants + multi-column UI + dashed-purple arrows + parked-thread visual.
- **SC-012**: Zero warnings under `RUSTFLAGS="-D warnings" cargo build --release` AND `cargo test`. Both host and WASM targets clean.

## Assumptions

- **Deterministic scheduling**: thread interleaving is fixed by a deterministic rule (e.g. round-robin per cursor step, or sequential by spawn order with explicit yield-points at `lock()`/`join()`). The event stream is reproducible across runs.
- **Move-only closures**: `thread::spawn` accepts `move || { ... }` closures only. Non-`move` closures (borrowing captures) are out of scope — they'd require borrow-checking across thread boundaries.
- **No `Send`/`Sync` typeck**: the visualizer assumes captured types satisfy the bounds. A program that captures non-Send data is accepted (incorrect Rust, but the visualizer doesn't enforce).
- **Refcount visible on heap display**: Arc's strong count is rendered as a `[refs: N]` suffix on the heap block's display string (no separate UI panel).
- **Mutex guard rendered as a stack slot**: `let g = m.lock();` produces a slot in the calling frame named `g` with type `MutexGuard<T>`. The guard's Drop is visualized as a `LockRelease` event when the slot goes out of scope.
- **No nested locks** (initial scope): a thread holding lock A then attempting to lock B is out of scope for M08's headline. If it appears in samples, it's left undefined.
- **Multi-column stacks layout**: columns are equal-width, rendered left-to-right in spawn order. Main is always column 0. The stacks panel scrolls horizontally if N > 3 (or similar). Exact layout details are a plan-phase decision.
- **Dashed-purple Arc arrows**: distinct visual treatment per FR-016. The exact stroke + dash pattern is a plan/UX-checkpoint decision (recommendation: dashed purple, same dash pattern as M07.7's orange dispatch arrows for visual family).
- **Parked-thread dotted line**: drawn from the parked column's header (e.g. the frame card top) to the mutex's heap block (when held via Arc) or to the mutex's slot (when held directly on the stack). Exact endpoint choice is a plan-phase decision.
- **Hover-only arrows continue**: per the post-M07.7 polish (`[[feedback-arrow-viz-rules]]` Rule 1), the dashed-purple Arc arrows default to hover-only — consistent with all other arrow flavors. Hovering a binding holding an Arc reveals its arrow to the shared heap block.
- **No new MemEvent variants beyond the 7 M03-declared stubs**: M03 declared `ThreadSpawn`, `ThreadJoin`, `ThreadPark`, `ArcClone`, `ArcDrop`, `LockAcquire`, `LockRelease` as stubs (FR-006 through FR-012 fill in payloads). M08 ships those payloads without adding NEW variants — true to M03's "closed enum" promise (the variants already exist with shapes; M08 emits them).
- **Refcount lives in the heap object**: HeapObject grows an `Arc { value, strong_count }` variant; `ArcClone`/`ArcDrop` mutate `strong_count`. No separate per-allocation refcount registry.
- **Mutex state lives in the heap object**: HeapObject grows a `Mutex { value, holder: Option<ThreadId> }` variant. `LockAcquire`/`LockRelease` mutate `holder`.
- **`Send`/`Sync` are NOT in the trait registry**: even though they're real Rust traits, M08 doesn't register them as known traits (matches the "no auto-trait inference" exclusion). A program writing `impl Send for Foo` would error as "Send is not declared" — acceptable since this is out of scope.
- **12th invocation candidate of the closed-enum-with-revisions rule**: M08 introduces `Value::Arc`, `Value::Mutex`, `Value::MutexGuard` (additive Value variants) AND a new `ThreadId` newtype (analog of `VtableAddr` from M07.7). The 7 thread/sync `MemEvent` variants are PRE-DECLARED from M03 — no new event variants needed. Pure-additive Value extensions + new ThreadId newtype.
- **Sized L per MILESTONES.md** — but the multi-column UI + parked-thread visual + dashed-purple arrows + refcount display together push toward XL. Plan-phase to confirm sizing; the MILESTONES note acknowledges this ("Borderline-XL by per-event counting; L by event-category counting; if XL during implementation, split into M08a + M08b").
- **Possible M08a/M08b split**: per MILESTONES.md's split contingency, if implementation reveals XL scope, the plan phase may split into M08a (threads + multi-column stacks) and M08b (Arc/Mutex/sync + dashed purple). Don't pre-commit to one or the other in the spec; defer to plan.
- **UX checkpoint expected**: multi-column stacks layout, parked-thread visual, refcount display, dashed-purple arrows are iterative UI pieces (analog of M07.4's struct view and M07.7's VTABLES panel). Plan stages a first cut, pauses for visual review before refining. Mirrors the M07.4/M07.7 workflow.
- **Stacks panel orientation**: per current CSS `flex-direction: column-reverse` inside `#stacks` — frames within a column go innermost-on-top. Multi-column extension keeps each column as-is but lays columns out left-to-right. Plan-phase confirms the outer container becomes a horizontal flex.
- **Bundle budget ≤ +25%**: comparable to M07.7's investment (also +5%) but with broader UI surface; the +25% headroom covers the multi-column rendering, parked-thread visualizations, and Arc/Mutex/Guard value variants.
- **Foundation for future levels**: after M08, Level 4 is complete. CLAUDE.md doesn't define Levels 5+; nothing is being deferred to a hypothetical M09 since no such scope claim exists.
