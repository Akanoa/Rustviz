# Research — M06.1 Implementation Decisions

Decision / Rationale / Alternatives for the mutation milestone.

## AST shape

### R-001 — `Stmt::Assign { lhs, rhs, span }` (statement, not expression)

- **Decision**: assignment is a `Stmt` variant, not an `Expr` variant. Shape:

  ```rust
  Stmt::Assign {
      lhs: Expr,
      rhs: Expr,
      span: Span,  // covers lhs through rhs (and through `;` per existing Stmt convention)
  }
  ```

- **Rationale**:
  - **Statement form matches M06.1's scope**: assignment is only used as a statement (`x = v;` at block level), never as an embedded expression. The expression form (`let y = (x = 5);`) is explicitly out of scope per spec.
  - **Smaller AST surface**: no need for an `Expr::Assign` arm that participates in every Expr match (eval, typeck, resolve, etc.). Stmt-only keeps the change to the existing `Stmt::Let | Stmt::Expr` enum.
  - **Future-proof**: if a future revision wants expression-form assignment, adding `Expr::Assign` then is additive.
- **Alternatives considered**:
  - **`Expr::Assign { lhs, rhs, span }`** (Rust's actual model): requires every Expr consumer to add an arm. Bigger refactor for no immediate benefit. Rejected.
  - **`Expr::Assign` + `Stmt::Expr(Expr::Assign(...))`**: same as above plus the indirection. Rejected.

### R-002 — `Expr::Deref { inner: Box<Expr>, span }`

- **Decision**:

  ```rust
  Expr::Deref {
      inner: Box<Expr>,
      span: Span,  // covers the `*` through the inner expression
  }
  ```

- **Rationale**: minimal shape; mirrors `Expr::Borrow`'s structure (which also has `inner: Box<Expr>` + `span`).
- **Alternatives considered**:
  - **`Expr::Unary { op: UnOp::Deref, ... }`** reusing the existing unary infrastructure: tempting but `Deref` is semantically different (place-expression-producing, not value-transforming). Mixing it into `UnOp` muddles the place-expression check in typeck. Rejected.

## Parser

### R-003 — Prefix `*` precedence

- **Decision**: `*` as a prefix operator binds at the same level as `&`/`-`/`!` (max bp = 70 in the existing parser). Disambiguation from binary `*` happens by parser position: `parse_atom` (or whatever dispatches prefix tokens) sees `*` as prefix; the binary-op section consumes `*` only when it appears AFTER an operand.
- **Rationale**: matches Rust's grammar. The existing precedence ladder reserves 70 for prefix unary, which is fine for `*`.
- **Alternatives considered**: bp 80 (higher than other unaries). Would let `*expr.method()` parse as `*(expr.method())`, but we don't have method calls. Not worth the divergence. Rejected.

### R-004 — Assignment-statement parsing at block level

- **Decision**: in `parse_block`, after parsing a non-let-statement starting expression, peek for `=`. If found, consume it, parse the rhs expression, expect `;`, return `Stmt::Assign { lhs: <parsed expr>, rhs, span }`. Otherwise treat as `Stmt::Expr` or tail expression (existing behavior).
- **Rationale**: simplest integration point. Avoids disrupting the existing `Stmt::Let` path. Existing precedence still works on each side of the `=`.
- **Alternatives considered**:
  - **Parse `=` as an infix operator** at bp 5 (very low, right-associative): would handle assignment in expression context, but per R-001 we're keeping it statement-only. Adding binary `=` to the parser would be misleading. Rejected.
  - **Lookahead from `parse_block` before parsing any expression**: peek the FIRST tokens to detect `IDENT =`. Less general — wouldn't catch `*r = v;`. Rejected.

### R-005 — Span on `Stmt::Assign`

- **Decision**: span covers `lhs.span().start` through the `;` token's end. Matches the existing `Stmt::Let.span` convention (`let` through `;`).
- **Rationale**: consistent with M03's stmt-span convention. The emitted `SlotWrite`'s span uses this span so the editor highlight covers the whole statement.
- **Alternatives considered**: span on just `=` token. Less informative. Rejected.

## Typeck

### R-006 — `Expr::Deref` typing

- **Decision**:
  - typecheck inner; require its type to be `Ty::Ref { inner: T, mutable: _ }`.
  - return `(*T).clone()` — the deref's type is the inner of the Ref, regardless of mutability.
  - if inner's type isn't a Ref, error: `"cannot dereference value of type \`{T}\`; expected a reference"` with span on the inner expression.

### R-007 — `Stmt::Assign` typing

- **Decision**: three-step check:
  1. **Place-expression check**: lhs must be `Expr::Ident(_, _)` OR `Expr::Deref(Expr::Ident(_, _))`. Anything else is a typeck error with span on the lhs: `"left side of assignment must be a place expression"`.
  2. **Mutability check**:
     - `Expr::Ident(x)`: look up x's binding decl; must be `BindingKind::Let { mutable: true, .. }`. Otherwise: `"cannot assign to immutable variable \`{x}\`"` with span on the lhs.
     - `Expr::Deref(Expr::Ident(r))`: typecheck `r` first (gives `Ty::Ref { mutable, .. }`); if `mutable == false`, error: `"cannot assign through \`&T\`; need \`&mut T\`"` with span on the lhs's `*r`.
  3. **Borrow-tracker check**: query the tracker for any active borrow of the lhs binding:
     - For `Expr::Ident(x)`: lookup `tracker.active[x]` — if non-empty, error: `"cannot assign to \`{x}\` because it is borrowed"` with span on the lhs and a hint mentioning the existing borrow's span (if available).
     - For `Expr::Deref(Expr::Ident(r))`: lookup `tracker.active[r]` — if non-empty, error: `"cannot mutate through \`r\` because it is borrowed"` (with the same logic as above; the `&mut` permitting the write is what's IN `tracker.active[r]` — but that's the active borrow ITSELF, the check is for whether there's ANOTHER borrow on top).
     - Wait — for `*r = v`, `r` having an active mut borrow is fine (that's how it works). The check should be: `r` not having ADDITIONAL borrows beyond its own. Hmm — actually `r` itself is the value holding the borrow; tracker tracks borrows OF bindings, not OF refs. So the active borrow listed for `r`'s target binding (x) is what `r` IS. Mutating through `r` doesn't require checking anything new — the typechecker already validated that the borrow was takeable. **Decision: skip the borrow-tracker check for `*r = v`. Only check direct `x = v`.** Documented in R-008.
  4. **Type check**: typecheck rhs; coerce lhs's expected type with the M03.2 `try_coerce_to` if rhs is a literal. If types still mismatch, error: `"expected \`{lhs_ty}\`, found \`{rhs_ty}\`"`.

### R-008 — Borrow tracker check is only for direct assignment

- **Decision**: for `x = v;`, the tracker check fires (assignment fails if x is borrowed). For `*r = v;`, no tracker check — the validity of mutating through `r` was already established when `r` was constructed (typeck'd `&mut x` against the tracker at that time, ensuring no other borrows existed). After construction, the `&mut` borrow stays exclusive until scope-end; nothing can take a conflicting borrow during its lifetime. So `*r = v` is always safe wrt aliasing.
- **Rationale**: matches Rust's actual rule. The `&mut` is what permits the write; checking the tracker at write-time would be redundant.
- **Alternatives considered**: also check at write-time defensively — would catch internal bugs but adds noise. Rejected.

### R-009 — Mutating a `mut` binding through a sub-binding

- **Decision**: `let mut x = 5; let mut r = &mut x; *r = 7;` is valid. `let mut x = 5; let r = &mut x; *r = 7;` is ALSO valid — `r` itself doesn't need to be `mut` to mutate through it (Rust's actual rule: `mut` on the variable holding the ref is unrelated to mutating through the ref). `r: &mut T` mutates regardless of `r` itself being a `mut` variable.
- **Rationale**: matches Rust. The `mut` keyword in `let mut r` means "I can reassign `r` itself" (e.g. `r = &mut y;`). It doesn't affect what `*r = v;` does, which depends on `r`'s TYPE being `&mut T`.
- **Implementation**: typeck of `Stmt::Assign` with `lhs = Expr::Deref(Expr::Ident(r))` checks `r`'s TYPE (not its mut-binding-kind). The `r` itself can be `let r` or `let mut r` — irrelevant.

## Eval

### R-010 — `Expr::Deref` as rvalue

- **Decision**: evaluate inner to a `Value::Ref { target_slot, .. }`. Walk the call stack (`lookup_slot_value(target_slot)`) to find the LocalSlot with that id; return its current value. If not found, panic (typeck guarantees the slot exists during the borrow's lifetime).
- **Rationale**: the value lives in the target's slot, not the ref. Reading reflects whatever the target's current value is.
- **Helper added**: `fn lookup_slot_value(&self, slot_id: SlotId) -> Option<Value>` in `Evaluator`. Mirrors `lookup_local_slot`.

### R-011 — `Stmt::Assign` evaluation

- **Decision**: two-case dispatch on lhs:
  - `Expr::Ident(x)`: resolve x → BindingId → LocalSlot. Emit `MemEvent::SlotWrite { slot_id: x.slot, value: rhs_v.clone(), span: assignment.span }`. Update the LocalSlot's stored `value` field in-place.
  - `Expr::Deref(Expr::Ident(r))`: read r's Value::Ref → target_slot. Emit `MemEvent::SlotWrite { slot_id: target_slot, value: rhs_v.clone(), span: assignment.span }`. Find the LocalSlot anywhere in the call stack with `slot_id == target_slot`; update its `value` in-place.
- **Rationale**: the existing `SlotWrite` semantics already mean "the bytes at this slot are now these bytes." Reusing it for re-assignment is exactly correct.
- **In-place update is critical**: without it, a subsequent `let y = *r;` after `*r = 7;` would read the OLD value (still `5`). Eval needs to keep the in-memory model in sync with the emitted events.

### R-012 — `SlotWrite`'s `span` field for assignments

- **Decision**: use the assignment statement's full span (`lhs.span().start` through `;` end). Consistent with how `let x = 5;` emits SlotWrite spans (covers the let-statement).
- **Rationale**: the editor highlights the whole assignment statement when the cursor lands on a SlotWrite emitted by an assignment. Matches existing M03 behavior.
- **Alternatives considered**: span on just lhs (identifies WHAT was assigned); span on rhs (identifies WHAT VALUE was used). Whole-statement span is most informative; matches existing convention. Rejected.

## Resolve

### R-013 — Traversal for new AST nodes

- **Decision**: `resolve_expr` adds an `Expr::Deref { inner, .. } => self.resolve_expr(inner)?` arm. `resolve_stmt` (or wherever statements are traversed) adds `Stmt::Assign { lhs, rhs, .. } => { self.resolve_expr(lhs)?; self.resolve_expr(rhs)?; }`.
- **Rationale**: standard traversal, no new BindingIds (assignment doesn't introduce names).

## SlotWrite event semantics

### R-014 — `SlotWrite` is the same variant for init and re-assignment

- **Decision**: no new event variant. M06.1 reuses `MemEvent::SlotWrite { slot_id, value, span }` for assignment-emitted writes. The UI's `apply_event` for `SlotWrite` already updates the slot's value cell — works without changes.
- **Rationale**: this was the design payoff of M03 separating `SlotAlloc` from `SlotWrite`. Re-assignment doesn't allocate, only writes. Variant reuse is exactly the right behavior.

## M03 contract amendment?

### R-015 — No M03 contract amendment

- **Decision**: M03's `m03-api.md` doesn't need updating. The `SlotWrite` variant's payload (slot_id, value, span) and its semantics ("write this value into this slot") were specified neutrally — they apply equally to init-time writes (M03) and re-assignment writes (M06.1). The contract docstring already implicitly permits this.
- **Rationale**: no protocol-level change → no contract amendment. The M03 contract amendment history (M03.1, M03.2, M06) is for SHAPE changes (new variants, restructures). Reusing an existing variant for a new logical case is below the threshold.

## Constitution

### R-016 — Same vacuous PASS

- **Decision**: `.specify/memory/constitution.md` still unfilled. No gates apply.
