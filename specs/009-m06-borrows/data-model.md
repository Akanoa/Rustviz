# Data Model — M06 Entities

M06 adds two new enum variants (`Ty::Ref`, `Value::Ref`), one new view type (`BorrowView` in `StateSnapshot`), one new AST node (`Expr::Borrow`), one new Type AST node (`Type::Ref`), two new lexer tokens (`Amp`, `AmpMut`), and an inline borrow-tracker module in typeck.

## Modified: `Ty` — adds `Ref` variant; drops `Copy` derive

```rust
// In src/typeck.rs

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
//      ^^^^^ no longer Copy (Box<Ty> isn't Copy)
pub enum Ty {
    Int(IntKind),
    Float(FloatKind),
    Bool,
    Unit,
    /// **M06**: reference type. Inner is the referent's type; `mutable` is
    /// `true` for `&mut T`, `false` for `&T`. Box because Ty is recursive.
    Ref { inner: Box<Ty>, mutable: bool },
}

impl Ty {
    pub fn name(&self) -> String { /* "&i32", "&mut u8", etc.; allocates */ }
    pub fn is_copy(&self) -> bool { /* `&T` and `&mut T` are themselves Copy
                                       in Rust — references implement Copy
                                       when their lifetime allows, simplified
                                       here to: shared refs are Copy, mut
                                       refs are not. Plan-phase confirms. */ }
}
```

### Validation rules

- **VR-1**: `Ty::Ref { inner, mutable }`'s `inner` must be a non-`Ref` type for now (no nested refs in L2). Plan-phase confirms; typeck enforces.
- **VR-2**: `Ty::is_copy()` returns `true` for `Ref { mutable: false, .. }` (shared refs implement Copy in Rust), `false` for `Ref { mutable: true, .. }` (mut refs are not Copy). Eval respects this for the existing copy-vs-move distinction (relevant once mut refs are passed to functions).
- **VR-3**: Dropping the `Copy` derive on `Ty` cascades to every method previously `(self) -> ...` — these become `(&self) -> ...`. Audit: ~50 sites. Mechanical.

## Modified: `Value` — adds `Ref` variant

```rust
// In src/event.rs

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Value {
    Int { kind, bits },
    Float { kind, value },
    Bool(bool),
    Unit,
    /// **M06**: a borrow value held in a stack slot. `borrow_id` matches the
    /// `BorrowShared.borrow_id` or `BorrowMut.borrow_id` that created it.
    /// `target_slot` denormalizes the borrow's target (also recoverable from
    /// the event stream) so the StateSnapshot is self-contained.
    Ref {
        borrow_id: BorrowId,
        target_slot: SlotId,
        mutable: bool,
    },
}
```

### Validation rules

- **VR-4**: `Value::Ref.borrow_id` corresponds to a `BorrowShared`/`BorrowMut` event in the trace.
- **VR-5**: `Value::Ref.target_slot` corresponds to a slot allocated by a prior `SlotAlloc` event.
- **VR-6**: `Value::Ref.mutable` matches the kind of `BorrowShared` (false) vs `BorrowMut` (true).
- **VR-7**: `Value` continues to derive `Clone, Debug, PartialEq` only — no `Eq` (carried from M03.2).

## New (AST): `Expr::Borrow`

```rust
// In src/parse/ast.rs

pub enum Expr {
    // ... existing variants
    /// **M06**: `&place` or `&mut place`. The `inner` must be a place expression.
    Borrow {
        inner: Box<Expr>,
        mutable: bool,
        span: Span,
    },
}
```

### Validation rules

- **VR-8**: `Expr::Borrow.inner` must be a place expression — for L2, that means `Expr::Ident(_, _)`. Anything else is a typeck error: "expected place expression for borrow."
- **VR-9**: `Expr::Borrow.span` covers the whole borrow including the `&` or `&mut` prefix and the inner expression.

## New (AST): `Type::Ref`

```rust
// In src/parse/ast.rs

pub enum Type {
    // ... existing variants
    /// **M06**: `&T` or `&mut T`.
    Ref {
        inner: Box<Type>,
        mutable: bool,
        span: Span,
    },
}
```

### Validation rules

- **VR-10**: `Type::Ref.inner` resolves via `ty_from_ast` to any `Ty` permitted by L2 (which for M06 means non-Ref leaf types: `Int`, `Float`, `Bool`, `Unit`). Nested refs (`& &i32`) are rejected at typeck for M06.

## New (Token): `Amp` and `AmpMut`

