# Contract — M07.4 Protocol Delta

**8th invocation** of the closed-enum-with-revisions rule. M07.4 adds two variants (`Ty::Struct`, `Value::Struct`) and extends one existing variant (`Value::Ref` gains a `field_path: Vec<String>` field with serde default + skip-when-empty). No new `MemEvent` variants. No new `Pointee` variants.

## Closed-enum rule — eighth invocation

| Milestone | Change |
|---|---|
| M03.1 | Added `MemEvent::ReturnValue`; removed `FrameEnter.params` (additive + redundant-field removal). |
| M03.2 | Restructured `Ty` and `Value` (kind-based). Rule generalized to all protocol types. |
| M06 | Added `Ty::Ref`, `Value::Ref`. Filled `MemEvent::BorrowShared/BorrowMut/BorrowEnd` payloads. |
| M07 | Added `Ty::Box/Vec/String`, `Value::Box/Vec/String/Str`. Restructured `Value::Ref` (target_slot → target: Pointee). Filled `MemEvent::HeapAlloc/HeapRealloc/HeapFree` payloads. |
| M07.1 | Added `Ty::Slice(Box<Ty>)` and `Value::Slice { .. }`. Pure additive. |
| M07.2 | Added `StaticAddr`, `Pointee::Static`, `Ty::Str`, `MemEvent::StaticAlloc`, `MemEvent::BytesCopy`, `ArrowTarget::Static`. Removed `Value::Str` (dead transient). |
| M07.3 | Added `Ty::Array(Box<Ty>, u64)` and `Value::Array { elements, elem_ty }`. Pure additive. First scenario constructing `Value::Slice { target: Pointee::Slot(_), .. }`. |
| **M07.4** | Adds `Ty::Struct { name, fields }` and `Value::Struct { name, fields }`. **Extends** `Value::Ref` with `field_path: Vec<String>` (serde-default-empty). First scenario constructing `Value::Ref { field_path: vec![name], target: Pointee::Slot(_), .. }`. |

