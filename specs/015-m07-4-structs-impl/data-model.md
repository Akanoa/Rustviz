# Data Model — M07.4 Entities

User-defined struct types + impl blocks. **2 new AST items**, **2 new `Expr` variants**, **`Param` extension** for self-receivers, **1 new `Ty` variant**, **1 new `Value` variant**, **1 `Value::Ref` extension** for field-path metadata, **`SlotRowView` extension** for struct rendering, **`ArrowView` extension** for field labels.

All `Value`/`Ty` changes are additive at the JSON layer (the `field_path` extension uses serde-default-empty so existing snapshots stay byte-identical).

## New (AST): `Item::Struct`

```rust
// In src/parse/ast.rs

pub enum Item {
    Fn(FnDecl),
    /// **M07.4**: struct declaration `struct Name { f1: T1, f2: T2 }`.
    /// At least one field required (empty structs typeck-rejected per
    /// spec edge case).
    Struct(StructDecl),
    /// **M07.4**: inherent impl block `impl Type { fn ...; fn ...; }`.
    Impl(ImplBlock),
}

pub struct StructDecl {
    /// Type name.
    pub name: String,
    /// Fields in declaration order. Order drives byte layout AND drop order.
    pub fields: Vec<StructField>,
    /// Span from `struct` keyword through closing `}`.
    pub span: Span,
}

pub struct StructField {
    /// Field name.
    pub name: String,
    /// Field type.
    pub ty: Type,
    /// Span covering `name: ty`.
    pub span: Span,
}
```

### Validation rules

- **VR-1**: `StructDecl.fields.len() >= 1` (empty struct rejected at parse time with "structs in M07.4 must have at least one field").
- **VR-2**: Two fields in the same struct may not share a name (parse-time error or typeck phase 1 error).
- **VR-3**: A field's `ty` must resolve at typeck phase 1 to one of: `Ty::Int`, `Ty::Float`, `Ty::Bool`, `Ty::Unit`. Non-Copy types (`Ty::Vec`, `Ty::String`, `Ty::Slice`, `Ty::Box`, `Ty::Array`, `Ty::Str`, other `Ty::Struct`) rejected ("M07.4 fields must be primitive types").

## New (AST): `Item::Impl`

```rust
pub struct ImplBlock {
    /// Receiver type name (`"Point"`). Single-segment in M07.4.
    pub ty_name: String,
    /// Associated functions + methods declared inside this block.
    pub items: Vec<FnDecl>,
    /// Span from `impl` keyword through closing `}`.
    pub span: Span,
}
```

### Validation rules

