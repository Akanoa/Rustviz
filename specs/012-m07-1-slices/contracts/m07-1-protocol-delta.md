# Contract — M07.1 Protocol Delta

5th invocation of the closed-enum-with-revisions rule. M07.1 adds variants only — no existing variant changes shape.

## Closed-enum rule — fifth invocation

| Milestone | Change |
|---|---|
| M03.1 | Added `MemEvent::ReturnValue`; removed `FrameEnter.params` (additive + redundant-field removal). |
| M03.2 | Restructured `Ty` and `Value` (kind-based). Rule generalized to all protocol types. |
| M06 | Added `Ty::Ref`, `Value::Ref`. Filled `MemEvent::BorrowShared/BorrowMut/BorrowEnd` payloads. |
| M07 | Added `Ty::Box/Vec/String`, `Value::Box/Vec/String/Str`. **Restructured `Value::Ref`** (target_slot → target: Pointee). Filled `MemEvent::HeapAlloc/HeapRealloc/HeapFree` payloads. |
| **M07.1** | Adds `Ty::Slice(Box<Ty>)` and `Value::Slice { borrow_id, target, len, mutable }`. **Pure additive — no restructure**. No event-variant changes. |

The pure-additive nature means M07.1 introduces zero risk to existing snapshot tests — every M01/M02/M03 snapshot stays byte-identical because no sample constructs the new variants.

## `Ty` — additive variant

```rust
pub enum Ty {
    Int(IntKind),
    Float(FloatKind),
    Bool,
    Unit,
    Ref { inner: Box<Ty>, mutable: bool },
    Box(Box<Ty>),
    Vec(Box<Ty>),
    String,
    // NEW in M07.1:
    Slice(Box<Ty>),
}
```

JSON shape gains one new tag: `{ "Slice": <Ty> }`.

## `Value` — additive variant

```rust
pub enum Value {
    Int { kind, bits },
    Float { kind, value },
    Bool(bool),
    Unit,
    Ref { borrow_id, target, mutable },
    Box { addr },
    Vec { addr },
    String { addr },
    Str(String),
    // NEW in M07.1:
    Slice {
        borrow_id: BorrowId,
        target: Pointee,
        len: u64,
        mutable: bool,
    },
}
```

JSON shape: `{ "Slice": { "borrow_id": ..., "target": <Pointee>, "len": ..., "mutable": bool } }`.

The `target` field is the existing `Pointee` enum: `{ "Slot": <SlotId> }` or `{ "Heap": <HeapAddr> }`. In M07.1, slice values always have `Pointee::Heap(_)` targets (no array-on-stack).

## `MemEvent` — no changes

M07.1 reuses existing `BorrowShared` and `BorrowEnd` events for slices. The borrow's `target` is `Pointee::Heap(addr)` (M07 already started producing these). No new event variants.

### Emission semantics

- **`BorrowShared { borrow_id, target: Pointee::Heap(addr), span }`**: emitted when a range-indexed borrow `&v[range]` evaluates. The receiver value (`Value::Slice { borrow_id, target, len, .. }`) is then written to the receiving slot via the normal `SlotWrite` path.
- **`BorrowEnd { borrow_id, span }`**: emitted at slice scope exit, same as for regular `Value::Ref` borrows.
- **Dangling-slice detection**: M07's existing scan in `realloc_heap` enumerates `world.borrows` and matches on `target: Pointee::Heap(from)`. Slices register with the same machinery → the scan catches them automatically. A `Note { RuntimeError, message: "dangling reference: slice ...", span: <original slice's span> }` fires.

## `ArrowView` — extended with `len: Option<u64>`

```rust
// OLD (M07):
pub struct ArrowView {
    pub source_slot: u32,
    pub target: ArrowTarget,
    pub kind: ArrowKind,
}

// NEW (M07.1):
pub struct ArrowView {
    pub source_slot: u32,
    pub target: ArrowTarget,
    pub kind: ArrowKind,
    /// **M07.1**: optional length annotation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub len: Option<u64>,
}
```

JSON consumers in `web/index.js`:
- `arrow.len` is either omitted (non-slice arrows) or a number (slice arrows). Check `arrow.len !== undefined`.
- The renderer adds a `<text class="arrow-len-label">[len: N]</text>` element on the SVG overlay when `len` is present, positioned at the arrow's mid-point with a small perpendicular offset.

**Backwards-compatibility note**: `#[serde(default, skip_serializing_if = "Option::is_none")]` ensures existing non-slice ArrowView JSON output is unchanged. Older JS consumers that don't handle `len` will simply ignore the field — but M07.1's JS update reads it.

## `Pointee` — usage expansion (no shape change)

```rust
pub enum Pointee {
    Slot(SlotId),
    Heap(HeapAddr),
}
```

M07 started producing `Pointee::Heap(_)` for `&v[0]`-style borrows. M07.1 extends usage to slice borrows; the Pointee enum itself is unchanged.

## Behavioral guarantees (post-M07.1)

- **B-M71-1**: Every range-indexed borrow `&v[range]` evaluation emits exactly one `BorrowShared` event with `target: Pointee::Heap(addr)`.
- **B-M71-2**: Every slice borrow registers in `world.borrows` and is reachable by the dangling-detection scan.
- **B-M71-3**: A slice borrow active at HeapRealloc time produces a `Note { RuntimeError }` (same code path as M07's single-element dangling detection).
- **B-M71-4**: `Slice::len()` on a slice returns `Value::Int { kind: U64, bits: slice.len as i128 }`.
- **B-M71-5**: Out-of-bounds range (`start > end`, `end > vec.len`, `start > vec.len`, or negative bounds) at indexing time produces a `Note { RuntimeError }` and halts the trace. No `BorrowShared` event fires.
- **B-M71-6**: `Value::Slice.target` is always `Pointee::Heap(_)` in M07.1.
- **B-M71-7**: `Value::Slice.mutable` is always `false` in M07.1 (mutable slices typeck-rejected).
- **B-M71-8**: `ArrowView.len` is `Some(n)` iff the source slot holds a `Value::Slice { len: n, .. }`.
- **B-M71-9**: `BorrowShared` event for a slice carries the same shape as for a `&v[0]` borrow — no `len` in the event payload. Length is in the receiving `Value::Slice`.
- **B-M71-10**: Standalone `Expr::Range` (outside `Expr::Index.index`) produces a typeck error, not a parse error.

## What this contract does NOT cover (deferred)

- **Mutable slices** `&mut [T]` with element mutation — typeck-rejected in M07.1. M07.x or later.
- **Iterator methods** on slices (`s.iter()`, `for x in s`) — out of scope.
- **Slice methods beyond `len()`** (`first()`, `last()`, `is_empty()`, `contains()`, etc.) — out of scope.
- **Slicing a slice** (`let t = &s[0..1];`) — typeck-rejected (only Vec receivers in M07.1; slice-receiver case deferred).
- **Standalone range expressions** (`let r = 1..3;`, `for i in 1..10`) — typeck-rejected.
- **Range bounds with non-Int types** — typeck-rejected at the bound's type-check site.
- **Slice's byte-offset within the Vec's allocation** — not tracked (slice's `target` is the whole Vec addr; only `len` is carried). Future improvement could add `start_offset` if pedagogy demands it.
- **`&str` and static-memory slices** — M07.2 builds on this milestone's slice infrastructure.
- **Array types `[T; N]` and references to arrays `&[T; N]`** — out of scope.
- **Multi-dimensional slices** — out of scope.
