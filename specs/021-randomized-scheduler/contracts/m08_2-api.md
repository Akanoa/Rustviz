# Contract — M08.2 trace + Player API

## Player API (Rust ↔ WASM ↔ JS)

### `Player::new(source: &str) -> Player`

Unchanged. Constructs a Player with the given source and `seed = 0`. Used by `index.js`'s initial Player creation and by all M01-M07.7 tests.

### `Player::set_source(source: &str, seed: u32) -> String`

**SIGNATURE CHANGE** from M08 v1 (`(source: &str)` → `(source: &str, seed: u32)`).

Compile the source with the given seed, run the pipeline, replace the player's trace.

Returns JSON of shape:

```json
{
  "ok": true,
  "state": {
    "seed": 42,
    "position": 0,
    "total": 31,
    "frames": [...],
    "threads": [...],
    "heap": [...],
    ...
  }
}
```

Or on parse / typeck / resolver error:

```json
{
  "ok": false,
  "error": {
    "stage": "parse",
    "message": "...",
    "span": { "start": 42, "end": 47 }
  }
}
```

### `Player::set_seed(seed: u32) -> String`

**NEW** in M08.2. Re-runs the pipeline using the player's CURRENT source and the new seed. Equivalent to calling `set_source(current_source, seed)` but doesn't require the caller to know the source.

Returns the same JSON shape as `set_source`.

## Trace shape — additions

### `state.seed: u32`

The seed used to generate the current trace. Always present.

### `MemEvent::Deadlock`

Emitted at most ONCE per trace, at the end, when the scheduler detects all threads are Blocked.

Serialized form (serde tag-by-key):

```json
{
  "Deadlock": {
    "thread_ids": [1, 2],
    "span": { "start": 142, "end": 156 }
  }
}
```

| Field | Type | Meaning |
|---|---|---|
| `thread_ids` | `[u32]` | All currently-blocked threads. Non-empty (≥ 1). |
| `span` | `Span` | Span of the most-recent scheduling-point. |

## Behavioral guarantees

- **B-M082-1**: For the same `(source, seed)` pair, `set_source` produces a bytewise-identical trace on every call within the same Player lifetime and across Player re-instantiations. (FR-002, SC-003.)
- **B-M082-2**: For a single-threaded program (no `thread::spawn`), `set_source(source, X)` produces the same trace for every `X ∈ [0, 2^32-1]`. (FR-008, SC-002.)
- **B-M082-3**: For an M08 multi-threaded program with at least one scheduling decision point, `set_source(source, X)` and `set_source(source, Y)` produce DIFFERENT traces for at least 80% of `(X, Y)` pairs sampled across `{0..100}`. (FR-003, SC-001.)
- **B-M082-4**: If the scheduler reaches a state where Ready is empty and at least one thread is `BlockedOn*`, the trace ends with exactly one `MemEvent::Deadlock` event and `state.position` is allowed to be < `state.total` (the player stops at the deadlock step). (FR-011, SC-007.)
- **B-M082-5**: The PRNG state advance is NOT observable to the user via the trace shape — only the resulting thread-selection decisions are. (Implementation detail; PRNG is internal to `Scheduler`.)
- **B-M082-6**: A program with a `mutex.lock()` call that's NOT the entire RHS of a let-binding panics with a clear runtime error: `M08.2 lock pattern restriction: wrap m.lock() in a let-binding before using the guard`. The panic surfaces in `state.error` rather than crashing the WASM module. (R-003 constraint.)

## Forward compatibility

- A future milestone removing the "lock must be RHS of let-binding" restriction will be additive: existing samples continue to work; the restriction error stops firing for the broader pattern set.
- A future milestone adding sub-statement scheduling will be additive: existing single-stmt-step traces stay valid; sub-stmt steps appear as extra cursor positions.
- A future v2 with full memory-ordering modeling (`Relaxed`/`Acquire`/`Release`) would extend the contract significantly — the trace shape would need per-thread "view shadows." Out of scope for 021.

## Out of scope

- **Cross-tab seed coordination**: each Player is independent. Two tabs with the same source+seed get the same trace.
- **Seed URL encoding**: not in v1. Could be added later via `?seed=` query param.
