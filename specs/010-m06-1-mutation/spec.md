# Feature Specification: M06.1 — Mutation: Assignment + Deref Read/Write

**Feature Branch**: `010-m06-1-mutation`
**Created**: 2026-05-22
**Status**: Draft
**Input**: User description: "M06.1"

**Authoritative scope source**: [`MILESTONES.md` › M06.1 — Mutation: assignment + deref read/write](../../MILESTONES.md). The In scope / Out of scope / Entry criteria / Exit criteria / Demo lines in that block are the contract for this feature. This spec elaborates user stories, requirements, and success criteria; it does not redefine scope.

## User Scenarios & Testing *(mandatory)*

M06.1 closes M06's pedagogical loop. M06 shipped `&` and `&mut` as observable arrows in the SVG overlay, but learners noticed: with `&mut x`, what's the point? You can SEE the mutable reference but can't actually mutate through it. M06.1 lands the missing half — direct assignment to `mut` bindings (also closing M03's cosmetic-`mut` gap), deref-as-rvalue (`let y = *r;`), and deref-as-lvalue (`*r = 5;`). The visualization improvement is "free": existing `SlotWrite` events animate slot value changes; the red arrow stays anchored to the source slot WHILE the target slot's value updates.

This is **the milestone where mutation becomes tangible**. The pedagogy of `&mut` finally pays off — learners see "the arrow points at `x`, I write `*r = 10`, `x`'s slot value animates to `10`, the arrow stays put."

### User Story 1 — Direct assignment to a `mut` binding (Priority: P1)

A learner types `fn main() { let mut x = 0; x = 7; }` and steps through. The slot `x` is allocated with value `0_i32`. At the `x = 7;` step, the slot's value animates to `7_i32`. The visualization makes mutation tangible: same slot, value changes.

**Why this priority**: this is the foundational case — assignment without any reference indirection. M03's `let mut` keyword has been cosmetic since M03 (no statement re-assigned bindings); M06.1 makes it actually do something. Without this, the deref-write case is also blocked since both share the assignment statement infrastructure. P1.

**Independent Test**: load `m06_1_assign_basic.rs`, step through, observe `x = 0_i32 → 7_i32` at the assignment step.

**Acceptance Scenarios**:

