# Data Model — M07.1 Entities

Slice-focused data-model expansion: 1 new Token (`DotDot`), 1 new AST expression (`Expr::Range`), 1 new AST type (`Type::Slice`), 1 new Ty variant (`Slice(Box<Ty>)`), 1 new Value variant (`Slice { borrow_id, target, len, mutable }`), ArrowView extension (`len: Option<u64>`).

All additive. No restructure of any existing variant.

## New (Token): `DotDot`

```rust
// In src/parse/token.rs

pub enum TokenKind {
    // ... existing variants
    /// **M07.1**: `..` — range operator. Only legal inside `[ ]` brackets in M07.1.
    DotDot,
}
```

### Validation rules

- **VR-1**: `DotDot` lexes as one token from two consecutive `.` chars (two-char lookahead).
- **VR-2**: The lexer emits `DotDot` unconditionally; the parser enforces the "only inside `[ ]`" rule.
- **VR-3**: `Float(1.0)` followed by `DotDot` followed by `Float(3.0)` parses correctly for `1.0..3.0`. The float arm's `digit.digit` requirement prevents misreading `..` as a float fragment.

## New (AST): `Expr::Range`

```rust
// In src/parse/ast.rs

pub enum Expr {
    // ... existing variants
    /// **M07.1**: range expression `a..b`, `..b`, `a..`, `..`. All four
    /// forms are represented by Option-ing the bounds. Standalone Range
    /// expressions parse but typeck rejects them (only valid inside
    /// `Expr::Index.index` in M07.1).
    Range {
        /// Start bound (inclusive). `None` defaults to 0 at eval time.
        start: Option<Box<Expr>>,
        /// End bound (exclusive). `None` defaults to receiver length at eval time.
        end: Option<Box<Expr>>,
        /// Span covering the whole range, including any bounds.
        span: Span,
    },
}
```

### Validation rules

- **VR-4**: `Expr::Range` parses only inside `Expr::Index.index` position in M07.1 (parser only accepts `..` between `[` and `]`).
- **VR-5**: `Expr::span()` extends to handle Range — `Range { span, .. } => *span`.
- **VR-6**: Both bounds typecheck to `Ty::Int(_)` (any integer kind). Other types are typeck errors.

## New (AST type): `Type::Slice`

```rust
// In src/parse/ast.rs

pub enum Type {
    // ... existing variants (Path, Unit, Ref, Generic)
    /// **M07.1**: slice type `&[T]` or `&mut [T]`. The leading `&` is
    /// absorbed into the slice type; this variant represents the whole
    /// `&[T]` syntactic form.
    Slice {
        /// Element type.
        inner: Box<Type>,
        /// `true` for `&mut [T]`, `false` for `&[T]`. **M07.1**: typeck
        /// rejects `mutable: true` with an out-of-scope message.
        mutable: bool,
        /// Span from `&` through `]`.
        span: Span,
    },
}
```

### Validation rules

- **VR-7**: `Type::Slice` lowers to `Ty::Slice(inner_ty)` at typeck (the `mutable: bool` is dropped — M07.1 only supports immutable slices, and typeck errors if `mutable: true`).
- **VR-8**: `Type::Slice` is parser-recognized when `parse_type` sees `&` or `&mut` followed by `[`.

## Modified: `Ty` — adds `Slice`

```rust
pub enum Ty {
    // ... existing variants (Int, Float, Bool, Unit, Ref, Box, Vec, String)
    /// **M07.1**: slice type `&[T]`. Always shared (immutable) in M07.1.
    /// Carries the element type. The fat-pointer nature (data ptr + len)
    /// is represented in the corresponding `Value::Slice` variant; the Ty
    /// only captures the element type, matching Rust's `[T]` unsized type
    /// (which only ever appears behind a reference).
    Slice(Box<Ty>),
}
```

### Validation rules

- **VR-9**: `Ty::Slice(_)` is non-Copy. `Ty::is_copy()` returns `false`.
- **VR-10**: `Ty::name()` renders as `"&[<inner_name>]"` (e.g. `"&[i32]"`). Always with the leading `&` since the slice type is always reference-shaped in M07.1.
- **VR-11**: Two `Ty::Slice` are equal iff their element types are equal.

