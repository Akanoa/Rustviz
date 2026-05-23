# Contract — M07 Protocol Delta

4th invocation of the closed-enum-with-revisions rule. M07 adds variants AND restructures `Value::Ref` to support heap borrow targets.

## Closed-enum rule — fourth invocation

| Milestone | Change |
|---|---|
| M03.1 | Added `MemEvent::ReturnValue`; removed `FrameEnter.params` (additive + redundant-field removal). |
| M03.2 | Restructured `Ty` and `Value` (kind-based). Rule generalized to all protocol types. |
| M06 | Added `Ty::Ref`, `Value::Ref`. Filled `MemEvent::BorrowShared/BorrowMut/BorrowEnd` payloads. |
| **M07** | Adds `Ty::Box/Vec/String`, `Value::Box/Vec/String/Str`. **Restructures `Value::Ref`** (`target_slot: SlotId` → `target: Pointee`). Fills `MemEvent::HeapAlloc/HeapRealloc/HeapFree` payloads. |

The restructure of `Value::Ref` is the second time a protocol type's existing variant has changed its field layout (after M03.2's `Value::Int(i64)` → `Value::Int { kind, bits }`). With maintainer consent per the rule.

## `Ty` — additive variants

```rust
pub enum Ty {
    Int(IntKind),
    Float(FloatKind),
    Bool,
    Unit,
    Ref { inner: Box<Ty>, mutable: bool },
    // NEW in M07:
    Box(Box<Ty>),
    Vec(Box<Ty>),
    String,
}
```

JSON shape gains three new tags: `{ "Box": <Ty> }`, `{ "Vec": <Ty> }`, `{ "String": null }`.

## `Value` — additive variants + Ref restructure

```rust
pub enum Value {
    Int { kind, bits },
    Float { kind, value },
    Bool(bool),
    Unit,
    // RESTRUCTURED in M07: Ref's target field is now Pointee, not SlotId.
    Ref {
        borrow_id: BorrowId,
        target: Pointee,  // was: target_slot: SlotId
        mutable: bool,
    },
    // NEW in M07:
    Box { addr: HeapAddr },
    Vec { addr: HeapAddr },
    String { addr: HeapAddr },
    Str(String),  // transient — never stored in a slot
}
```

JSON wire format changes:
- `Value::Ref` JSON shape: `{ "Ref": { "borrow_id": ..., "target": <Pointee>, "mutable": bool } }`. The `target` is now an object like `{ "Slot": <SlotId> }` or `{ "Heap": <HeapAddr> }` (the existing M03 Pointee shape).
- `Value::Box/Vec/String/Str` JSON: standard variant tags.

## `MemEvent` — heap variant payloads filled

The three variants existed in M03 with their payload shapes already defined (`addr`, `size`, `ty_name`, `from`/`to`/`new_size`). M07 starts emitting them.

### Emission semantics

- **`HeapAlloc { addr, size, ty_name, span }`**: emitted by `Box::new(...)` and `String::from("...")`. NOT emitted by `Vec::new()` (empty Vec doesn't allocate). The FIRST `v.push(x)` on a fresh Vec emits `HeapAlloc` (the capacity grows from 0 to 1).
- **`HeapRealloc { from, to, new_size, span }`**: emitted when Vec or String capacity grows. The `from` addr is invalidated; the `to` addr is fresh. Active borrows pointing at `from` become dangling — a `Note { RuntimeError }` event fires for each, IMMEDIATELY after the HeapRealloc.
- **`HeapFree { addr, span }`**: emitted at scope exit for each Box/Vec/String binding in the scope's locals. Order: HeapFree first (deallocate); SlotDrop second (slot bytes invalidated). Mirrors M06's BorrowEnd-before-SlotDrop ordering.

### Vec growth policy

Doubling. Capacity goes 0 → 1 → 2 → 4 → 8 → 16 → ... Realloc fires on pushes that cross a capacity boundary.

### String growth policy

Same as Vec. `String::from("hi")` allocates with capacity = 2 (the source string's length). `push_str("!!!")` would require capacity 5; realloc to next power of 2 (8).

## `Pointee` — usage expansion (no shape change)

```rust
pub enum Pointee {
    Slot(SlotId),
    Heap(HeapAddr),
}
```

M06 only produced `Pointee::Slot(_)` from borrow events. **M07 starts producing `Pointee::Heap(_)`** when `&v[0]` (or similar) borrows into a heap allocation. The Pointee enum itself is unchanged from M03.

## `ArrowView` — replaces `BorrowView` (renamed; restructured)

```rust
// OLD (M06):
pub struct BorrowView {
    pub source_slot: u32,
    pub target_slot: u32,
    pub mutable: bool,
}

// NEW (M07):
pub struct ArrowView {
    pub source_slot: u32,
    pub target: ArrowTarget,  // Slot(u32) | Heap(u32)
    pub kind: ArrowKind,      // Shared | Mut | Owning
}
```

JSON consumers in `web/index.js`:
- `state.borrows` → `state.arrows`
- `arrow.target_slot` → `arrow.target` (object with `Slot` or `Heap` key)
- `arrow.mutable` → `arrow.kind` ("Shared" | "Mut" | "Owning")

## Behavioral guarantees (post-M07)

- **B-M7-1**: Every `Box::new(...)` evaluation emits exactly one `HeapAlloc` event with size matching the inner type's byte size.
- **B-M7-2**: Every Vec push that grows capacity emits exactly one `HeapRealloc` event; otherwise just a `SlotWrite`-like in-place update (M07 may need a new event variant for in-place vec writes, OR can reuse SlotWrite with synthetic slot ids — plan-phase decides; default: no event for in-place writes since the heap state view derived from event stream alone handles it).
- **B-M7-3**: Every dangling borrow at HeapRealloc time produces a `Note { RuntimeError }` with the original borrow's span.
- **B-M7-4**: HeapFree fires for every Box/Vec/String binding at its scope's exit, BEFORE the SlotDrop for the same slot.
- **B-M7-5**: `Value::Ref.target` is `Pointee::Heap(addr)` iff the borrow targets a heap allocation; `Pointee::Slot(id)` iff it targets a stack slot.
- **B-M7-6**: `ArrowView.kind == Owning` iff the source slot's value is `Value::Box/Vec/String`. `ArrowView.kind == Shared/Mut` iff the source slot's value is `Value::Ref { mutable: false/true }`.

## What this contract does NOT cover (deferred)

- **`HashMap`, `Rc`, `RefCell`, other heap types** — explicitly out of scope per MILESTONES.md.
- **Threads, `Arc`, `Mutex`** — M08.
- **`Vec<T>` for non-Copy T** (e.g. `Vec<Box<i32>>`) — out of scope; M07 supports only primitive-element Vecs.
- **Indexing assignment** (`v[0] = 5;`) — out of scope (extends M06.1's place-expression set).
- **Box re-borrows** (`&*b`, `&mut *b`) — deferred from M06.1.
- **Vec borrows other than `&v[0]`** (e.g. `&v[..]` slice borrows) — out of scope.
- **Method chaining** (`v.push(x).foo()`) — parser supports it syntactically (postfix loop) but no chained M07 method returns a usable receiver. Not a real limitation, just unused.
- **Vec::with_capacity, Vec::iter, Vec::clear, etc.** — out of scope.