```rust
// In src/parse/token.rs

pub enum TokenKind {
    // ... existing variants
    /// **M06**: `&` (single).
    Amp,
    /// **M06**: `&mut` (with no intervening whitespace).
    AmpMut,
}
```

### Validation rules

- **VR-11**: Lexer produces `AmpMut` when `&` is immediately followed by `mut` (no whitespace, `mut` as a complete identifier). Produces `Amp` otherwise. `& mut x` (with space) lexes as `Amp + Ident("mut") + Ident("x")` — parser then errors.

## New: `BorrowTracker` (private to typeck)

```rust
// In src/typeck.rs (inline mod)

mod borrow_tracker {
    use crate::parse::span::Span;
    use crate::resolve::BindingId;
    use indexmap::IndexMap;

    pub struct BorrowTracker {
        active: IndexMap<BindingId, Vec<ActiveBorrow>>,
    }

    pub struct ActiveBorrow {
        kind: BorrowKind,
        scope_depth: u32,
        borrow_span: Span,
    }

    pub enum BorrowKind { Shared, Mut }

    pub struct AliasConflict {
        existing_kind: BorrowKind,
        existing_span: Span,
    }

    impl BorrowTracker {
        pub fn new() -> Self;
        pub fn try_take_shared(&mut self, b: BindingId, depth: u32, span: Span) -> Result<(), AliasConflict>;
        pub fn try_take_mut(&mut self, b: BindingId, depth: u32, span: Span) -> Result<(), AliasConflict>;
        pub fn pop_scope(&mut self, leaving_depth: u32);
    }
}
```

### Validation rules

- **VR-12**: `try_take_shared(b)` succeeds if all `active[b]` entries are `BorrowKind::Shared`. Fails on any `Mut` entry, returning the most recent `Mut`'s span.
- **VR-13**: `try_take_mut(b)` succeeds only if `active[b]` is empty. Fails on any existing entry, returning the most recent's span + kind.
- **VR-14**: `pop_scope(depth)` removes all entries with `scope_depth >= depth`. Idempotent if called twice with the same depth (second call removes nothing).

## New: `BorrowView` (in `StateSnapshot`)

```rust
// In src/ui.rs

pub struct StateSnapshot {
    // ... existing fields
    /// **M06**: active borrows at this cursor position. The JS renderer reads
    /// this to draw the SVG arrow overlay.
    pub borrows: Vec<BorrowView>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BorrowView {
    /// The slot holding the reference value (the source of the arrow).
    pub source_slot: u32,
    /// The slot being borrowed (the target of the arrow).
    pub target_slot: u32,
    /// `true` for `&mut` (red arrow), `false` for `&` (blue arrow).
    pub mutable: bool,
}
```

### Validation rules

- **VR-15**: `StateSnapshot.borrows` reflects only currently-active borrows at the cursor position. A borrow appears in the vec from its `BorrowShared`/`BorrowMut` event up to (and not including) its `BorrowEnd`.
- **VR-16**: `BorrowView` order in the vec is allocation-order (oldest first). The renderer may sort visually if needed.

## Internal: `World.borrows` (in `src/ui.rs::Cursor`)

The cursor's internal world model tracks active borrows analogously to active frames + slots:

```rust
struct World {
    frames: Vec<FrameInProgress>,
    borrows: Vec<ActiveBorrowState>,
}

struct ActiveBorrowState {
    borrow_id: u32,
    source_slot: u32,
    target_slot: u32,
    mutable: bool,
}
```

`apply_event` for `BorrowShared`/`BorrowMut` pushes; for `BorrowEnd` removes by `borrow_id`. The snapshot derives `Vec<BorrowView>` from this.

## New: M06 reference samples

| File                              | Notes                                                        |
|-----------------------------------|--------------------------------------------------------------|
| `tests/samples/m06_shared_borrow.rs` | `fn main() { let x = 5; let r = &x; }` |
| `web/samples/m06_shared_borrow.rs`   | Mirror.                                                  |
| `tests/samples/m06_mut_borrow.rs`    | `fn main() { let mut x = 5; let r = &mut x; }` |
| `web/samples/m06_mut_borrow.rs`      | Mirror.                                                  |
| `tests/samples/m06_aliasing_error.rs`| `fn main() { let mut x = 5; let r1 = &x; let r2 = &mut x; }` (deliberate typeck error) |
| `web/samples/m06_aliasing_error.rs`  | Mirror.                                                  |
| `tests/samples/m06_scoped_borrow.rs` | `fn main() { let x = 5; { let r = &x; } }` — BorrowEnd at inner `}`. |
| `web/samples/m06_scoped_borrow.rs`   | Mirror.                                                  |
