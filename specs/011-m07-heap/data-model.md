# Data Model — M07 Entities

Significant data-model expansion: 5 new tokens, 4 new AST expression nodes (+ 1 type variant), 3 new Ty variants, 4 new Value variants + 1 Value restructure, new HeapState eval-side, ArrowView extension.

## New (Token): five variants

```rust
// In src/parse/token.rs

pub enum TokenKind {
    // ... existing variants
    /// **M07**: String literal contents (escapes already processed).
    Str(String),
    /// **M07**: `::`
    ColonColon,
    /// **M07**: `.` (postfix dot for method calls; floats consume `.digit` greedily before this).
    Dot,
    /// **M07**: `[`
    LBracket,
    /// **M07**: `]`
    RBracket,
}
```

### Validation rules

- **VR-1**: `Str` literals have escapes (`\n`, `\t`, `\\`, `\"`) processed at lex time. Invalid escape sequences are parse errors.
- **VR-2**: `::` lexes as one `ColonColon` token, NOT two `Colon` tokens.
- **VR-3**: Postfix `.` only appears AFTER the float-literal lexer has consumed its greedy `digits.digits`. Bare `.digit` doesn't conflict with float literals.

## New (AST): four expression nodes

```rust
// In src/parse/ast.rs

pub enum Expr {
    // ... existing variants
    /// **M07**: string literal `"..."`. Used as an argument to
    /// `String::from(...)` and `String::push_str(...)`.
    StrLit(String, Span),
    /// **M07**: multi-segment path `Vec::new`, `Box::new`, `String::from`.
    /// Single-segment idents stay as `Expr::Ident`.
    Path {
        segments: Vec<String>,
        span: Span,
    },
    /// **M07**: method call `receiver.method(args)`.
    MethodCall {
        receiver: Box<Expr>,
        name: String,
        args: Vec<Expr>,
        span: Span,
    },
    /// **M07**: indexing `receiver[index]`. Rvalue-only in M07
    /// (no `v[0] = ...` assignment).
    Index {
        receiver: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
}
```

### Validation rules

- **VR-4**: `Expr::Path.segments` has ≥ 2 elements (single-segment paths stay as `Expr::Ident`).
- **VR-5**: `Expr::MethodCall.name` is a single identifier (no path methods like `Vec::<i32>::push` — out of scope).
- **VR-6**: `Expr::Index` is rvalue-only. Place-expression check at typeck rejects `Index` in assignment lhs position.
- **VR-7**: `Expr::span()` extends to include the four new variants — `(StrLit, _, s) | Path { span: s, .. } | MethodCall { span: s, .. } | Index { span: s, .. } => *s`.

## New (AST type): `Type::Generic`

```rust
pub enum Type {
    // ... existing variants (Path, Unit, Ref)
    /// **M07**: generic type path `Vec<i32>`, `Box<i32>`. Multi-segment
    /// paths with type arguments.
    Generic {
        segments: Vec<String>,
        args: Vec<Type>,
        span: Span,
    },
}
```

### Validation rules

- **VR-8**: `Type::Generic.segments` describes a Box/Vec name (M07 recognizes `Box`, `Vec`); other generics are typeck-rejected.
- **VR-9**: `Type::Generic.args.len()` must match the expected arity (Box and Vec take 1 arg each). M07 rejects `Vec<i32, i64>` etc.
- **VR-10**: `String` parses as `Type::Path { segments: vec!["String"], span }` — no generics needed.

## Modified: `Ty` — adds Box, Vec, String

```rust
pub enum Ty {
    // ... existing variants
    Box(Box<Ty>),
    Vec(Box<Ty>),
    String,
}
```

### Validation rules

- **VR-11**: All three are non-Copy. `Ty::is_copy()` returns `false`.
- **VR-12**: `Ty::name()` renders as `"Box<i32>"`, `"Vec<i32>"`, `"String"`.

## Modified: `Value` — 4 additions + Value::Ref restructure

