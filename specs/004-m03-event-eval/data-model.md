# Data Model — M03 Entities

M03 introduces the runtime event vocabulary. All types live in `src/event.rs`; the evaluator's internal state lives in `src/eval.rs` and is private.

## Public entity: `SlotId`

```rust
pub struct SlotId(pub u32);
```

Newtype, derives `Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord`. Fresh per runtime slot. Distinct from M02's `BindingId` — see research R-005.

## Public entity: `FrameId`

```rust
pub struct FrameId(pub u32);
```

Same derives as `SlotId`. Fresh per function call. Sequential from 0.

## Public entity: `HeapAddr`

```rust
pub struct HeapAddr(pub u32);
```

Forward-compat placeholder for M07. Defined here so the `Pointee` enum and `HeapAlloc`/`HeapRealloc`/`HeapFree` variants compile cleanly without breaking changes later.

## Public entity: `BorrowId`

```rust
pub struct BorrowId(pub u32);
```

Forward-compat placeholder for M06. Defined here for the same reason as `HeapAddr`.

## Public entity: `Pointee`

```rust
pub enum Pointee {
    Slot(SlotId),
    Heap(HeapAddr),
}
```

Per CLAUDE.md › Event model: "Pointee is an enum `Slot(SlotId) | Heap(HeapAddr)` — a `&T` can point into the stack or the heap." M03 evaluator never constructs `Pointee::Heap` (no heap in L1), but the variant exists.

## Public entity: `Value`

```rust
pub enum Value {
    Int(i64),
    Bool(bool),
    Unit,
}
```

The L1 value lattice. Derives `Debug, Clone, PartialEq`. `Int` uses `i64` internally (research R-007).

## Public entity: `NoteKind`

```rust
pub enum NoteKind {
    /// Runtime error: overflow, division by zero, recursion depth exceeded.
    RuntimeError,
    /// Placeholder for future kinds (M06 dangling borrow, M07 realloc invalidation, M08 lock poisoning).
    /// M03 defines this kind so the `Note` variant has a coherent enum to refer to.
    Info,
}
```

M03 emits `RuntimeError`. `Info` reserved for benign pedagogical messages. Later milestones may add more variants (additive).

## Public entity: `MemEvent`

The central enum. Variants for every category in CLAUDE.md › Event model. Each variant carries a `span: Span` field. Derives `Debug, Clone, PartialEq`.

```rust
pub enum MemEvent {
    // Threads (M08)
    ThreadSpawn { thread_id: u32, span: Span },
    ThreadJoin  { thread_id: u32, span: Span },
    ThreadPark  { thread_id: u32, lock: HeapAddr, span: Span },

    // Frames (M03)
    FrameEnter {
        frame_id: FrameId,
        fn_name: String,
        params: Vec<(SlotId, String, Value)>,
        span: Span,
    },
    FrameLeave {
        frame_id: FrameId,
        return_value: Value,
        span: Span,
    },

    // Stack slots (M03 for L1, extended L2-L4)
    SlotAlloc {
        slot_id: SlotId,
        name: String,
        ty: crate::typeck::Ty,
        span: Span,
    },
    SlotWrite {
        slot_id: SlotId,
        value: Value,
        span: Span,
    },
    SlotMove {
        from: SlotId,
        to: SlotId,
        value: Value,
        span: Span,
    },
    SlotDrop {
        slot_id: SlotId,
        span: Span,
    },

    // Heap (M07)
    HeapAlloc  { addr: HeapAddr, size: u32, ty_name: String, span: Span },
    HeapRealloc { from: HeapAddr, to: HeapAddr, new_size: u32, span: Span },
    HeapFree   { addr: HeapAddr, span: Span },

    // Borrows (M06)
    BorrowShared { borrow_id: BorrowId, target: Pointee, span: Span },
    BorrowMut    { borrow_id: BorrowId, target: Pointee, span: Span },
    BorrowEnd    { borrow_id: BorrowId, span: Span },

    // Synchronization (M08)
    LockAcquire { addr: HeapAddr, span: Span },
    LockRelease { addr: HeapAddr, span: Span },
    ArcClone    { addr: HeapAddr, span: Span },
    ArcDrop     { addr: HeapAddr, span: Span },

    // Pedagogy (M03 infrastructure, all milestones may emit)
    Note {
        kind: NoteKind,
        message: String,
        span: Span,
    },
}
```

