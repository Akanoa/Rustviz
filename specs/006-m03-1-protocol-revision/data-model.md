# Data Model — M03.1 Entities

M03.1 doesn't define new top-level entities — it modifies three existing ones. This file enumerates the diffs.

## Modified: `Ty` — adds `is_copy()` method

```rust
// In src/typeck.rs

impl Ty {
    /// Returns `true` if values of this type are `Copy` (no destructor; bytes
    /// physically persist on the stack until the storage is reused).
    ///
    /// L1's lattice (`I32`, `Bool`, `Unit`) is entirely Copy. M07+ will add
    /// non-Copy heap-allocated variants (`Box`, `Vec`, `String`) that return
    /// `false`; the exhaustive `match` below ensures any new variant forces
    /// a deliberate classification.
    pub fn is_copy(self) -> bool {
        match self {
            Self::I32 | Self::Bool | Self::Unit => true,
        }
    }
}
```

### Validation rules

- **VR-1**: `is_copy()` is exhaustive over `Ty`. Adding a new variant without updating `is_copy()` is a compile error (Rust's exhaustiveness check).
- **VR-2**: M01/M02 tests don't exercise `is_copy()` directly. It's used only by M03's evaluator.

## Modified: `MemEvent` — adds `ReturnValue` variant, drops `FrameEnter.params`

```rust
// In src/event.rs

pub enum MemEvent {
    // ── Frames ────────────────────────────────────────────────────────────
    FrameEnter {
        frame_id: FrameId,
        fn_name: String,
        // FIELD REMOVED: was `params: Vec<(SlotId, String, Value)>` in M03.
        // The same info is now solely conveyed by the per-param
        // SlotAlloc + SlotWrite events that follow this FrameEnter.
        span: Span,
    },
    FrameLeave {
        frame_id: FrameId,
        return_value: Value,
        span: Span,
    },
    /// NEW in M03.1. Emitted between body completion and `FrameLeave` so the
    /// function's return value is visible for one cursor step before the frame
    /// closes. Carries the same `Value` as the matching `FrameLeave.return_value`.
    ReturnValue {
        frame_id: FrameId,
        value: Value,
        span: Span,
    },

    // ── Stack slots (unchanged shape, gated emission) ─────────────────────
    SlotAlloc { /* unchanged */ },
    SlotWrite { /* unchanged */ },
    SlotMove  { /* unchanged */ },
    SlotDrop  { /* unchanged */ },

    // ── All other variants ─ unchanged ────────────────────────────────────
    // ThreadSpawn, ThreadJoin, ThreadPark,
    // HeapAlloc, HeapRealloc, HeapFree,
    // BorrowShared, BorrowMut, BorrowEnd,
    // LockAcquire, LockRelease, ArcClone, ArcDrop,
    // Note { kind, message, span }.
}
```

### Validation rules

- **VR-3**: `MemEvent::ReturnValue.value` matches the immediately-following `MemEvent::FrameLeave.return_value` for the same `frame_id`. This is an invariant in successful evaluations (programs that don't halt).
- **VR-4**: `ReturnValue` is **not** emitted for frames that halt on a `Note { kind: NoteKind::RuntimeError, ... }`. The trace ends at the Note; no ReturnValue or FrameLeave for the halted frame.
- **VR-5**: `ReturnValue.span` is the body's tail expression span when present; otherwise the body block's span.
- **VR-6**: `SlotDrop` is emitted only when the binding's `Ty::is_copy()` returns `false`. For L1 traces this means **zero** `SlotDrop` events.
- **VR-7**: `FrameEnter` no longer carries the `params` field. Trace JSONs from before M03.1 are no longer valid input (would fail JSON deserialization due to the missing field having been a non-optional structural element).

## Modified: `StateSnapshot` — adds `pending_return`

```rust
// In src/ui.rs

pub struct StateSnapshot {
    pub frames: Vec<FrameCardView>,
    pub editor_highlight: Option<Span>,
    pub status: Option<StatusView>,
    /// NEW in M03.1. `Some` when the most recently applied event is a
    /// `MemEvent::ReturnValue`. `None` otherwise. The JS renderer
    /// decorates the matching frame card with a transient return-value
    /// annotation.
    pub pending_return: Option<PendingReturnView>,
    pub position: usize,
    pub total: usize,
}

/// NEW in M03.1.
pub struct PendingReturnView {
    /// The frame that's about to return.
    pub frame_id: u32,
    /// Rendered return value (e.g. `"5"`, `"true"`, `"()"`).
    pub value: String,
}
```

### Validation rules

- **VR-8**: `pending_return` is `Some` iff the last applied event is `MemEvent::ReturnValue`. Any other last event → `None`. Specifically: stepping past the ReturnValue to the next event (FrameLeave) sets `pending_return` back to `None`.
- **VR-9**: `pending_return.frame_id` matches `MemEvent::ReturnValue.frame_id`.
- **VR-10**: `pending_return.value` uses the same `Value → String` rendering as `SlotRowView.value` (e.g. `Value::Int(5) → "5"`, `Value::Bool(true) → "true"`, `Value::Unit → "()"`).

## Cursor / Player API

No signature changes. `Cursor::state_snapshot` returns the extended `StateSnapshot`. `Player::state()` JSON gains the `pending_return` field. JS consumers that read the JSON should handle the new field gracefully (per the additive-fields stability rule).

## Re-baselined artifacts

| Path                                           | Change                                          |
|------------------------------------------------|-------------------------------------------------|
| `tests/snapshots/emits_arithmetic.snap`        | regenerated (no SlotDrop for x; +ReturnValue main) |
| `tests/snapshots/emits_fn_call.snap`           | regenerated (−3 SlotDrops; +2 ReturnValues) |
| `tests/snapshots/emits_if_then.snap`           | regenerated |
| `tests/snapshots/emits_if_else.snap`           | regenerated |
| `tests/snapshots/emits_shadow.snap`            | regenerated |
| `tests/snapshots/emits_nested_block.snap`      | regenerated |
| `tests/snapshots/emits_short_circuit.snap`     | regenerated |
| `tests/snapshots/emits_div_by_zero_note.snap`  | re-checked, unchanged (halts before drops/returns) |
| `web/traces/m03_*.json`                        | regenerated by `gen_traces` |

M01 and M02 snapshots untouched.
