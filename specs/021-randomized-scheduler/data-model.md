# Data Model — Randomized scheduler: entities & state transitions

Three layers of state: scheduler, per-thread, event-stream.

## Scheduler state (Evaluator-side, Rust)

```rust
// New substruct inside Evaluator (sibling field).
struct Scheduler {
    /// Seeded PRNG. State advances on every pick().
    prng: Prng,
    /// Original seed (for Note emission and debug).
    seed: u32,
}

struct Prng {
    state: u64,
}

impl Scheduler {
    fn new(seed: u32) -> Self { ... }

    /// Pick uniformly at random from the Ready set. Caller passes the
    /// current Ready slice (sorted by ThreadId for determinism). Returns
    /// the chosen ThreadId. Panics if `ready` is empty.
    fn pick(&mut self, ready: &[ThreadId]) -> ThreadId { ... }
}
```

### Validation rules

- **VR-S1**: `pick(empty_slice)` is a logic error; caller must check before invoking.
- **VR-S2**: `pick(single_element)` returns that element WITHOUT advancing the PRNG state — preserves single-thread byte-identical traces (SC-002).
- **VR-S3**: the Ready slice MUST be sorted by `ThreadId` before passing to `pick` — guarantees determinism even if the underlying `IndexMap<ThreadId, ThreadState>` iteration order changes.

## Per-thread state extensions

```rust
enum ThreadStatus {
    Ready,                          // existing
    Done,                           // existing
    // **021**: new variants for cooperative parking
    BlockedOnLock(HeapAddr),        // waiting for a Mutex to be released
    BlockedOnJoin(ThreadId),        // waiting for a thread to finish
}
```

### Validation rules

- **VR-T1**: `BlockedOnLock(addr)` MUST reference a heap addr that points to a `HeapObject::Mutex` or `HeapObject::ArcMutex`. Other addrs are an internal bug.
- **VR-T2**: `BlockedOnJoin(tid)` MUST reference a `ThreadId` present in `evaluator.threads`. Joining a non-existent thread is a typeck-level error caught earlier.
- **VR-T3**: a thread's status MUST be flipped back to `Ready` (not other states) when its block condition is satisfied. The scheduler's next `pick` chooses among all-Ready as usual.

### State transitions

| From | Event | To |
|---|---|---|
| Ready | Scheduler picks; statement completes normally | Ready (continues) |
| Ready | Scheduler picks; statement is a let-binding RHS that's `mutex.lock()` where `holder.is_some()` and holder ≠ self | BlockedOnLock(addr) |
| Ready | Scheduler picks; statement is `h.join()` where target.status != Done | BlockedOnJoin(target) |
| Ready | Statement is the closure body's final stmt (no more stmts) | Done |
| BlockedOnLock(addr) | Another thread releases the lock at `addr` (drops MutexGuard) | Ready |
| BlockedOnJoin(target) | `target.status` flips to Done | Ready |
| Done | (terminal) | — |

## Event stream extensions

```rust
enum MemEvent {
    // ... existing variants ...
    /// **021**: emitted when the scheduler detects no Ready threads but
    /// at least one Blocked thread. The trace ends with this event.
    Deadlock {
        thread_ids: Vec<ThreadId>,    // all currently-blocked threads
        span: Span,                   // most-recent scheduling-point span
    },
}
```

### Validation rules

- **VR-E1**: `Deadlock` MUST be the last event in the trace if emitted. No further events follow.
- **VR-E2**: `thread_ids` MUST be non-empty (a deadlock requires at least one blocked thread).
- **VR-E3**: serialization tag matches the existing `serde` tag-by-key style: `{"Deadlock": {"thread_ids": [1, 2], "span": {...}}}`.

## Seed flow (host ↔ WASM ↔ UI)

```text
┌─────────────────────────────────────────────────┐
│  JS: <input id="seed-input" type="number">     │
│      <button id="btn-reroll-seed">🎲</button>   │
└─────────────────┬───────────────────────────────┘
                  │ (debounced 300ms / instant on click)
                  ▼
┌─────────────────────────────────────────────────┐
│  player.set_source(source: string, seed: u32)   │
└─────────────────┬───────────────────────────────┘
                  │ wasm-bindgen
                  ▼
┌─────────────────────────────────────────────────┐
│  Player::set_source(source: &str, seed: u32)    │
│      ↓                                          │
│  pipeline::run(source: &str, seed: u32)         │
│      ↓                                          │
│  Evaluator::new_with_seed(ast, types, seed: u32)│
│      ↓                                          │
│  Scheduler { prng, seed }                       │
│      ↓                                          │
│  Trace serialization includes:                  │
│      { "events": [...], "seed": 42 }            │
└─────────────────┬───────────────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────────────┐
│  JS reads state.seed and updates input.value    │
│  (single source of truth = the trace's seed)    │
└─────────────────────────────────────────────────┘
```

### Validation rules

- **VR-SF1**: the seed passed into `Player::set_source` MUST appear in the resulting trace's metadata (so the UI can display "the trace you're looking at was generated with seed X").
- **VR-SF2**: the seed input field's value MUST equal `lastSnapshot.seed` after every successful render. If the user types `42` but the WASM clamps it (it shouldn't — u32 accepts the full range), the displayed value reflects what was actually used.
- **VR-SF3**: re-roll's `Math.random()` is called ONCE per click; the resulting seed is set on the input AND passed to `set_source` in the same JS tick (no debounce window where user could re-roll twice and get one render).

## Public API contract

| Function | Signature | Notes |
|---|---|---|
| `Player::new` | `fn new(source: &str) -> Player` | Defaults seed to `0`. Backwards-compatible with M01-M08 tests. |
| `Player::set_source` | `fn set_source(&mut self, source: &str, seed: u32) -> String` | NEW signature (was `(source)`). Returns JSON with `{ok, state}` or `{ok: false, error}`. State includes `seed` field. |
| `Player::set_seed` | `fn set_seed(&mut self, seed: u32) -> String` | NEW. Re-runs pipeline with the current source and the new seed. Returns the same JSON shape as `set_source`. |

### Migration plan for existing call sites

- **`Player::new("")` in `index.js`**: unchanged. Defaults to seed=0.
- **`player.set_source(source)` in `index.js`**: updates to `player.set_source(source, currentSeed)`.
- **M01-M07.7 tests**: pass `seed=0` everywhere. No trace changes (single-thread).
- **M08 tests**: re-baseline once; new traces reflect the random scheduler.
