# Contract — M04 Public APIs

Two distinct contracts: the **wasm-bindgen `Player` API** (the WASM↔JS boundary) and the **trace JSON schema** (the file `gen_traces` writes and `Player::new` reads).

> **M05 amendment** (`specs/007-live-l1-editing/`): the `Player::new` signature is changed from `new(trace_json: &str) -> Result<Player, JsValue>` to `new(source: &str) -> Player` (infallible — takes Rust source, not pre-recorded trace JSON, and exists in error state when the source fails to compile). A new method `set_source(&mut self, source: &str) -> String` is added. This is a **breaking signature change** to a previously-shipped method, permitted by relaxing M04's "additive only" rule in the same spirit M03.1 relaxed M03's closed-`MemEvent` rule. Future revision milestones can change `Player` methods with maintainer consent + coordinated update of all JS consumers. See `specs/007-live-l1-editing/contracts/m05-api.md` for the full M05 contract delta.

## Player API (wasm-bindgen)

```rust
#[wasm_bindgen]
pub struct Player { /* opaque */ }

#[wasm_bindgen]
impl Player {
    /// Parse a trace JSON document and create a Player positioned at step 0.
    /// Returns an Err if the JSON is malformed.
    #[wasm_bindgen(constructor)]
    pub fn new(trace_json: &str) -> Result<Player, JsValue>;

    /// Current state snapshot, encoded as JSON. JS deserializes into `StateSnapshot`.
    pub fn state(&self) -> String;

    /// The Rust source code for this trace's sample (verbatim).
    pub fn source(&self) -> String;

    /// Advance the cursor by 1 event. No-op at end of trace. Returns the new state.
    pub fn step_forward(&mut self) -> String;

    /// Decrement the cursor by 1. No-op at position 0. Returns the new state.
    pub fn step_back(&mut self) -> String;

    /// Reset the cursor to position 0. Returns the new state.
    pub fn rewind(&mut self) -> String;

    /// Current cursor position (also embedded in `state()`).
    pub fn position(&self) -> usize;

    /// Total number of events in the trace (also embedded in `state()`).
    pub fn total(&self) -> usize;
}
```

### Behavioral guarantees

- **B-1**: `Player::new(json).is_ok()` iff `json` is a syntactically valid trace document (see schema below). Semantic validity (e.g. balanced FrameEnter/FrameLeave) is NOT enforced at parse time — invalid traces simply produce odd-looking state snapshots when stepped.
- **B-2**: `step_forward` past the last event is a no-op; the returned state is unchanged from the previous call.
- **B-3**: `step_back` from position 0 is a no-op.
- **B-4**: `rewind` is idempotent — calling it twice in a row produces identical state.
- **B-5**: `state()` is pure (no side effects); calling it N times in a row returns the same value.
- **B-6**: For any sequence of step / rewind operations, the resulting `StateSnapshot` depends only on the final cursor position — NOT on the path taken to reach it. (SC-003 determinism.)

## StateSnapshot JSON schema

`state()` returns JSON of this shape:

```json
{
  "frames": [
    {
      "frame_id": 0,
      "fn_name": "main",
      "slots": [
        { "slot_id": 0, "name": "x", "ty": "i32", "value": "5" },
        { "slot_id": 1, "name": "y", "ty": "i32", "value": null }
      ]
    }
  ],
  "editor_highlight": { "start": 16, "end": 30, "file": 1 },
  "status": null,
  "position": 4,
  "total": 13
}
```

Field semantics:

- `frames`: outermost first, innermost (current) last. JS renders bottom-up.
- `editor_highlight`: span of the event most recently applied (i.e. `events[position - 1].span`). `null` at position 0.
- `status`: present only when the most recent event was a `Note` — `{ "kind": "error" | "info", "message": "..." }`.
- `position` / `total`: redundant with `Player::position()` / `Player::total()`; included for atomic snapshot consistency.

## Trace JSON schema

`gen_traces` writes one file per sample to `web/traces/<sample_name>.json`:

```json
{
  "source": "<verbatim .rs source text>\n",
  "events": [ <MemEvent>, <MemEvent>, ... ]
}
```

Each `MemEvent` is serde's default externally-tagged representation. Examples:

```json
{ "FrameEnter": { "frame_id": 0, "fn_name": "main", "params": [], "span": { "start": 0, "end": 32, "file": 1 } } }
{ "SlotAlloc": { "slot_id": 0, "name": "x", "ty": "I32", "span": { "start": 16, "end": 30, "file": 1 } } }
{ "SlotWrite": { "slot_id": 0, "value": { "Int": 5 }, "span": { "start": 16, "end": 30, "file": 1 } } }
{ "SlotDrop": { "slot_id": 0, "span": { "start": 16, "end": 30, "file": 1 } } }
{ "FrameLeave": { "frame_id": 0, "return_value": "Unit", "span": { "start": 10, "end": 32, "file": 1 } } }
{ "Note": { "kind": "RuntimeError", "message": "division by zero", "span": { "start": 24, "end": 29, "file": 1 } } }
```

Schema rules:

- `source` is a UTF-8 string; JS uses it verbatim as the editor's initial content.
- `events` is an array; element order is the emission order (FR-009 from spec).
- A trailing newline in `source` is recommended for editor cleanliness but not required.
- `events` may be empty (the M03 evaluator returns `[]` for programs without `main`).

## Stability rules

- **Player API**: the methods `new`, `state`, `source`, `step_forward`, `step_back`, `rewind`, `position`, `total` are stable from M04 close. Additions are non-breaking; signature changes are breaking.
- **StateSnapshot JSON**: fields shown above are stable. M06–M08 will add fields (e.g. `arrows`, `heap`); JS should ignore unknown fields gracefully. Removing or renaming fields is breaking.
- **Trace JSON**: the outer `{ source, events }` shape is stable. The `MemEvent` variant tags are stable (closed enum from M03). Adding new fields to existing variant payloads in M06+ is additive; the JS UI ignores variant fields it doesn't know.

## What this contract does NOT cover (deferred)

- **Live editing** (the editor is read-only in M04). M05 will extend the Player API with a `set_source(&str)` method that re-runs the pipeline.
- **Heap rendering** (M07). The StateSnapshot will gain a `heap` field then.
- **Pointer arrows** (M06). The StateSnapshot will gain an `arrows` field.
- **Multi-thread stacks** (M08). The `frames` field's interpretation changes — currently a single thread's stack; M08 makes it multi-column.
- **Persistence** (cursor / selected sample across reloads). Not in M04.
- **Auto-play rate configurability** (research R-015). Not in M04.
- **Mobile / accessibility** (FR-011). Not in M04.
