# Data Model — M06.1 Entities

M06.1 adds two AST nodes (`Expr::Deref`, `Stmt::Assign`). No new types, no new `MemEvent` variants, no `Ty`/`Value` extensions, no public crate API additions.

## New (AST): `Expr::Deref`

```rust
// In src/parse/ast.rs

pub enum Expr {
    // ... existing variants
    /// **M06.1**: `*expr` — read through a reference. The `inner` must
    /// typecheck as `Ty::Ref { inner, .. }`; the deref produces a value
    /// of the referenced type.
    Deref {
        /// The expression being dereferenced (a reference value).
        inner: Box<Expr>,
        /// Span from `*` through end of inner.
        span: Span,
    },
}
```

### Validation rules

- **VR-1**: typeck requires `inner`'s type to be `Ty::Ref { .. }`. Dereferencing a non-reference is a typeck error.
- **VR-2**: `Expr::Deref(Expr::Ident(_))` is a *place expression* (usable as the lhs of `Stmt::Assign`). Other `Expr::Deref` shapes (e.g. `*(f())`) are NOT place expressions for M06.1.
- **VR-3**: `Expr::Deref.span` covers `*` plus the inner expression.

## New (AST): `Stmt::Assign`

```rust
// In src/parse/ast.rs

pub enum Stmt {
    // ... existing variants (Let, Expr)
    /// **M06.1**: `lhs = rhs;` — assignment statement. The `lhs` must be a
    /// place expression (per VR-4); the assignment emits a `SlotWrite`
    /// event for the resolved target slot.
    Assign {
        /// Left side — a place expression.
        lhs: Expr,
        /// Right side — value expression matching the lhs's type.
        rhs: Expr,
        /// Span from start of `lhs` through `;`.
        span: Span,
    },
}
```

### Validation rules

- **VR-4**: `Stmt::Assign.lhs` must be one of: `Expr::Ident(_, _)` (direct assignment) or `Expr::Deref(Expr::Ident(_, _))` (through-reference assignment). Other lhs shapes are typeck errors.
- **VR-5**: When lhs is `Expr::Ident(x)`, `x` must be a `BindingKind::Let { mutable: true, .. }` binding. Otherwise typeck error: "cannot assign to immutable variable `x`".
- **VR-6**: When lhs is `Expr::Deref(Expr::Ident(r))`, `r`'s type must be `Ty::Ref { mutable: true, .. }`. Otherwise typeck error: "cannot assign through `&T`; need `&mut T`".
- **VR-7**: When lhs is `Expr::Ident(x)`, the M06 borrow tracker must have no active borrows of `x`. Otherwise typeck error: "cannot assign to `x` because it is borrowed".
- **VR-8**: `lhs` and `rhs` must typecheck to the same `Ty` (with M03.2 literal coercion applied to the rhs against the lhs's type).
- **VR-9**: `Stmt::Assign` typechecks as `Ty::Unit` (so the statement contributes nothing to the enclosing block's tail value — but it never IS a tail value because it's a statement).

## Place-expression set (extended)

For M06.1, place expressions are:

- `Expr::Ident(name, span)` — direct binding access (same as M06's place set).
- `Expr::Deref(Expr::Ident(name, span), _span)` — through-reference. **NEW** in M06.1.

For comparison, M06 only allowed `Expr::Ident` as a place (for `&place` and `&mut place`). M06.1 extends the set for `Stmt::Assign.lhs`. Note that `&` still only accepts `Expr::Ident` — borrowing a deref (`&*r`) is explicitly out of scope.

## `MemEvent::SlotWrite` semantics (clarified, not changed)

```rust
// In src/event.rs — UNCHANGED from M03

SlotWrite { slot_id, value, span }
```

**M06.1 starts emitting `SlotWrite` for re-assignment**, not just initial write. The variant's payload and semantics are unchanged — the only thing changing is the set of source positions where it's emitted from. Before M06.1: from `let x = init;` statements. After M06.1: also from `x = v;` and `*r = v;` statements.

### Validation rules (unchanged from M03)

- **VR-10**: `SlotWrite.slot_id` corresponds to a previously-allocated slot (`SlotAlloc` event earlier in the trace).
- **VR-11**: `SlotWrite.value` is the new value stored in the slot.
- **VR-12**: `SlotWrite.span` is a source span. For let-init writes (M03), it's the binding's decl span. **M06.1 extends**: for assignment writes, it's the whole assignment-statement span (lhs through `;`).

## In-memory eval state (internal)

The `Evaluator::frames[i].scopes[j].locals[k]` struct (existing) holds a `value: Value` field. M06.1 mutates this in-place when an assignment evaluates. New helper:

```rust
// In src/eval.rs

/// **M06.1**: write `value` to the slot with `slot_id`, anywhere in the
/// call stack. Used by `Stmt::Assign` evaluation. Panics if the slot
/// isn't found (typeck guarantees it exists).
fn update_slot_value(&mut self, slot_id: SlotId, value: Value);

/// **M06.1**: read the current value at `slot_id`. Used by `Expr::Deref`
/// rvalue evaluation. Returns `None` if not found (typeck would have
/// rejected).
fn lookup_slot_value(&self, slot_id: SlotId) -> Option<Value>;
```

## New: M06.1 reference samples

| File                                  | Notes                                                                            |
|---------------------------------------|----------------------------------------------------------------------------------|
| `tests/samples/m06_1_assign_basic.rs` | `fn main() { let mut x = 0; x = 7; }` — direct assignment, x animates 0 → 7.   |
| `web/samples/m06_1_assign_basic.rs`   | Mirror.                                                                          |
| `tests/samples/m06_1_deref_read.rs`   | `fn main() { let x = 42; let r = &x; let y = *r; }` — y becomes 42 through r.   |
| `web/samples/m06_1_deref_read.rs`     | Mirror.                                                                          |
| `tests/samples/m06_1_deref_write.rs`  | `fn main() { let mut x = 5; let r = &mut x; *r = 10; }` — x animates 5 → 10 while red arrow persists. |
| `web/samples/m06_1_deref_write.rs`    | Mirror.                                                                          |