```rust
pub enum Value {
    // ... existing variants (Int, Float, Bool, Unit)
    /// **M07**: owns a Box-allocated value.
    Box { addr: HeapAddr },
    /// **M07**: owns a Vec allocation.
    Vec { addr: HeapAddr },
    /// **M07**: owns a String allocation.
    String { addr: HeapAddr },
    /// **M07**: transient value for string literals consumed by string
    /// methods. Not stored in slots.
    Str(String),
    /// **M06.1 (restructured in M07)**: borrow value. Target was
    /// `target_slot: SlotId`; now uses `Pointee` to support both Slot and
    /// Heap borrow targets.
    Ref {
        borrow_id: BorrowId,
        target: Pointee,         // was: target_slot: SlotId
        mutable: bool,
    },
}
```

### Validation rules

- **VR-13**: `Value::Box/Vec/String.addr` corresponds to a `HeapAlloc` event earlier in the trace and no `HeapFree` event since.
- **VR-14**: `Value::Ref.target` of `Pointee::Slot(slot_id)` corresponds to an active SlotAlloc; of `Pointee::Heap(heap_addr)` corresponds to an active HeapAlloc.
- **VR-15**: `Value::Str` never appears in a SlotWrite event (transient only).

## New: HeapState (Evaluator side)

```rust
// In src/eval.rs (private)

struct HeapState {
    next_addr: u32,
    objects: IndexMap<HeapAddr, HeapObject>,
}

enum HeapObject {
    Box(Value),
    Vec {
        elements: Vec<Value>,
        capacity: usize,
        elem_ty: Ty,
    },
    Str {
        bytes: String,
        capacity: usize,
    },
}
```

### Validation rules

- **VR-16**: `HeapState.next_addr` is monotonic. Realloc gets a fresh `to` addr; the `from` addr is removed from `objects` and never reused.
- **VR-17**: `HeapObject::Vec.elements.len() <= capacity`. Push that would exceed triggers realloc (capacity doubles).
- **VR-18**: `HeapObject::Vec.elem_ty` matches every element's runtime Value type. Required for typeck-eval consistency.
- **VR-19**: `HeapObject::Str.bytes.len() <= capacity`. Push that would exceed triggers realloc.

## New: ArrowView (renames BorrowView from M06)

```rust
// In src/ui.rs

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArrowView {
    pub source_slot: u32,
    pub target: ArrowTarget,
    pub kind: ArrowKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ArrowTarget {
    Slot(u32),
    Heap(u32),  // u32 = HeapAddr.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ArrowKind {
    Shared,   // blue
    Mut,      // red
    Owning,   // black
}
```

### Validation rules

- **VR-20**: `ArrowView.source_slot` is always a stack slot (SlotId) — owning relationships and borrows both originate from a binding.
- **VR-21**: `ArrowView.kind` determines the color and arrowhead marker.
- **VR-22**: For each `Value::Box/Vec/String { addr }` held in a slot, exactly one `ArrowView { kind: Owning, target: Heap(addr) }` is present.

## New: HeapView (in `StateSnapshot`)

```rust
// In src/ui.rs

pub struct StateSnapshot {
    // ... existing fields
    /// **M07**: live heap allocations at this cursor position.
    pub heap: Vec<HeapView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeapView {
    pub addr: u32,
    pub ty_name: String,
    /// Rendered contents, e.g. `"5_i32"`, `"[1, 2, 3]"`, `"\"hi\""`.
    pub display: String,
    /// Size in bytes (renderer scales the box's width by this).
    pub size: u32,
}
```

### Validation rules

- **VR-23**: `HeapView` entries appear in `addr` order; the renderer determines visual layout (flexbox auto-wraps).
- **VR-24**: `HeapView.display` is renderer-ready (already includes type-tag suffixes for primitives).

## New: M07 reference samples

| File | Content |
|---|---|
| `tests/samples/m07_box.rs` | `fn main() { let b = Box::new(5); }` |
| `web/samples/m07_box.rs` | Mirror. |
| `tests/samples/m07_vec_realloc.rs` | The headline demo: `let mut v: Vec<i32> = Vec::new(); v.push(1); v.push(2); let r = &v[0]; v.push(3);` |
| `web/samples/m07_vec_realloc.rs` | Mirror. |
| `tests/samples/m07_string.rs` | `let mut s = String::from("hi"); s.push_str("!");` |
| `web/samples/m07_string.rs` | Mirror. |
