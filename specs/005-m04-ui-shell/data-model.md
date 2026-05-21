# Data Model — M04 Entities

M04 introduces the `Cursor` + the `StateSnapshot` view family. All M01/M02/M03 types are reused with a single additive change: derived `Serialize` + `Deserialize` on the types that appear in `MemEvent`.

## Existing types: additive `serde` derives

The following types — already public from earlier milestones — gain `#[derive(serde::Serialize, serde::Deserialize)]` (additive per their stability contracts):

| Type        | Origin    | Why                                                          |
|-------------|-----------|--------------------------------------------------------------|
| `Span`      | M01       | Appears in every `MemEvent` field and in `StateSnapshot.editor_highlight`. |
| `FileId`    | M01       | Inside `Span`.                                               |
| `Ty`        | M02       | Inside `MemEvent::SlotAlloc.ty` and rendered in `SlotRowView`. |
| `MemEvent`  | M03       | Trace payload.                                               |
| `Value`     | M03       | Inside `MemEvent` and rendered in `SlotRowView`.             |
| `NoteKind`  | M03       | Inside `MemEvent::Note`.                                     |
| `Pointee`   | M03       | Inside `MemEvent` borrow variants.                           |
| `SlotId`    | M03       | Inside `MemEvent` slot variants.                             |
| `FrameId`   | M03       | Inside `MemEvent` frame variants.                            |
| `HeapAddr`  | M03       | Inside `MemEvent` heap variants (forward-compat).            |
| `BorrowId`  | M03       | Inside `MemEvent` borrow variants (forward-compat).          |

These changes are additive and don't affect Debug-snapshot tests for M01/M02/M03 (Debug output is independent of serde derives). M01–M03 snapshot tests pass unchanged.

## New entity: `Cursor`

```rust
pub struct Cursor {
    pub trace: Vec<MemEvent>,
    /// 0 ≤ position ≤ trace.len()
    pub position: usize,
}
```

State machine:

```text
position = 0     │ initial — no events applied
                 │
step_forward() ──┤  position = (position + 1).min(trace.len())
step_back()    ──┤  position = position.saturating_sub(1)
rewind()       ──┤  position = 0
                 │
position = trace.len() │ terminal (cannot advance past)
```

### Validation rules

- **VR-1**: `position` always satisfies `0 ≤ position ≤ trace.len()`.
- **VR-2**: `step_forward()` past the last event is a no-op (cursor stays at `trace.len()`).
- **VR-3**: `step_back()` from position 0 is a no-op.
- **VR-4**: `rewind()` always sets position to 0 regardless of current position.
- **VR-5**: If the last event is `MemEvent::Note { kind: NoteKind::RuntimeError, ... }`, advancing into it is allowed; advancing past it is not. The cursor's max meaningful position is `trace.len()`.

## New entity: `StateSnapshot`

The serialized "what the UI should show at the current cursor position".

```rust
pub struct StateSnapshot {
    /// Frame cards, outermost first (bottom of the visual stack).
    pub frames: Vec<FrameCardView>,
    /// Span the editor should highlight at this step (the event whose state we just applied).
    pub editor_highlight: Option<Span>,
    /// Status message (runtime error, info note).
    pub status: Option<StatusView>,
    /// Cursor position (for the toolbar `N / total` indicator).
    pub position: usize,
    /// Total events in the trace (for the toolbar).
    pub total: usize,
}
```

Derives `Serialize` + `Deserialize`.

## New entity: `FrameCardView`

```rust
pub struct FrameCardView {
    pub frame_id: u32,
    pub fn_name: String,
    /// Active slots, oldest first (declaration order).
    pub slots: Vec<SlotRowView>,
}
```

### Validation rules

- **VR-6**: `slots` is in declaration order. Dropped slots are removed from the vec (not retained as tombstones).
- **VR-7**: Each `frame_id` in a `StateSnapshot.frames` is unique within that snapshot.

## New entity: `SlotRowView`

```rust
pub struct SlotRowView {
    pub slot_id: u32,
    pub name: String,
    /// Type label (`"i32"`, `"bool"`, `"()"`).
    pub ty: String,
    /// `None` if allocated-but-not-written (placeholder `?` in UI); `Some` after the first `SlotWrite`.
    pub value: Option<String>,
}
```

### Validation rules

- **VR-8**: `value` is `None` for the span between `SlotAlloc` and the slot's first `SlotWrite`. After at least one `SlotWrite`, `value` is `Some(rendered string)`.
- **VR-9**: `value` strings are rendered with `Value`'s natural display: `5`, `true`, `()`.

## New entity: `StatusView`

```rust
pub struct StatusView {
    /// Category: `"error"` (RuntimeError Note) or `"info"` (Info Note).
    pub kind: String,
    /// The note's message.
    pub message: String,
}
```

`StatusView` is `Some` only when the most recently-applied event is a `Note` event. It doesn't persist across subsequent steps.

## New entity: `Player` (wasm-bindgen)

```rust
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct Player {
    cursor: Cursor,
    source: String,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl Player {
    #[wasm_bindgen(constructor)]
    pub fn new(trace_json: &str) -> Result<Player, JsValue>;
    pub fn state(&self) -> String;       // JSON of StateSnapshot
    pub fn source(&self) -> String;       // the .rs source for the editor
    pub fn step_forward(&mut self) -> String;
    pub fn step_back(&mut self) -> String;
    pub fn rewind(&mut self) -> String;
    pub fn position(&self) -> usize;
    pub fn total(&self) -> usize;
}
```

`Player` is the entire JS-WASM boundary. JS instantiates a Player with the trace JSON, then calls step/rewind methods and reads `state()` to drive the DOM.

### Validation rules

- **VR-10**: `Player::new` returns an `Err(JsValue)` if the JSON is malformed. The string error message is suitable for surfacing to the user.
- **VR-11**: `state()`, `step_forward()`, `step_back()`, `rewind()` all return a JSON string of the current `StateSnapshot`. JS deserializes each.
- **VR-12**: `position()` and `total()` return the same numbers embedded in the snapshot (provided for convenience).

## Trace JSON schema

The trace file format that `gen_traces` writes and `Player::new` parses.

```json
{
  "source": "fn main() { let x = 2 + 3; }\n",
  "events": [
    { "FrameEnter": { "frame_id": 0, "fn_name": "main", "params": [], "span": { "start": 0, "end": 32, "file": 1 } } },
    { "SlotAlloc": { "slot_id": 0, "name": "x", "ty": "I32", "span": { ... } } },
    ...
  ]
}
```

Serde's default enum representation (externally tagged) puts each `MemEvent` variant as a JSON object with one key. This is verbose but unambiguous and decodes back into `MemEvent` cleanly via `Deserialize`.

## Relationships

```
trace JSON ──► Player::new (parse) ──► Cursor { trace, position }
                                          │
JS event (step) ──► Player::step_forward ──► Cursor advances
                                          │
                                          ▼
                                   StateSnapshot ──► JSON ──► JS DOM render
```

## State transitions in `Cursor`

```
position 0 ──step──► 1 ──step──► 2 ──step──► … ──step──► trace.len()
   ▲          │         │                            │
   │          │         │                            │
   │          ◄── step_back ────────────────────────┘
   │
   └────────── rewind ───────────────────────────────
```