## Modified: `Value` — adds `Slice`

```rust
pub enum Value {
    // ... existing variants (Int, Float, Bool, Unit, Ref, Box, Vec, String, Str)
    /// **M07.1**: slice value — a fat pointer (target + length) into a
    /// heap allocation. Shares the borrow registry with `Value::Ref`: a
    /// `Slice` borrow shows up in `world.borrows` and the existing
    /// dangling-detection scan catches it on later realloc.
    Slice {
        /// Identifier of the active borrow.
        borrow_id: BorrowId,
        /// What's being sliced — a `Pointee::Heap(addr)` for Vec slices.
        /// `Pointee::Slot(_)` is unreachable in M07.1 (no array-on-stack).
        target: Pointee,
        /// Length of the slice (number of elements visible through it).
        len: u64,
        /// `true` for `&mut [T]`, `false` for `&[T]`. **M07.1**: always
        /// `false`. The field is here for forward-compat with future
        /// mutable-slice support.
        mutable: bool,
    },
}
```

### Validation rules

- **VR-12**: `Value::Slice.target` corresponds to a live heap allocation (Pointee::Heap pointing at a non-freed HeapAddr at construction time; may go stale via realloc, triggering RuntimeError note).
- **VR-13**: `Value::Slice.borrow_id` corresponds to a `BorrowShared` event earlier in the trace (no `BorrowEnd` since).
- **VR-14**: `Value::Slice.len` is the number of elements in the slice (end - start at construction time). Invariant at value level — doesn't change after construction (Rust slices are immutable views).
- **VR-15**: `Value::Slice.mutable` is always `false` in M07.1 (typeck rejects mutable-slice construction).
- **VR-16**: `Value::type_name()` returns `"&[T]"`-style string (delegates to the Ty layer; the Value layer just returns a short tag `"&[]"`).

## Modified: `ArrowView` — adds `len: Option<u64>`

```rust
// In src/ui.rs

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArrowView {
    pub source_slot: u32,
    pub target: ArrowTarget,
    pub kind: ArrowKind,
    /// **M07.1**: optional length annotation for slice arrows.
    /// `None` for non-slice borrows and owning arrows. `Some(n)` when the
    /// arrow originates from a `Value::Slice { len: n, .. }`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub len: Option<u64>,
}
```

### Validation rules

- **VR-17**: `ArrowView.len` is `Some(_)` iff the source slot holds a `Value::Slice`.
- **VR-18**: `ArrowView.kind` for slice arrows is always `Shared` in M07.1 (no mutable slices).
- **VR-19**: `ArrowView.target` for slice arrows is always `ArrowTarget::Heap(_)` in M07.1 (slices only target Vec's heap allocation).
- **VR-20**: `#[serde(default, skip_serializing_if = "Option::is_none")]` keeps the wire format backwards-compatible — non-slice arrows omit the field.

## New: M07.1 reference samples

| File | Content | Pedagogy |
|---|---|---|
| `tests/samples/m07_1_slice_basic.rs` | `fn main() { let mut v: Vec<i32> = Vec::new(); v.push(1); v.push(2); v.push(3); let s = &v[..]; let n = s.len(); }` | Full-vec slice; `[len: 3]` annotation on the blue arrow; `s.len()` returns 3_u64. |
| `web/samples/m07_1_slice_basic.rs` | Mirror. | |
| `tests/samples/m07_1_slice_range.rs` | `fn main() { let mut v: Vec<i32> = Vec::new(); v.push(10); v.push(20); v.push(30); v.push(40); let s = &v[1..3]; }` | Partial-range slice; `[len: 2]` annotation; slice covers Vec elements 1 and 2 (values 20 and 30). |
| `web/samples/m07_1_slice_range.rs` | Mirror. | |
| `tests/samples/m07_1_slice_dangling.rs` | `fn main() { let mut v: Vec<i32> = Vec::new(); v.push(1); v.push(2); let s = &v[..]; v.push(3); }` | Slice into a Vec that reallocates; same RuntimeError pedagogy as M07's `&v[0]` case but for slice granularity. |
| `web/samples/m07_1_slice_dangling.rs` | Mirror. | |