The `Value::Ref` extension is the first time we've ADDED a field to an existing variant (vs. adding a new variant). Justification: a new variant (`Value::FieldRef`) would split every `Value::Ref` consumer into a two-arm match where one arm just delegates to the other — pure boilerplate. The extension uses `#[serde(default, skip_serializing_if = "Vec::is_empty")]` to preserve byte-identical M06+ borrow snapshots.

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
    Array(Box<Ty>, u64),
    // NEW in M07.4:
    Struct {
        name: String,
        fields: Vec<(String, Ty)>,
    },
}
```

JSON shape gains one new tag: `{ "Struct": { "name": "Point", "fields": [["x", { "Int": "I32" }], ["y", { "Int": "I32" }]] } }`.

## `Value` — additive variant

```rust
pub enum Value {
    Int { kind, bits },
    Float { kind, value },
    Bool(bool),
    Unit,
    Ref { borrow_id, target, mutable, field_path },  // ← field_path NEW (extension)
    Box { addr },
    Vec { addr },
    String { addr },
    Slice { borrow_id, target, start, len, mutable, byte_offset, byte_len },
    Array { elements, elem_ty },
    // NEW in M07.4:
    Struct {
        name: String,
        fields: Vec<(String, Value)>,
    },
}
```

JSON shape: `{ "Struct": { "name": "Point", "fields": [["x", { "Int": { "kind": "I32", "bits": 1 } }], ["y", { "Int": { "kind": "I32", "bits": 2 } }]] } }`.

## `Value::Ref` — extension (additive field)

```rust
Value::Ref {
    borrow_id: BorrowId,
    target: Pointee,
    mutable: bool,
    /// **M07.4 ADDITIVE EXTENSION**: navigation path into a sub-field of
    /// the target. Empty = whole binding (M06+ semantics). Non-empty =
    /// field borrow (`&p.x` → `vec!["x"]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    field_path: Vec<String>,
}
```

JSON shape **when `field_path` is empty** (M06+ borrows): identical to pre-M07.4 — `{ "Ref": { "borrow_id": 0, "target": { "Slot": 1 }, "mutable": false } }`. The `field_path` field is omitted entirely thanks to `skip_serializing_if`.

JSON shape **when `field_path` is non-empty** (M07.4 field borrows): `{ "Ref": { "borrow_id": 0, "target": { "Slot": 1 }, "mutable": false, "field_path": ["x"] } }`.

Deserialization defaults `field_path` to `Vec::new()` when absent → existing borrow traces parse unchanged.

## `MemEvent` — no changes

M07.4 reuses existing events for everything:
- Struct binding: existing `SlotAlloc` + `SlotWrite` carry the `Value::Struct` payload.
- Method call: existing `FrameEnter` + per-param `SlotAlloc`/`SlotWrite` + `ReturnValue` + `FrameLeave`. `self` is just another param slot.
- Associated function call: same as method call without the `self` slot.
- Field borrow: lazily materialized in the UI's `apply_event` SlotWrite arm (no `BorrowShared`/`BorrowEnd` emitted — slot-target borrows follow M07.3's pattern).
- Field assignment (if deferral #2 lands): existing `SlotWrite` with the mutated `Value::Struct`.

### Heap events ABSENT for struct-only programs

A defining pedagogical property: a program that only uses structs (no Box/Vec/String/String::from) emits **zero** `HeapAlloc`/`HeapRealloc`/`HeapFree` events. The heap panel stays empty for the entire trace. Echoes M07.3's array property.

## `Pointee` — no shape change; usage expansion

```rust
pub enum Pointee {
    Slot(SlotId),       // M03 declared; M06 used for `&x`; M07.3 used for slice slot-target; M07.4 ALSO used for field borrows
    Heap(HeapAddr),
    Static(StaticAddr),
}
```

M07.4 starts producing `Value::Ref { target: Pointee::Slot(_), field_path: vec![name] }` when `&p.x` is evaluated. The `Slot` variant itself is unchanged.

## `ArrowTarget` — no shape change; field_label added to `ArrowView`

```rust
// In src/ui.rs — UI-side view (not part of the wire event protocol)
pub struct ArrowView {
    pub source_slot: u32,
    pub target: ArrowTarget,
    pub mutable: bool,
    pub slice_len: Option<u64>,
    pub slice_byte_offset: Option<u64>,
    pub slice_byte_len: Option<u64>,
    pub slice_elem_start: Option<u64>,
    /// **M07.4**: present for field-borrow arrows (`&p.x`). Drives the
    /// `.x` annotation rendered at the arrow midpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_label: Option<String>,
}
```

## Behavioral guarantees (post-M07.4)

- **B-M74-1**: Every `Expr::StructLit` evaluation produces a `Value::Struct { name, fields }` with `fields.len()` matching the struct schema and `fields[i].0` matching the declared field name at index i.
- **B-M74-2**: Struct-only programs emit zero `HeapAlloc`, `HeapRealloc`, or `HeapFree` events.
- **B-M74-3**: `p.x` on `p: Point { x: i32, .. }` returns a clone of the named field's value. `p` remains usable (Copy semantics for primitive-only structs in M07.4).
- **B-M74-4**: `&p.x` produces `Value::Ref { borrow_id, target: Pointee::Slot(p_slot), mutable: false, field_path: vec!["x"] }`. NO `BorrowShared`/`BorrowEnd` event emitted (slot-target lazy materialization per M07.3).
- **B-M74-5**: `let v = p.method();` on a user-defined `impl Point { fn method(&self) -> .. { .. } }` enters a new frame via `FrameEnter`, binds `self` to a `Value::Ref { target: Pointee::Slot(p_slot), .. }`, executes the body, emits `ReturnValue` then `FrameLeave`, lands the result in `v`.
- **B-M74-6**: `let p = Point::new(1, 2);` dispatches to `ImplRegistry.assoc_fns[["Point", "new"]]`; enters a new frame, binds params (NO self slot), executes body, returns the struct, lands it in `p`.
- **B-M74-7**: Method dispatch tie-breaker: hardcoded M07 built-ins (`Vec::push`, `Vec::len`, etc.) win over user-defined methods if names collide. Practical impact: minimal (M07.4 typeck rejects `impl Vec { .. }` outright since `Vec` is not a user-defined struct).
- **B-M74-8**: Two-pass typeck: phase 1 collects struct schemas + impl signatures; phase 2 typechecks fn bodies. Forward references (`impl Point` before `struct Point`) work.
- **B-M74-9**: Auto-deref for `self.x` in method bodies: typeck accepts `Ty::Ref { Ty::Struct(_), .. }` as a `FieldAccess` receiver; eval looks up the target slot's `Value::Struct` and reads the named field.
- **B-M74-10**: SlotRowView gains `struct_view: Option<StructView>` for struct slots; mutually exclusive with `value: Some(_)` and `inline_cells: Some(_)`.
- **B-M74-11**: Field-borrow arrow hover lights up only the borrowed field's row in the target slot's struct view (per-field hover).
- **B-M74-12**: `Value::Ref` JSON shape stays byte-identical for empty `field_path` — existing M06+ borrow snapshots unchanged.

## What this contract does NOT cover (deferred)

- **Non-Copy field types**: struct fields restricted to primitives in M07.4. `struct Foo { v: Vec<i32> }` rejected. Future M07.x lifts.
- **Nested structs**: `struct Inner { x: i32 } struct Outer { i: Inner }` — out of scope.
- **Multi-level field access / borrow**: `p.x.y`, `&p.x.y` — out of scope. Parser accepts via left-associativity; typeck rejects.
- **Struct update syntax**: `Point { x: 10, ..p }` — out of scope.
- **Tuple structs**: `struct Pair(i32, i32)` — out of scope.
- **Unit structs**: `struct Marker;` — out of scope.
- **Empty structs**: `struct Empty {}` — out of scope (parser rejects).
- **Pattern matching on struct fields**: `let Point { x, y } = p;` — out of scope.
- **Generic structs / methods**: `Point<T>`, `fn foo<T>(&self) -> T` — out of scope.
- **Traits / trait impls / trait objects**: out of scope. Only inherent impls.
- **Derive macros**: `#[derive(Debug, Clone)]` — out of scope.
- **Multiple impl blocks per type**: typeck rejects the second block.
- **Recursive structs** (`struct A { a: A }`, even with `Box`): out of scope; the primitive-only field restriction implicitly forbids them.
- **`Drop` impls**: out of scope.
- **Field-assignment scope** (`p.x = 5;`, `self.x = v;`): partial — plan-phase decides whether to support based on M06.1 place-expression extension cost. Recommendation: support.