- **VR-4**: `ty_name` must reference a `StructDecl` declared in the same file (phase 1 typeck verifies; "impl block references unknown type `<name>`" if not).
- **VR-5**: M07.4 supports one impl block per type. A second `impl Point { .. }` typeck-rejects with "M07.4 supports only one impl block per type; merge into a single block" (named with the prior block's location for clarity).
- **VR-6**: Each item is a `FnDecl`. The first `Param` may be a self-receiver (kind ≠ Normal); subsequent params must be Normal.

## Modified (AST): `Param` adds `kind: ParamKind`

```rust
pub enum ParamKind {
    /// Regular `name: Type` param.
    Normal,
    /// `self` (owned receiver). Only valid as the first param of an impl-block fn.
    SelfOwned,
    /// `&self` (shared borrow receiver).
    SelfShared,
    /// `&mut self` (mutable borrow receiver).
    SelfMut,
}

pub struct Param {
    pub name: String,
    pub ty: Type,
    /// **M07.4**: param classification. Defaults to `Normal` for all
    /// pre-M07.4 free-fn params.
    pub kind: ParamKind,
    pub span: Span,
}
```

### Validation rules

- **VR-7**: Self-receivers (kind ≠ Normal) only allowed at param index 0 of an `Item::Impl`'s fn decls. Parser rejects out-of-position self with "`self` parameter must be the first parameter".
- **VR-8**: Self-receivers in free-fn decls (`Item::Fn`) rejected at parse time ("self parameters only valid inside impl blocks").
- **VR-9**: When `kind` is `SelfOwned`, the param's `ty` is set to `Type::Path { segments: [<impl_block_ty_name>], .. }` synthesized at parse time (placeholder span). Phase 1 typeck swaps this for `Ty::Struct { .. }`.
- **VR-10**: When `kind` is `SelfShared` / `SelfMut`, the param's `ty` is `Type::Ref { inner: <as above>, mutable: <true for SelfMut>, .. }`.

## New (AST): `Expr::StructLit`

```rust
pub enum Expr {
    // ... existing variants
    /// **M07.4**: struct literal `Path { f1: e1, f2: e2 }`.
    StructLit {
        /// Path segments (single-segment in M07.4 — `["Point"]`).
        path: Vec<String>,
        /// Field initializers in source order.
        fields: Vec<StructLitField>,
        /// Span from path start through closing `}`.
        span: Span,
    },
}

pub struct StructLitField {
    /// Field name.
    pub name: String,
    /// Field value. `None` indicates field-shorthand `Point { x, y }` —
    /// resolved to the local binding of the same name at typeck/eval.
    pub value: Option<Expr>,
    /// Span covering `name: value` (or just `name` for shorthand).
    pub span: Span,
}
```

### Validation rules

- **VR-11**: `path.len() == 1` (multi-segment paths typeck-rejected — "M07.4 supports single-segment struct paths only").
- **VR-12**: Typeck verifies every declared field of the struct schema appears exactly once in `fields`; extras error; missing error.
- **VR-13**: For shorthand fields (`value: None`), typeck looks up a local binding of `name` in the current scope. Missing → "no local named `<name>` for field-shorthand".
- **VR-14**: Trailing comma allowed in source (`Point { x: 1, y: 2, }`).

## New (AST): `Expr::FieldAccess`