1. **Given** `let mut x = 0; x = 7;`, **When** the pipeline runs, **Then** typeck succeeds and the evaluator emits a `SlotWrite` event at the assignment step with `value = 7`.
2. **Given** `let x = 0; x = 7;` (no `mut`), **When** the pipeline runs, **Then** typeck rejects the assignment with span on the lhs identifier and message identifying the missing `mut`.
3. **Given** `let mut x: u8 = 5; x = 256;`, **When** the pipeline runs, **Then** typeck rejects with the existing M03.2 "literal out of range for u8" error (mutation doesn't relax range checks).
4. **Given** `let mut x: i32 = 5; x = true;`, **When** the pipeline runs, **Then** typeck rejects with span on the rhs and a "type mismatch" message.

---

### User Story 2 — Read through a reference (Priority: P1)

A learner types `fn main() { let x = 42; let r = &x; let y = *r; }`. The slot `r` shows the blue arrow pointing at `x` (M06). At the `let y = *r;` step, slot `y` is allocated with value `42_i32` — the value flowed *through* the reference. The blue arrow stays visible.

**Why this priority**: deref-as-rvalue is the read half of mutation pedagogy. Without it, learners can take borrows but can't use them. P1 alongside US1 because both expose the missing "do something with the reference" half.

**Independent Test**: load `m06_1_deref_read.rs`, step through, observe `y = 42_i32` after the deref-read step; blue arrow persists.

**Acceptance Scenarios**:

1. **Given** `let x = 42; let r = &x; let y = *r;`, **When** the pipeline runs, **Then** typeck succeeds and `y` is bound to `42_i32` at runtime.
2. **Given** `let x = 42; let r = &x; let y: i32 = *r;`, **When** the pipeline runs, **Then** `y` is `42_i32` (annotation matches).
3. **Given** `let mut x = 5; let r = &mut x; let y = *r;`, **When** the pipeline runs, **Then** typeck succeeds (deref works on both `&T` and `&mut T` for reading).
4. **Given** `let x = 5; let y = *x;` (deref on a non-reference), **When** the pipeline runs, **Then** typeck rejects with span on `*x` and a "cannot deref non-reference" message.

---

### User Story 3 — Write through a mutable reference (Priority: P1)

A learner types `fn main() { let mut x = 5; let r = &mut x; *r = 10; }`. After the `&mut x` step the red arrow appears from `r` to `x`. At the `*r = 10;` step, **the value at `x`'s slot animates from `5_i32` to `10_i32` WHILE the red arrow remains anchored**. The pedagogy is exact: same slot, value changes, the mut reference is the visible cause.

**Why this priority**: this IS the headline pedagogy for `&mut`. Without it, `&mut` was just an observation in M06. P1.

**Independent Test**: load `m06_1_deref_write.rs`, step through, observe `x = 10_i32` updated at the `*r = 10` step with the red arrow still visible.

**Acceptance Scenarios**:

1. **Given** `let mut x = 5; let r = &mut x; *r = 10;`, **When** the pipeline runs, **Then** at the `*r = 10` step a `SlotWrite` event fires for `x`'s slot with `value = 10`.
2. **Given** `let x = 5; let r = &x; *r = 10;`, **When** the pipeline runs, **Then** typeck rejects with span on `*r` and a "cannot assign through `&T`; need `&mut T`" message.
3. **Given** `let mut x: u8 = 250; let r = &mut x; *r = 256;`, **When** the pipeline runs, **Then** typeck rejects with "literal out of range for u8" (range checks unchanged by deref-write).
4. **Given** the trace at the `*r = 10` step is rendered, **When** the user observes the page, **Then** the red arrow from `r` to `x` is still visible — the mutation flows through the reference, doesn't break or reset it.

---

### Edge Cases

- **Assignment as expression vs statement**: in Rust, `x = 5` is an expression of type `()`. M06.1 supports the **statement-only** form (`x = 5;` ending with `;`). Using assignment as an embedded expression (`let y = (x = 5);`) is **out of scope** — plan-phase confirms.
- **Multi-level deref** `**r`: out of scope. M06.1 only supports a single `*` prefix. Nested references aren't constructible in L1+L2 anyway.
- **Re-borrow through deref** `let s = &*r;`: out of scope. M06.1's deref supports rvalue read + lvalue write only, not as part of a borrow expression.
- **Compound assignment** (`x += 5`, `*r *= 2`): out of scope. Only `=` in M06.1.
- **Assignment within an if/else** (e.g. `if cond { x = 5; } else { x = 10; }`): in scope — these are just statements at block level, no special handling.
- **Aliasing during mutation**: `*r = 10` on `r: &mut T` does NOT take a new borrow. The existing `&mut` borrow is what permits the write. The borrow tracker's state is unchanged across the assignment statement.
- **Reading through a borrowed binding directly**: `let mut x = 5; let r = &x; x = 7;` — typeck must reject the direct assignment to `x` because `x` is currently borrowed (immutably). The M06 borrow tracker already knows about active borrows; assignment integrates with this check.
- **Assignment to a literal or expression**: `5 = 7;`, `(x + 1) = 5;` — typeck rejects "left side of assignment must be a place expression."
- **Place expression for lhs**: in M06.1, place expressions are `Expr::Ident(_, _)` and `Expr::Deref(Expr::Ident(_, _), _, _)`. Nothing else.
- **`*r = ...` where `r` is itself a borrowed binding**: tricky if `let r = &mut x; let s = &r; *r = 10;` — `r` is borrowed by `s`, mutation through `r` might be allowed (Rust's NLL gets nuanced here). For M06.1's scope-level lifetimes, we apply the simple rule: a `*r = ...` requires `r` to not be currently borrowed. If `r` IS borrowed (by `s`), the mutation through `r` is rejected.
- **Empty assignment** (`x = ;`): parse error (missing rhs), caught at parse stage.
- **Pre-mutation slot read**: when `*r = 10` runs after `r` has been declared, the slot `r` itself isn't modified — `r` still points at the same target. The mutation goes through `r` to `x`. The `SlotWrite` event targets `x`'s slot id, not `r`'s.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST extend the AST to express deref expressions (`*expr`) and assignment statements (`lhs = rhs;`). Specific shapes are plan-phase decisions; the AST changes are additive over M06's AST.
- **FR-002**: System MUST parse `*expr` as a prefix unary expression at expression positions. Disambiguation from binary `*` (multiplication) is resolved by the lexer/parser based on position (prefix vs. infix), not the token itself.
- **FR-003**: System MUST parse `lhs = rhs;` as a statement at block level. The lhs can be any expression syntactically; the typeck restricts it to place expressions.
- **FR-004**: System MUST typeck `*r` as follows: `r` must have type `Ty::Ref { inner, .. }`; the deref's type is `inner` (regardless of `mutable` — read access works on both `&T` and `&mut T`). Dereferencing a non-reference is a typeck error.
- **FR-005**: System MUST typeck assignment as follows: lhs must be a *place expression* — either `Expr::Ident(x)` where `x` is a `let mut` binding, OR `Expr::Deref(Expr::Ident(r))` where `r` has type `Ty::Ref { mutable: true, .. }`. Any other lhs is a typeck error.
- **FR-006**: System MUST verify the assignment lhs and rhs have matching types per the M03.2 numeric coercion rules (literal coercion still applies, e.g. `let mut x: u8 = 5; x = 250;` works because `250` coerces to `u8`).
- **FR-007**: System MUST integrate with the M06 borrow tracker: if the lhs is `Expr::Ident(x)` and `x` is currently borrowed (shared or mutable), the assignment is a typeck error ("cannot assign to `x` because it is borrowed"). If the lhs is `*r` where `r` is itself currently borrowed, also reject.
- **FR-008**: System MUST emit, at evaluation of an assignment statement, exactly one `MemEvent::SlotWrite` event targeting the lhs's resolved slot — `x`'s slot for `x = v`, `r.target_slot` for `*r = v`. No new event variants are introduced.
- **FR-009**: System MUST preserve M06's borrow events: a `*r = v` assignment does NOT emit `BorrowEnd` (the existing `&mut` borrow is still alive after the assignment). Subsequent steps continue to render the red arrow until the borrow's scope ends.
- **FR-010**: System MUST type assignment expressions as `Ty::Unit`. (Even though M06.1 only supports assignment-as-statement, the underlying expression-typing layer must produce a consistent type — used implicitly when assignments appear in larger AST contexts in future milestones.)
- **FR-011**: System MUST ship at least 3 new reference programs (`tests/samples/m06_1_*.rs` + `web/samples/m06_1_*.rs`) demonstrating: (a) direct assignment to a `mut` binding, (b) deref-as-rvalue with the arrow persisting, (c) deref-as-lvalue with the slot value updating and the red arrow persisting.
- **FR-012**: System MUST add the three new sample entries to the M05 dropdown.

### Key Entities

- **Deref expression**: an AST node `*expr`. Inner is any expression but typeck restricts it to expressions of `Ty::Ref { .. }`.
- **Assignment**: a statement (or expression typed as `()`) of the form `lhs = rhs`. Place-expression restriction on lhs.
- **Place expression (extended)**: for M06.1, place expressions on the assignment lhs are: `Expr::Ident(_, _)` (direct binding) and `Expr::Deref(Expr::Ident(_, _), _)` (through-ref). `&` and `&mut` apply to the same set.
- **Borrow tracker extension**: assignments query the borrow tracker. Direct assignment to `x` fails if `x` is borrowed. Through-ref assignment to `*r` doesn't take a new borrow — the existing `&mut` is what permits it.
- **`SlotWrite` event reuse**: the existing event variant carries assignments. No new event variant.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After M06.1 ships, the M05 page accepts source containing `x = v;`, `let y = *r;`, and `*r = v;` (where types match per M06's rules). Pipeline produces a trace; the stacks panel animates slot value changes; M06's SVG arrows persist across through-ref mutations.
- **SC-002**: Direct assignment to a non-`mut` binding is a typeck error with the offending identifier's span. Verified by automated test.
- **SC-003**: Deref-write through a `&T` (shared ref) is a typeck error with span on `*r`. Verified by automated test.
- **SC-004**: A `*r = v` step emits exactly one `SlotWrite` event targeting `r.target_slot`. The borrow corresponding to `r` is still active at the next cursor position (no `BorrowEnd` fired). Verified by automated test.
- **SC-005**: At least 3 new `m06_1_*.rs` reference programs ship — at minimum: (a) direct mut assignment, (b) deref-read, (c) deref-write.
- **SC-006**: Existing M01, M02, M03, M03.1, M03.2, M04, M05, M06 tests all pass — M06.1 is purely additive at the language level (no new events, no Ty/Value variants). M03 snapshots stay byte-identical (existing samples don't use deref or assignment).
- **SC-007**: WASM bundle growth ≤ +20% vs M06 baseline (87,354 B gzipped → ≤ 104,825 B). Adding deref + assignment is small surface change; should easily fit.
- **SC-008**: Zero warnings under `RUSTFLAGS="-D warnings" cargo build --release` AND `cargo test`. Both host and WASM targets clean.

## Assumptions

- **Assignment is a STATEMENT in M06.1**, not an expression. The form is `lhs = rhs;` at block level only. The expression form `let y = (x = 5);` (which would type `y` as `()`) is out of scope. Plan-phase confirms by deciding the AST shape: `Stmt::Assign { lhs, rhs, span }` rather than `Expr::Assign`.
- **No compound assignment** (`+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`, `^=`, `<<=`, `>>=`). Only `=`. A future revision could add these if pedagogically valuable.
- **Single-level deref only**. `**r`, `***r`, etc. are not parseable. The parser accepts `*expr` once at the prefix position; nested `*` inside `expr` would be a multiplication operator (different position).
- **No re-borrow through deref** (`&*r`, `&mut *r`): out of scope. These would require treating `*r` as a place expression for borrows, which extends M06's place-expression set in non-trivial ways. Defer to a future revision if needed.
- **Borrow tracker integration**: the existing M06 tracker is sufficient. M06.1 adds assignment-rejection rules that consult the tracker; doesn't need new tracker fields.
- **`SlotWrite` event reuse**: M06.1 emits no new event variants. The existing `SlotWrite { slot_id, value, span }` carries the mutation. Visualization is "free" — the stacks panel already animates slot value changes.
- **No protocol changes**: no `MemEvent` changes, no `Ty` changes, no `Value` changes. M03's contract is unchanged.
- **Place-expression set is small**: `Expr::Ident` and `Expr::Deref(Expr::Ident)` only. No fields, no indexing, no method-receiver places. Future milestones (M07's heap types) may extend.
- **Type coercion still applies**: `let mut x: u8 = 5; x = 250;` — the rhs literal coerces to `u8`. M03.2's `try_coerce_to` machinery is reused on the assignment rhs.
- **Span anchoring**: the `SlotWrite` emitted for an assignment uses the assignment statement's span (covering `lhs = rhs`), not the lhs alone. This matches the existing `SlotWrite` convention for `let` initializers (uses the binding's decl span).
- **Default to statement form, plan-phase confirms `Stmt::Assign`** vs alternative shapes like `Expr::Assign` (with `Stmt::Expr(Expr::Assign(...))`). Either works; statement form has slightly less AST surface.
- **Sized M** per the rubric: 3 modules touched (`parse/{ast,parser}`, `typeck`, `eval`) + 3 sample pairs + ~6 new unit tests. Estimated ~250 LOC net change.
