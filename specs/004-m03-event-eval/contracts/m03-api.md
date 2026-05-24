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

- The `MemEvent`, `Ty`, and `Value` types' shapes are stable from M03 onward. **Revision milestones** (e.g. `M03.1`, `M03.2`, future `M0X.N` patterns) may:
  - Add **new variants** to closed enums with explicit maintainer consent + coordinated update of all consumers (M04+).
  - **Remove redundant fields** from existing variants when the same information is reachable via other events in the stream.
  - **Restructure variants' internal field layout** with maintainer consent (e.g. `Value::Int(i64)` → `Value::Int { kind: IntKind, bits: i128 }` in M03.2 to support multiple integer widths).
  Removing or renaming top-level variants remains a breaking change requiring full re-coordination. Modifying payload field semantics (without restructure) is breaking.
- **M03.1 was the first invocation of this revised rule** (see `specs/006-m03-1-protocol-revision/contracts/m03-1-protocol-delta.md`): adds `MemEvent::ReturnValue`, removes the redundant `FrameEnter.params` field.
- **M03.2 extends the rule to `Ty` + `Value`** (see `specs/008-m03-2-scalar-lattice/contracts/m03-2-protocol-delta.md`): restructures `Ty` to `Int(IntKind) / Float(FloatKind) / Bool / Unit`, restructures `Value` to a unified `Int { kind, bits }` + `Float { kind, value }` form, and introduces `IntKind` (12 variants) + `FloatKind` (2 variants).
- **M06 adds a `Ref` variant to `Ty` and `Value`** (see `specs/009-m06-borrows/contracts/m06-protocol-delta.md`): pure additive growth, no restructure. Third invocation of the closed-enum-with-revisions rule. `Ty::Ref { inner: Box<Ty>, mutable }` drops `Copy` on `Ty` as a cascade consequence. The `MemEvent::BorrowShared`/`BorrowMut`/`BorrowEnd` variants (declared with their payload shapes in M03) start being emitted by the evaluator in M06.
- **M07 adds `Box/Vec/String` variants to `Ty` and `Value`** AND **restructures `Value::Ref`** (see `specs/011-m07-heap/contracts/m07-protocol-delta.md`): fourth invocation of the closed-enum-with-revisions rule. `Value::Ref { target_slot: SlotId, .. }` becomes `Value::Ref { target: Pointee, .. }` to support heap borrow targets (`Pointee::Heap(addr)` in addition to `Pointee::Slot(id)`). Second restructure ever after M03.2's `Value::Int`. The `MemEvent::HeapAlloc`/`HeapRealloc`/`HeapFree` variants (declared with their payload shapes in M03) start being emitted by the evaluator in M07.
- **M07.1 adds `Slice` variants to `Ty` and `Value`** (see `specs/012-m07-1-slices/contracts/m07-1-protocol-delta.md`): fifth invocation of the closed-enum-with-revisions rule. **Pure additive — no restructure**. `Ty::Slice(Box<Ty>)` represents `&[T]` (the leading `&` is absorbed into the slice type, matching Rust's "[T] only ever appears behind a reference"). `Value::Slice { borrow_id, target: Pointee, len: u64, mutable }` is a fat-pointer borrow with length metadata, sibling to `Value::Ref` rather than an extension. Slice borrows reuse the existing `MemEvent::BorrowShared`/`BorrowEnd` events with `Pointee::Heap(addr)` targets — no new event variants. `ArrowView` gains an optional `len: Option<u64>` field for the visual length annotation on slice arrows.
- **M07.2 adds the static memory region** (see `specs/013-m07-2-str-static/contracts/m07-2-protocol-delta.md`): sixth invocation of the closed-enum-with-revisions rule. Additions: `StaticAddr(u32)` newtype, `Pointee::Static(StaticAddr)` variant, `Ty::Str` sugar (semantically equivalent to `Ty::Slice(Box::new(Ty::Int(IntKind::U8)))`; rendered as `"&str"`), `MemEvent::StaticAlloc { addr, bytes, span }` (fires once per unique literal content — content-deduplicated to match Rust linker behavior), `ArrowTarget::Static(u32)`. **Removal**: `Value::Str(String)` — M07's transient is now dead since string literals become `Value::Slice` targeting the static region. Second variant removal in the project after M03.1's `FrameEnter.params`; both are dead-code cleanups (no shipped sample preserves `Value::Str` in any persisted artifact).
- **M07.3 adds arrays** (see `specs/014-m07-3-arrays/contracts/m07-3-protocol-delta.md`): seventh invocation of the closed-enum-with-revisions rule. **Pure additive — no restructure, no event-variant changes, no Pointee changes**. `Ty::Array(Box<Ty>, u64)` represents `[T; N]` (stack-allocated, fixed size known at compile time). `Value::Array { elements: Vec<Value>, elem_ty: Ty }` holds the array's content inline in the slot's value. First scenario constructing `Value::Slice { target: Pointee::Slot(_), .. }` — the `Pointee::Slot` variant was declared in M03 and used by M06 for `&x`-style borrows, but M07.3 is the first to use it as a slice target (slicing `&t[range]` on an array). Closes the slice trilogy: Slot (M07.3) + Heap (M07/M07.1) + Static (M07.2) all carry the same slice abstraction.
- Behavioral changes (different event emission order for the same input) are breaking and require coordination with M04.