```rust
pub enum Expr {
    // ... existing variants
    /// **M07.4**: field access `receiver.name`. Postfix; `name` is an
    /// identifier NOT followed by `(` (which would be a `MethodCall`).
    FieldAccess {
        /// Receiver expression.
        receiver: Box<Expr>,
        /// Field name.
        name: String,
        /// Span from receiver start through `name`.
        span: Span,
    },
}
```

### Validation rules

- **VR-15**: Typeck accepts receiver of type `Ty::Struct(_)` OR `Ty::Ref { inner: Ty::Struct(_), .. }` (auto-deref per R-017).
- **VR-16**: Field name must exist in the resolved struct's schema; "no field `<name>` on struct `<ty>`" if not.
- **VR-17**: Multi-level field access (`p.x.y`) typeck-rejects in M07.4 — "nested struct fields not supported; use intermediate let bindings". (Parser accepts the chain via left-associativity; typeck rejects.)

## Modified: `Ty` — adds `Struct`

```rust
pub enum Ty {
    // ... existing variants
    /// **M07.4**: nominal struct type. Equal iff the names match (fields
    /// carried for convenience but not part of identity).
    Struct {
        /// Type name (`"Point"`).
        name: String,
        /// Fields in declaration order (matches `StructDecl.fields`).
        fields: Vec<(String, Ty)>,
    },
}
```

### Validation rules

- **VR-18**: `Ty::name()` returns the bare `name` (`"Point"`).
- **VR-19**: `Ty::is_copy()` returns true in M07.4 (every field is a primitive — all Copy). Future milestones with non-Copy fields refine to "Copy iff every field is Copy".
- **VR-20**: Two `Ty::Struct` are equal iff their `name` fields match. The `fields` vectors should match too in a well-formed program, but equality is by name (nominal typing).

## Modified: `Value` — adds `Struct`

```rust
pub enum Value {
    // ... existing variants
    /// **M07.4**: struct value — N fields held inline in the binding's
    /// stack slot. No heap allocation. Field order matches the type's
    /// declaration order. Cloning deep-copies each field.
    Struct {
        /// Struct type name (`"Point"`).
        name: String,
        /// Field values in declaration order.
        fields: Vec<(String, Value)>,
    },
}
```

### Validation rules

- **VR-21**: `Value::Struct.fields.len()` matches the binding's `Ty::Struct.fields.len()`. Set at construction; never resized (struct shape is fixed).
- **VR-22**: `Value::Struct.fields[i].0` matches the declared field name at index i.
- **VR-23**: `Value::type_name()` returns `"{}"` (short tag).
- **VR-24**: Cloning a `Value::Struct` deep-copies each field's `Value`. Used for the Copy-style assignment `let p2 = p` (M07.4 structs are Copy).

## Modified: `Value::Ref` — adds `field_path: Vec<String>`

```rust
pub enum Value {
    // ... existing variants
    Ref {
        borrow_id: BorrowId,
        target: Pointee,
        mutable: bool,
        /// **M07.4**: navigation path into a sub-field of the target. Empty
        /// when the ref points at the whole binding (pre-M07.4 semantics);
        /// non-empty when the ref points at a struct sub-field via `&p.x`.
        /// Single-segment in M07.4 (multi-level `&p.x.y` out of scope).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        field_path: Vec<String>,
    },
}
```

### Validation rules

- **VR-25**: When `field_path` is non-empty, `target` MUST be `Pointee::Slot(_)` AND the slot's value MUST be `Value::Struct` at eval time. Multi-level paths and non-struct targets rejected.
- **VR-26**: M07.4 always produces single-segment `field_path` (`Vec` shape kept for future nested-struct support).
- **VR-27**: `render_value` produces `"&p.x"` when `field_path = ["x"]` and target slot's binding is `p` — versus `"&p"` when `field_path` is empty.
- **VR-28**: Existing M06/M07/M07.1/M07.2/M07.3 snapshots stay byte-identical because `Vec::is_empty` skips serialization when there's no field path.

## Modified: `SlotRowView` — adds `struct_view`

```rust
// In src/ui.rs

pub struct SlotRowView {
    pub slot_id: u32,
    pub name: String,
    pub ty: String,
    pub value: Option<String>,
    pub inline_cells: Option<InlineCellsView>,    // M07.3
    /// **M07.4**: present when the slot holds a `Value::Struct`. The JS
    /// renders per-field byte-cell strips with field-name labels (per
    /// research R-016 Proposal A). Mutually exclusive with both `value`
    /// (which holds plain values) and `inline_cells` (which holds arrays).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub struct_view: Option<StructView>,
}

pub struct StructView {
    /// Struct type name (`"Point"`).
    pub name: String,
    /// Per-field render data in declaration order.
    pub fields: Vec<StructFieldView>,
}

pub struct StructFieldView {
    /// Field name (`"x"`).
    pub name: String,
    /// Field type label (`"i32"`).
    pub ty_label: String,
    /// Byte size of this field (drives the byte-cell count).
    pub size: u32,
    /// Rendered field value (`"1_i32"`).
    pub display: String,
}
```

### Validation rules

- **VR-29**: `struct_view` is `Some(_)` iff the slot's `Value` is `Value::Struct`. Mutually exclusive with `value: Some(_)` and `inline_cells: Some(_)`.
- **VR-30**: `StructView.fields.len() == Value::Struct.fields.len()` AND order matches.
- **VR-31**: `StructFieldView.size == ty_size_bytes_ui(field_value)` (per-field byte size).
- **VR-32**: `#[serde(skip_serializing_if = "Option::is_none")]` keeps existing slot-row JSON unchanged for non-struct slots.

## Modified: `ArrowView` — adds `field_label`

```rust
pub struct ArrowView {
    pub source_slot: u32,
    pub target: ArrowTarget,
    pub mutable: bool,
    pub slice_len: Option<u64>,                    // M07.1
    pub slice_byte_offset: Option<u64>,            // M07.1
    pub slice_byte_len: Option<u64>,               // M07.1
    pub slice_elem_start: Option<u64>,             // M07.1
    /// **M07.4**: present when the arrow is a field-borrow arrow
    /// (`&p.x`). Drives the small `.x` label rendered next to the
    /// arrow midpoint, analogous to slice arrows' `[len: N]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_label: Option<String>,
}
```

### Validation rules

- **VR-33**: `field_label` is `Some(".x")` when the underlying borrow's `Value::Ref.field_path` is `["x"]`; `None` otherwise.
- **VR-34**: When `field_label` is set, hover on the arrow highlights the matching field's row in the target slot's `.struct-view` (via `[data-slot-id=X] .struct-field[data-field-name="x"]`).

## Typeck-side registries (not serialized)

```rust
// In src/typeck.rs — built during phase 1; consumed during phase 2.

