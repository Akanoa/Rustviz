# Data Model — M07.3 Entities

Stack-array expansion: 1 new AST expression (`Expr::ArrayLit`), 1 new AST type (`Type::Array`), 1 new `Ty` variant (`Array(Box<Ty>, u64)`), 1 new `Value` variant (`Array { elements, elem_ty }`), `SlotRowView` extension (`inline_cells: Option<InlineCellsView>`).

All purely additive. No restructure of any existing variant. No new MemEvent variants.

## New (AST): `Expr::ArrayLit`

```rust
// In src/parse/ast.rs

pub enum Expr {
    // ... existing variants
    /// **M07.3**: array literal `[e1, e2, ..., eN]`. The size N is
    /// `elements.len()` (no separate size field). Inferred type:
    /// `Ty::Array(elem_ty, elements.len())` after all elements unify.
    ArrayLit {
        /// Element expressions in source order.
        elements: Vec<Expr>,
        /// Span from `[` through `]`.
        span: Span,
    },
}
```

### Validation rules

- **VR-1**: `ArrayLit` parses when `parse_atom` sees `LBracket` at expression-atom position. The existing postfix-`[` (for `expr[i]` indexing) is unaffected because that fires only after an atom has been parsed.
- **VR-2**: `Expr::span()` extends to cover `ArrayLit { span, .. } => *span`.
- **VR-3**: Empty literal `[]` parses successfully; typeck rejects without annotation (can't infer element type).
- **VR-4**: All elements must unify to a common type at typeck via `try_coerce_to` (handles literal-narrowing patterns like `[1u8, 2]`).

## New (AST type): `Type::Array`

```rust
// In src/parse/ast.rs

pub enum Type {
    // ... existing variants (Path, Unit, Ref, Generic, Slice)
    /// **M07.3**: array type annotation `[T; N]` where N is an integer
    /// literal (no const expressions in M07.3).
    Array {
        /// Element type.
        inner: Box<Type>,
        /// Compile-time-known size N.
        size: u64,
        /// Span from `[` through `]`.
        span: Span,
    },
}
```

### Validation rules

- **VR-5**: `Type::Array` lowers to `Ty::Array(inner_ty, size)` at typeck.
- **VR-6**: Parsed by `parse_type` when LBracket appears at the type-context entry. Inside: `parse_type` for inner, expect `Semi`, expect `Int(n, _)` literal (n >= 0), expect `RBracket`.
- **VR-7**: When both `Type::Array { inner, size }` and `Expr::ArrayLit { elements }` are present in the same `let` binding, `size == elements.len() as u64` (typeck error on mismatch).

## Modified: `Ty` — adds `Array`

```rust
pub enum Ty {
    // ... existing variants
    /// **M07.3**: array type `[T; N]`. Stack-allocated, fixed size known
    /// at compile time. Distinct from `Ty::Vec(T)` (heap-allocated, runtime
    /// size) and `Ty::Slice(T)` (size-erased borrow). Copy iff `T: Copy`.
    Array(Box<Ty>, u64),
}
```

### Validation rules

- **VR-8**: `Ty::is_copy()` returns true iff `inner.is_copy()`. In M07.3 elements are restricted to primitives (always Copy), so `Ty::Array(_, _)` is always Copy.
- **VR-9**: `Ty::name()` renders as `"[<inner_name>; <size>]"` (e.g. `"[i32; 3]"`).
- **VR-10**: Two `Ty::Array` are equal iff both element types and sizes match.

## Modified: `Value` — adds `Array`

```rust
pub enum Value {
    // ... existing variants
    /// **M07.3**: array value — N elements of `elem_ty` held inline in
    /// the binding's stack slot. No heap allocation; the slot's bytes
    /// ARE the array's bytes. Slicing produces `Value::Slice` with
    /// `target: Pointee::Slot(receiver_slot)`.
    Array {
        /// Element values in index order. Length = N from the type.
        elements: Vec<Value>,
        /// Element type. Used for sizing (`N * elem_size`) and as the
        /// parent type when this array is sliced (`Ty::Slice(elem_ty)`).
        elem_ty: Ty,
    },
}
```

### Validation rules

- **VR-11**: `Value::Array.elements.len()` matches the binding's `Ty::Array(_, N).1`. Set at construction; never resized (arrays are fixed-size).
- **VR-12**: `Value::Array.elem_ty` matches the binding's `Ty::Array(elem_ty, _).0`.
- **VR-13**: `Value::type_name()` returns `"[]"` (short tag; full `[T; N]` rendering comes from the `Ty` layer).
- **VR-14**: Cloning a `Value::Array` deep-copies the `Vec<Value>` (each element cloned). Used for the Copy-style assignment `let t2 = t1`.

## Modified: `SlotRowView` — adds `inline_cells`

```rust
// In src/ui.rs

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlotRowView {
    pub slot_id: u32,
    pub name: String,
    pub ty: String,
    pub value: Option<String>,
    /// **M07.3**: present when the slot holds a `Value::Array`. The JS
    /// renders inline byte-cells in the slot's value area (instead of
    /// the text `value` field). Mirrors the per-byte-cell + per-element
    /// rendering used for heap blocks, but visually distinct
    /// (gray-tinted) to convey "stack memory" not "heap memory".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_cells: Option<InlineCellsView>,
}

/// **M07.3**: inline byte-cell rendering for a stack-allocated array.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InlineCellsView {
    /// Total byte size (`N * elem_size`).
    pub size: u32,
    /// Used bytes — for arrays always equals `size` (fully populated at
    /// construction). Kept as a field for parallelism with HeapView.
    pub used: u32,
    /// Per-element display strings (e.g. `["1_i32", "2_i32", "3_i32"]`).
    /// Drives both the byte-cell visualization AND the per-element
    /// hover-highlight when a slice arrow points into this slot.
    pub elements: Vec<String>,
}
```

### Validation rules

- **VR-15**: `inline_cells` is `Some(_)` iff the slot's `Value` is `Value::Array`. Mutually exclusive with `value: Some(_)`.
- **VR-16**: `inline_cells.size == N * elem_size_bytes(elem_ty)` for the array's element type.
- **VR-17**: `inline_cells.elements.len() == N` (one display string per element).
- **VR-18**: `serde(skip_serializing_if = "Option::is_none")` keeps existing slot-row JSON unchanged for non-array slots.

## Slot-targeted slice value (new construction site)

```rust
// In src/event.rs — Value::Slice already exists (M07.1)

Value::Slice {
    borrow_id: BorrowId(...),
    target: Pointee::Slot(SlotId(...)),  // ← NEW construction site in M07.3
    start: u64,
    len: u64,
    mutable: false,  // mutable slices are out of scope per M07.1
    byte_offset: u64,
    byte_len: u64,
}
```

### Validation rules

- **VR-19**: `Pointee::Slot(_)` target on `Value::Slice` is constructed only when slicing an `Expr::Ident`-receiver of array type (`&t[range]` where `t: [T; N]`).
- **VR-20**: Slot-target slice borrows skip `BorrowShared`/`BorrowEnd` events (consistent with M07.2's Static treatment). The UI's `apply_event` SlotWrite arm lazily materializes the borrow with `source_slot` bound when it sees a `Value::Slice` with no matching world.borrows entry.
- **VR-21**: The dangling-borrow scan in `realloc_heap` already ignores `Pointee::Slot(_)` targets (frames disappear atomically; no per-element dangling within an array slot).

## New: M07.3 reference samples

| File | Content | Pedagogy |
|---|---|---|
| `tests/samples/m07_3_array_basic.rs` | `fn main() { let t = [10, 20, 30]; let n = t.len(); }` | Stack-allocated array; inline byte-cells in t's slot; `n = 3_u64`; zero heap events. |
| `web/samples/m07_3_array_basic.rs` | Mirror. | |
| `tests/samples/m07_3_array_index.rs` | `fn main() { let t = [10, 20, 30]; let x = t[1]; }` | Array indexing returns element copy; `x = 20_i32`. |
| `web/samples/m07_3_array_index.rs` | Mirror. | |
| `tests/samples/m07_3_array_slice.rs` | `fn main() { let t = [1, 2, 3, 4]; let s = &t[1..3]; }` | Array slicing produces `Value::Slice { target: Pointee::Slot(t_slot), len: 2, .. }`; blue slice arrow from `s` to `t` (slot-to-slot routing); `[len: 2]` on hover. |
| `web/samples/m07_3_array_slice.rs` | Mirror. | |
