# Contract — M03 Public API

The surface M04 (and any later consumer) relies on once M03 closes.

## Entry point

```rust
pub fn rustviz::evaluate(
    program: &ast::Program,
    resolution: &Resolution,
    types: &TypeMap,
) -> Result<Vec<MemEvent>, ParseError>;
```

- **Input**: the three outputs of the M01 → M02 pipeline.
- **Output**: on success, a `Vec<MemEvent>` in emission order (depth-first, left-to-right walk). On static failure (which should be unreachable when M02 succeeded), returns `Err(ParseError)`. On *runtime* failure (overflow, div-by-zero, recursion depth), returns `Ok(events_so_far)` where the last event is `MemEvent::Note { kind: NoteKind::RuntimeError, ... }`.
- **Determinism**: same input → byte-identical `Vec<MemEvent>` across runs (SC-005).

## Re-exports from `lib.rs`

```rust
pub use parse::{parse, ast};
pub use parse::error::ParseError;
pub use parse::span::{FileId, SourceMap, Span};
pub use resolve::{resolve, BindingDecl, BindingId, BindingKind, Resolution};
pub use typeck::{typeck, BindingType, FnSig, Ty, TypeMap};
pub use eval::evaluate;
pub use event::{
    BorrowId, FrameId, HeapAddr, MemEvent, NoteKind, Pointee, SlotId, Value,
};
pub mod event;  // exposed for `rustviz::event::MemEvent` pattern matching
```

## Stable types (from M03 close)

| Type           | Stability  | Notes                                                                       |
|----------------|------------|-----------------------------------------------------------------------------|
| `SlotId`       | stable     | Newtype `u32`. Allocated by evaluator.                                       |
| `FrameId`      | stable     | Newtype `u32`.                                                               |
| `HeapAddr`     | stable     | Newtype `u32`. Forward-compat for M07.                                       |
| `BorrowId`     | stable     | Newtype `u32`. Forward-compat for M06.                                       |
| `Pointee`      | stable for M03 variants | Additive; M07 may extend.                                          |
| `Value`        | stable for M03 variants | M07 will add heap-allocated variants (additive).                   |
| `NoteKind`     | stable for M03 variants | Later milestones may add variants (additive).                      |
| `MemEvent`     | **closed enum** | All variants present at M03. M06–M08 fill in payloads, not new variants. (Exception: if a category turns out to need a new variant, that's a breaking change requiring coordinated update.) |

## Behavioral guarantees

- **B-1**: `evaluate(...).is_ok()` implies the returned `Vec<MemEvent>` ends with a `FrameLeave` for the outermost call (typically `main`), or with a `Note { kind: RuntimeError, ... }` if a runtime error stopped evaluation.
- **B-2**: Every event carries a non-zero `Span` (SC-002).
- **B-3**: `FrameEnter` / `FrameLeave` pair in LIFO order.
- **B-4**: `SlotAlloc` / `SlotDrop` for any given `slot_id` pair in LIFO order within the slot's frame.
- **B-5**: Only the taken branch of an `if` emits events. Short-circuit `&&` / `||` skip the RHS when the LHS determines the result.
- **B-6**: `SlotMove` is never emitted from a pure L1 evaluation. The variant exists for M07+.
- **B-7**: Runtime errors emit a final `Note { kind: RuntimeError }` and stop the stream.

## Errors

`ParseError { message: String, span: Span }` — same shape as M01/M02. M03 doesn't normally produce one; it's a safety net for invariant violations from M02. Runtime errors are stream events.

## What this contract does NOT cover (deferred)

- **Borrow tracking** — M06. The variants exist (`BorrowShared` etc.); M06 will fill payload semantics.
- **Heap allocation events** — M07. Variants exist; payloads frozen for now (HeapAddr → u32).
- **Thread + sync events** — M08.
- **Multiple runtime errors** — M03 stops at the first. If a future milestone wants continuation, that's a contract change.
- **Streaming / async evaluation** — not promised. M03 returns a `Vec`; if very large traces become an issue (unlikely for pedagogical examples), revisit.
- **Reverse playback** — the cursor's "rewind" feature lives in M04. M03 just produces events; it doesn't track reversibility.

## Stability rules

- The `MemEvent` enum's variant set is stable from M03 onward. **Revision milestones** (e.g. `M03.1`, future `M0X.N` patterns) may:
  - Add **new variants** with explicit maintainer consent + coordinated update of all consumers (M04+).
  - **Remove redundant fields** from existing variants when the same information is reachable via other events in the stream.
  Removing or renaming existing variants remains a breaking change requiring full re-coordination. Modifying payload field semantics (without removal) is breaking.
- **M03.1 is the first invocation of this revised rule** (see `specs/006-m03-1-protocol-revision/contracts/m03-1-protocol-delta.md`): it adds `MemEvent::ReturnValue` and removes the redundant `FrameEnter.params` field.
- Payload value-types that ARE marked "stable for M03 variants" (e.g. `Value`, `NoteKind`) can grow additively.
- Behavioral changes (different event emission order for the same input) are breaking and require coordination with M04.