#[derive(Default)]
pub struct StructRegistry {
    /// Maps struct name → field schema (name + type per field).
    schemas: IndexMap<String, Vec<(String, Ty)>>,
}

#[derive(Default)]
pub struct ImplRegistry {
    /// Maps `(struct_name, method_name) → FnSig`. Self-receiver is
    /// REMOVED from the params (the type-system records it separately).
    methods: IndexMap<(String, String), FnSig>,
    /// Maps fully-qualified path → FnSig. `Point::new` → entry with
    /// key `vec!["Point", "new"]`.
    assoc_fns: IndexMap<Vec<String>, FnSig>,
}
```

### Validation rules

- **VR-35**: `StructRegistry` populated in phase 1 with one entry per `Item::Struct`. Duplicate struct names rejected ("struct `<name>` already defined at <span>").
- **VR-36**: `ImplRegistry` populated in phase 1 with one entry per fn item per impl block. Duplicate `(struct_name, method_name)` rejected ("method `<name>` already defined on `<struct>` at <span>"). Same for assoc fns.
- **VR-37**: Phase 2 typecheck of method/path calls consults the registries AFTER the M07 hardcoded built-ins (R-018 tie-breaker).

## New: M07.4 reference samples

| File | Content | Pedagogy |
|---|---|---|
| `tests/samples/m07_4_struct_basic.rs` | `struct Point { x: i32, y: i32 } fn main() { let p = Point { x: 1, y: 2 }; let a = p.x; }` | Struct decl + literal + field access; p's slot renders two field rows with byte-cells; zero heap events. |
| `web/samples/m07_4_struct_basic.rs` | Mirror. | |
| `tests/samples/m07_4_field_borrow.rs` | `struct Point { x: i32, y: i32 } fn main() { let p = Point { x: 1, y: 2 }; let r = &p.x; }` | Field borrow produces `Value::Ref { field_path: ["x"], target: Pointee::Slot(p_slot), .. }`; blue arrow from r to p with `.x` annotation; hover lights up just the x row. |
| `web/samples/m07_4_field_borrow.rs` | Mirror. | |
| `tests/samples/m07_4_method.rs` | `struct Point { x: i32, y: i32 } impl Point { fn x(&self) -> i32 { self.x } } fn main() { let p = Point { x: 1, y: 2 }; let v = p.x(); }` | Method call enters new frame for x; self bound to &p; returns 1; v = 1_i32. |
| `web/samples/m07_4_method.rs` | Mirror. | |
| `tests/samples/m07_4_associated_fn.rs` | `struct Point { x: i32, y: i32 } impl Point { fn new(x: i32, y: i32) -> Point { Point { x, y } } } fn main() { let p = Point::new(1, 2); }` | Associated fn via path call; new frame for new; constructs and returns the struct; p gets the struct. |
| `web/samples/m07_4_associated_fn.rs` | Mirror. | |
