# Contract — M07.3 Protocol Delta

7th invocation of the closed-enum-with-revisions rule. M07.3 adds variants only — no existing variant changes shape, no new event variants, no Pointee changes (the `Slot` variant declared in M03 is finally exercised by `Value::Slice` targets).

## Closed-enum rule — seventh invocation

| Milestone | Change |
|---|---|
| M03.1 | Added `MemEvent::ReturnValue`; removed `FrameEnter.params` (additive + redundant-field removal). |
| M03.2 | Restructured `Ty` and `Value` (kind-based). Rule generalized to all protocol types. |
| M06 | Added `Ty::Ref`, `Value::Ref`. Filled `MemEvent::BorrowShared/BorrowMut/BorrowEnd` payloads. |
| M07 | Added `Ty::Box/Vec/String`, `Value::Box/Vec/String/Str`. **Restructured `Value::Ref`** (target_slot → target: Pointee). Filled `MemEvent::HeapAlloc/HeapRealloc/HeapFree` payloads. |
| M07.1 | Added `Ty::Slice(Box<Ty>)` and `Value::Slice { .. }`. Pure additive. |
| M07.2 | Added `StaticAddr`, `Pointee::Static`, `Ty::Str`, `MemEvent::StaticAlloc`, `MemEvent::BytesCopy`, `ArrowTarget::Static`. Removed `Value::Str` (dead transient). |
| **M07.3** | Adds `Ty::Array(Box<Ty>, u64)` and `Value::Array { elements, elem_ty }`. **Pure additive** — no event-variant changes, no Pointee changes. First scenario constructing `Value::Slice { target: Pointee::Slot(_), .. }`. |

The pure-additive nature means M07.3 introduces zero risk to existing snapshot tests — every M01/M02/M03 snapshot stays byte-identical because no sample constructs the new variants.

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
    Slice(Box<Ty>),
    Str,
    // NEW in M07.3:
    Array(Box<Ty>, u64),
}
```

JSON shape gains one new tag: `{ "Array": [<Ty>, <u64>] }`.

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
    Slice { borrow_id, target, start, len, mutable, byte_offset, byte_len },
    // NEW in M07.3:
    Array {
        elements: Vec<Value>,
        elem_ty: Ty,
    },
}
```

JSON shape: `{ "Array": { "elements": [...], "elem_ty": <Ty> } }`.

## `MemEvent` — no changes

M07.3 reuses existing `SlotAlloc` and `SlotWrite` events for arrays — the binding's slot allocation and the array's first SlotWrite carry the `Value::Array { .. }` payload. No new variants.

### Heap events are ABSENT for array-only programs

A defining pedagogical property: a program that only uses arrays (no Box/Vec/String/String::from) emits **zero** `HeapAlloc`/`HeapRealloc`/`HeapFree` events. The heap panel stays empty for the entire trace. This is the headline contrast vs `Vec`.

## `Pointee` — usage expansion (no shape change)

```rust
pub enum Pointee {
    Slot(SlotId),       // M03 declared; M06 used for `&x` borrows; M07.3 ALSO used by Value::Slice targets
    Heap(HeapAddr),     // M07 first produced for `&v[0]`; M07.1 produced for `&v[range]` slices
    Static(StaticAddr), // M07.2 produced for `&"hi"` and `let s = "hi"` literals
}
```

M07.3 starts producing `Value::Slice { target: Pointee::Slot(_), .. }` when `&t[range]` is evaluated on an array binding. The `Slot` variant itself is unchanged from M03; only its usage as a Slice target is new.

## `ArrowTarget` — no changes (already supports Slot)

```rust
pub enum ArrowTarget {
    Slot(u32),       // M06 (`&x` borrow arrow); M07.3 ALSO used by Pointee::Slot slice arrows
    Heap(u32),
    Static(u32),
}
```

The existing M06 slot-to-slot arrow routing in the renderer handles M07.3's slice arrows with no changes. The slice's `[len: N]` annotation + hover-highlight machinery from M07.1/M07.2 generalizes via the broadened `.elem-cell` / `.byte-cell` CSS scopes.

## Behavioral guarantees (post-M07.3)

- **B-M73-1**: Every `Expr::ArrayLit` evaluation produces a `Value::Array { elements, elem_ty }` with `elements.len() == N` from the type.
- **B-M73-2**: Array-only programs emit zero `HeapAlloc`, `HeapRealloc`, or `HeapFree` events.
- **B-M73-3**: `t.len()` on `t: [T; N]` returns `Value::Int { kind: U64, bits: N as i128 }` — computed from the value's `elements.len()`, equivalent to the type's N.
- **B-M73-4**: `t[i]` with valid `i < N` returns a clone of `elements[i]` (the element type is Copy in M07.3 so clone is byte-equivalent).
- **B-M73-5**: `t[i]` with `i >= N` or `i < 0` emits `Note { RuntimeError, message: "index out of bounds: array len is N but the index is M", span }` and halts.
- **B-M73-6**: `&t[range]` on `t: [T; N]` produces `Value::Slice { target: Pointee::Slot(t_slot), start, len, byte_offset: start * elem_size, byte_len: len * elem_size, mutable: false, .. }`.
- **B-M73-7**: Slot-target slice borrows (`Pointee::Slot(_)` from arrays) skip `BorrowShared`/`BorrowEnd` emission (M07.2 pattern). The UI materializes the arrow lazily in `apply_event`'s SlotWrite arm when it sees a slot-target slice with no matching world.borrows entry.
- **B-M73-8**: Array assignment (`let t2 = t1`) clones the `Value::Array` deep (each element). Both `t1` and `t2` remain usable (Copy semantics).
- **B-M73-9**: Slicing an array with OOB range emits `Note { RuntimeError }` and halts.
- **B-M73-10**: SlotRowView gains `inline_cells: Option<InlineCellsView>` for array slots; mutually exclusive with `value: Option<String>`.

## What this contract does NOT cover (deferred)

- **Repeat syntax `[v; N]`** — out of scope. Literal-only `[e1, e2, ..., eN]` form is sufficient for pedagogical samples.
- **Multi-dimensional arrays `[[T; N]; M]`** — out of scope.
- **Arrays of non-Copy types** — out of scope (matches M07's Vec-of-primitives restriction).
- **Mutation through index** `t[0] = 5;` — out of scope (extending M06.1's place-expression set to include `Expr::Index`).
- **Array iteration** `for x in t`, `t.iter()` — out of scope (matches M07.1's slice-method deferrals).
- **Slicing temporaries** `&[1,2,3][1..2]` — out of scope. Only directly-bound arrays (`&t[range]` where `t: Expr::Ident`) supported.
- **Const generics / const expressions in array size** — out of scope. Size must be a literal integer.
- **Array as function return type with complex bounds checks** — partial scope; basic `fn foo() -> [i32; 3]` works via standard value-flow.
- **Array equality `[1, 2] == [1, 2]`** — out of scope.
