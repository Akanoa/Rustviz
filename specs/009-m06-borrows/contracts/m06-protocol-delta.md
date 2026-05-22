# Contract — M06 Protocol Delta

M06 fills M03's reserved borrow-event payloads and adds variants to `Ty` and `Value`. No new `MemEvent` variants. This document is the **delta**; M03's unchanged surface stays in `specs/004-m03-event-eval/contracts/m03-api.md`.

## Closed-enum rule — third invocation

M03.1 added `MemEvent::ReturnValue` (additive variant).
M03.2 restructured `Ty` and `Value` (rule generalized to all event-protocol types).
**M06** adds variants to `Ty` (`Ref`) and `Value` (`Ref`) — pure additive growth. Already permitted by the relaxed rule; this is the third precedent.

## `Ty::Ref` — additive variant

```rust
pub enum Ty {
    Int(IntKind),
    Float(FloatKind),
    Bool,
    Unit,
    Ref { inner: Box<Ty>, mutable: bool },  // NEW in M06
}
```

**Cascading consequence**: `Ty` drops the `Copy` derive (Box isn't Copy). Methods change from `(self)` to `(&self)`. Pure-internal refactor; no JSON wire-format change.

JSON shape gains a new tag: `{ "Ref": { "inner": <Ty>, "mutable": bool } }`. JS consumers (M04 renderer's `render_ty`) update to handle the new case.

## `Value::Ref` — additive variant

```rust
pub enum Value {
    Int { kind, bits },
    Float { kind, value },
    Bool(bool),
    Unit,
    Ref {                                   // NEW in M06
        borrow_id: BorrowId,
        target_slot: SlotId,
        mutable: bool,
    },
}
```

JSON shape gains: `{ "Ref": { "borrow_id": <u32>, "target_slot": <u32>, "mutable": bool } }`. JS-side `render_value` updates to display references as `&<target>` or `&mut <target>` strings.

## `MemEvent::BorrowShared` / `BorrowMut` / `BorrowEnd` — payloads filled

The variants existed since M03 with their payload shapes already defined:

```rust
BorrowShared { borrow_id: BorrowId, target: Pointee, span: Span }
BorrowMut    { borrow_id: BorrowId, target: Pointee, span: Span }
BorrowEnd    { borrow_id: BorrowId, span: Span }
```

M06 starts emitting them from the evaluator. For L2, `target` is always `Pointee::Slot(SlotId)`. `Pointee::Heap(HeapAddr)` is reserved for M07.

### Emission semantics

- **`BorrowShared`**: emitted when an `Expr::Borrow { mutable: false }` evaluates. Position in the event stream: right after the borrow expression's place identifier is resolved, before the resulting `Value::Ref` is bound to a slot via `SlotWrite`.
- **`BorrowMut`**: same as `BorrowShared` but with `mutable: true`.
- **`BorrowEnd`**: emitted at scope exit, in reverse-allocation order, before the slot drops for any non-Copy slots (which in M06 means `&mut T` slots — and only `&mut T` borrows themselves; mut refs are non-Copy per Rust).

### Borrow lifetimes

- M06 implements **scope-level lifetimes** only. A borrow taken at scope depth `d` lives until the scope exits.
- NLL (non-lexical lifetimes, ending the borrow when no longer used) is **explicitly deferred**.

## Aliasing rules (statically enforced at typeck)

- At any cursor position, a binding `x` has either:
  - 0+ shared borrows (`&x`), zero mutable borrows, OR
  - exactly 1 mutable borrow (`&mut x`), zero shared borrows.
- Violations are typeck errors with span on the offending borrow expression and a message identifying the conflict.

## Lexer / parser additions

- `TokenKind::Amp` (`&`) and `TokenKind::AmpMut` (`&mut`) — new tokens, contiguous `&mut` only.
- `Expr::Borrow { inner: Box<Expr>, mutable: bool, span }` — new AST node.
- `Type::Ref { inner: Box<Type>, mutable: bool, span }` — new AST node.

The M01 lexer rejection of `&` (which emitted "L1 doesn't support borrows") is removed. Any test asserting that specific rejection is updated to instead accept the borrow.

## `StateSnapshot.borrows` — additive view field

```rust
pub struct StateSnapshot {
    // ... existing fields
    pub borrows: Vec<BorrowView>,           // NEW in M06
}

pub struct BorrowView {
    pub source_slot: u32,
    pub target_slot: u32,
    pub mutable: bool,
}
```

Reflects currently-active borrows at the snapshot's cursor position. The M05 page reads this to render the SVG arrow overlay (blue for `!mutable`, red for `mutable`).

## Behavioral guarantees (post-M06)

- **B-M6-1**: For every successful `Expr::Borrow` evaluation, exactly one `BorrowShared` or `BorrowMut` event fires, with a unique `borrow_id`.
- **B-M6-2**: For every emitted `BorrowShared`/`BorrowMut`, exactly one matching `BorrowEnd` fires before the trace ends (unless the trace halts on a runtime error mid-borrow).
- **B-M6-3**: At any cursor position, `StateSnapshot.borrows` includes exactly the borrows whose `BorrowShared`/`BorrowMut` event has fired but whose `BorrowEnd` has not.
- **B-M6-4**: `Ty::Ref { mutable: false, .. }.is_copy()` returns `true`; `Ty::Ref { mutable: true, .. }.is_copy()` returns `false`. Reflects Rust's actual `Copy` implementation for `&T` vs `&mut T`.
- **B-M6-5**: Aliasing rule violations are caught at typeck — never at eval. If the evaluator sees a Value::Ref, the borrow is guaranteed to respect the aliasing rules.
- **B-M6-6**: Place-expression restriction: `&expr` requires `expr` to be `Expr::Ident(_, _)` in M06. Other expressions (`&5`, `&(1 + 2)`, `&f()`) are typeck errors with span on `expr`.

## What this contract does NOT cover (deferred)

- **Deref `*r`** — references are observable arrows but not yet usable values (no read/write through references).
- **Named lifetimes `<'a>`** — function signatures with reference parameters use elision only. No lifetime parameters.
- **Returning references from functions** — requires named lifetimes. Out of scope.
- **`Drop` for `&mut T`** — references don't have destructors. `BorrowEnd` is the closest signal; no `SlotDrop` for references that are themselves references (since `&T` is Copy and `&mut T`'s "drop" is just the BorrowEnd).
- **NLL** — borrows end at lexical scope exit only.
- **Re-borrowing through deref** — `&*r` not supported (needs deref).
- **Field borrows** — no fields yet (M07+).
- **`Box<&T>` / `Vec<&T>` etc.** — needs heap types (M07).