Total 19 variants. M03 actively emits: `FrameEnter`, `FrameLeave`, `SlotAlloc`, `SlotWrite`, `SlotDrop`, and `Note` (for runtime errors). `SlotMove` exists but isn't emitted by L1 evaluation (FR-006) — verified by a unit test that constructs it (research R-008). All other variants compile but are inert in M03.

### Validation rules

- **VR-1**: Every variant carries a non-zero `Span` (`span.start < span.end`, or zero-length only at deliberate end-of-input positions like `FrameLeave` at end of empty body). Verified by SC-002 inspection.
- **VR-2**: `FrameEnter` and `FrameLeave` are properly paired in LIFO order across the stream. A `FrameLeave` for `frame_id N` always appears after the matching `FrameEnter` and after all events nested inside that frame.
- **VR-3**: For each frame, `SlotAlloc`/`SlotDrop` events for a given `slot_id` are paired in LIFO order within the frame.
- **VR-4**: `SlotWrite` for a given `slot_id` always follows its `SlotAlloc` and precedes its `SlotDrop`.
- **VR-5**: `SlotMove` is never emitted by the M03 L1 evaluator (FR-006). The variant exists in the enum but won't appear in any L1 snapshot.
- **VR-6**: `Note { kind: RuntimeError, ... }` is always the last event in a stream that ended due to a runtime error. Subsequent events would not be emitted.
- **VR-7**: `Value::Int(i)` for `i` outside `[i32::MIN, i32::MAX]` is permitted internally (`i64` storage, research R-007) but visualizations may flag it.

## Private internal entities (in `src/eval.rs`)

### `Evaluator<'a>`

```rust
struct Evaluator<'a> {
    program: &'a ast::Program,
    resolution: &'a Resolution,
    types: &'a TypeMap,
    /// BindingId → FnDecl lookup, built once at construction (research R-011).
    fn_decls: HashMap<BindingId, &'a ast::FnDecl>,
    /// Call stack, innermost last.
    frames: Vec<Frame>,
    /// Sequential SlotId allocator.
    next_slot_id: u32,
    /// Sequential FrameId allocator.
    next_frame_id: u32,
    /// Emitted events.
    events: Vec<MemEvent>,
    /// Set on runtime error to stop further evaluation.
    halted: bool,
}
```

### `Frame`

```rust
struct Frame {
    frame_id: FrameId,
    fn_binding: BindingId,
    scopes: Vec<Scope>,
}

struct Scope {
    /// Locals in declaration order; LIFO drop at scope exit.
    locals: Vec<LocalSlot>,
}

struct LocalSlot {
    binding_id: BindingId,
    slot_id: SlotId,
    name: String,
    value: Value,
    decl_span: Span,
}
```

These types are not part of the public API; they may evolve freely.

## Relationships

```
ast::Program ─┐
Resolution ───┼──► evaluate() ──► Vec<MemEvent>
TypeMap ──────┘

Each MemEvent.span → points back to an ast node's span
SlotAlloc.slot_id ─── SlotWrite.slot_id ─── SlotDrop.slot_id (paired)
FrameEnter.frame_id ─── FrameLeave.frame_id (paired)
```

## State transitions

The evaluator is stateful during a `evaluate()` call but stateless from the caller's perspective (one-shot function). `halted` transitions from `false` to `true` on runtime error and remains `true`; further events are not pushed.

## Reused: `Span`, `ParseError` (from M01)

No new error type. M03's static-time errors (e.g. `M02 produced a TypeMap missing an entry that M03 needs`) surface as `ParseError`; runtime errors surface as `Note` events.
