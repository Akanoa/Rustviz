# Feature Specification: M06 — Level 2: References and Borrows

**Feature Branch**: `009-m06-borrows`
**Created**: 2026-05-22
**Status**: Draft
**Input**: User description: "M06"

**Authoritative scope source**: [`MILESTONES.md` › M06 — Level 2: references and borrows](../../MILESTONES.md). The In scope / Out of scope / Entry criteria / Exit criteria / Demo lines in that block are the contract for this feature. This spec elaborates user stories, requirements, and success criteria; it does not redefine scope.

## User Scenarios & Testing *(mandatory)*

M06 is **the project's first Level-2 milestone** — the lattice expands from "primitive values in stack slots" to "values + references between them." The lexer accepts `&` and `&mut` for the first time (replacing the M01 rejection that ships with a clear pedagogical error). The evaluator emits `BorrowShared` / `BorrowMut` / `BorrowEnd` events with proper `BorrowId` payloads. The UI gains an SVG arrow overlay drawing blue arrows for shared borrows, red for mutable. Aliasing rule violations are caught statically at typeck and surfaced in the editor as borrow-check errors with a span.

This is the milestone where the project's pedagogical premise — visualizing what's usually invisible — pays off most. The arrows make Rust's borrow checker tangible.

### User Story 1 — Shared borrows visible (Priority: P1)

A learner types `fn main() { let x = 5; let r = &x; }` in the editor. After the pipeline compiles, the stacks panel shows two slots: `x = 5_i32` and `r : &i32`. **A blue arrow** appears in the SVG overlay, originating from the `r` slot and pointing at the `x` slot. Stepping past the end of `main`'s scope, the arrow disappears (`BorrowEnd` fires) before `r` and `x` themselves disappear with the frame.

**Why this priority**: shared borrows are the canonical introduction to references in Rust. Without them, the milestone hasn't shipped. P1.

**Independent Test**: load the upcoming `m06_shared_borrow.rs` sample, step through, observe the blue arrow appearing and disappearing at the right cursor positions.

**Acceptance Scenarios**:

1. **Given** the source `fn main() { let x = 5; let r = &x; }`, **When** the pipeline runs, **Then** typeck succeeds and the evaluator emits a `BorrowShared` event (the cursor step at which `r` is bound) followed by a `BorrowEnd` event (at the end of `main`'s scope).
2. **Given** the resulting trace is loaded into the M05 page, **When** the cursor is positioned after the `BorrowShared` event, **Then** a blue SVG arrow is visible originating from `r`'s slot card and terminating at `x`'s slot card.
3. **Given** the cursor is positioned after the `BorrowEnd` event, **When** the SVG overlay re-renders, **Then** the blue arrow is no longer visible.
4. **Given** multiple shared borrows `let r1 = &x; let r2 = &x;`, **When** stepped through, **Then** two blue arrows are visible simultaneously (each pointing at `x`), both ending at scope exit. No aliasing violations are reported (multiple `&` is permitted).

---

### User Story 2 — Mutable borrows visible (Priority: P1)

A learner types `fn main() { let mut x = 5; let r = &mut x; }`. The stacks panel shows `x = 5_i32` (with the mut indicator) and `r : &mut i32`. **A red arrow** appears from `r` to `x`. At scope exit, the arrow disappears.

**Why this priority**: mutable borrows are the second half of Rust's reference story and the source of most beginner confusion. P1 alongside US1.

**Independent Test**: load `m06_mut_borrow.rs`, observe the red arrow throughout the borrow's lifetime.

**Acceptance Scenarios**:

1. **Given** the source `fn main() { let mut x = 5; let r = &mut x; }`, **When** the pipeline runs, **Then** typeck succeeds and the evaluator emits `BorrowMut` followed by `BorrowEnd`.
2. **Given** the cursor is on the `BorrowMut` step, **When** the overlay renders, **Then** a **red** arrow (visually distinct from the blue shared-borrow arrow) connects `r` to `x`.
3. **Given** an attempt to take `&mut x` when `x` is not declared `mut` (`let x = 5; let r = &mut x;`), **When** the pipeline runs, **Then** typeck reports an error spanning the `&mut` expression: "cannot borrow `x` as mutable; it is not declared as mutable".

---

### User Story 3 — Aliasing rule violations are caught at typeck (Priority: P1)

A learner writes `fn main() { let mut x = 5; let r1 = &x; let r2 = &mut x; }`. The editor displays a red wavy underline at the `&mut x` expression with a status-bar message: "cannot borrow `x` as mutable because it is also borrowed as immutable" (or similar). The stacks panel does not advance past the error.

**Why this priority**: this IS the borrow checker. Showing learners the rule violation as a static error — at the source location — is the heart of M06's pedagogy. P1.

**Independent Test**: load `m06_aliasing_error.rs` (a deliberately-violating sample), observe the underline + status message + disabled controls (US2 behavior from M05).

**Acceptance Scenarios**:

1. **Given** `let r1 = &x; let r2 = &mut x;` (shared then mutable), **When** the pipeline runs, **Then** typeck rejects with an error spanning the `&mut x` expression.
2. **Given** `let r1 = &mut x; let r2 = &mut x;` (two mutable), **When** the pipeline runs, **Then** typeck rejects the second `&mut`.
3. **Given** `let r1 = &mut x; let r2 = &x;` (mutable then shared), **When** the pipeline runs, **Then** typeck rejects the `&x`.
4. **Given** `let r1 = &x; let r2 = &x;` (two shared — legal), **When** the pipeline runs, **Then** typeck accepts and two blue arrows appear.
5. **Given** all error cases above, **When** the editor shows the violation, **Then** the message identifies (a) the type of borrow attempted, (b) the existing conflicting borrow, and (c) the binding being contended over.

---

### User Story 4 — Borrows ending at scope exit (Priority: P2)

A learner writes a nested-block program:

```rust
fn main() {
    let x = 5;
    {
        let r = &x;
    }
    // r is gone; x is still alive
}
```

When the cursor passes the inner block's closing brace, the blue arrow representing `r → x` disappears (a `BorrowEnd` event fires). `x` itself remains alive in the outer scope.

**Why this priority**: scope-level lifetime tracking is the M06 minimum (per MILESTONES.md). The visualization makes the otherwise-implicit lifetime visible. P2 because the simpler US1+US2 cases already validate the borrow events firing; this story specifically validates the BorrowEnd timing.

**Independent Test**: load `m06_scoped_borrow.rs`, step through, observe the arrow disappearing exactly at the inner block's closing brace.

**Acceptance Scenarios**:

1. **Given** a nested block introducing a borrow, **When** the cursor reaches the closing `}` of the inner block, **Then** `BorrowEnd` fires for that borrow and the arrow disappears from the SVG overlay.
2. **Given** an outer binding survives the inner block, **When** the cursor passes the inner block's closing `}`, **Then** the outer binding's slot remains visible (only the borrow ended; the borrowed value is intact).

---

### Edge Cases

- **Empty borrow** (`let r = &x;` followed by no use of `r`): valid. The borrow's lifetime is the rest of `r`'s scope. Arrow appears and persists until scope exit.
- **Borrow of a literal expression** (`let r = &5;`): currently NOT in scope — `&` requires a place expression in Rust. M06 typeck rejects with a clear error. M07+ can revisit if temporaries are wanted.
- **Borrow of a non-Copy type**: not relevant in M06's L2 lattice (still i32/bool/float). M07+ adds heap types where this matters more.
- **Re-borrowing** (`let r = &x; let s = r;`): copy semantics — `s` becomes another shared borrow with the same target. Two arrows from two slots both pointing at `x`. Both end at scope exit.
- **Re-borrowing through `*`** (`let r = &x; let y = *r;`): deref operator is **deferred** to keep M06 scope tight. M06 references are observable arrows but not yet usable values. A future revision can add `*r` semantics.
- **Borrow as function argument** (`fn f(r: &i32) { ... }; f(&x);`): in scope. The callee frame's parameter slot displays as `r : &i32` with an arrow back to `x` in the caller's grayed frame OR active frame, depending on M03.1's frame-grayed semantics. The arrow follows control flow.
- **Returning a reference from a function** (`fn f<'a>(x: &'a i32) -> &'a i32`): **out of scope** — requires named lifetime annotations which MILESTONES.md explicitly defers. M06 supports only `fn f(r: &i32)` (parameter-side borrow only, no borrow returned).
- **Aliasing violation across function call**: in scope. If `f(&x)` is called while `&mut x` is active, the typeck flags the call site as a violation.
- **Borrow of mutable binding without `&mut`**: legal (`let mut x = 5; let r = &x;`). `&` doesn't require `mut`; only `&mut` does.
- **`*r = v` (assignment through mutable ref)**: deferred along with deref. Without deref support, `&mut` is observable but the simulated mutation it would enable isn't visualized. Future revision.
- **Self-reference** (`let r = &r;`): can't happen — `r` doesn't exist before its `let`. resolve catches this as "use of undeclared variable".
- **Borrow lifetime ends mid-statement**: M06's scope-level lifetimes only end at scope close (`}`). Rust's NLL (non-lexical lifetimes) can end them earlier; M06 doesn't implement NLL. Documented in Assumptions.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST lex `&` and `&mut` as tokens `Amp` and `AmpMut` (replacing M01's outright rejection of `&`). The two-char lookahead at `&` distinguishes `& mut` from `&mut` and produces `AmpMut` for the latter; `&` followed by anything else is `Amp`.
- **FR-002**: System MUST parse borrow expressions (`&expr` and `&mut expr`) and borrow types (`&T` and `&mut T`) into appropriate AST nodes.
- **FR-003**: System MUST recognize `&T` and `&mut T` as valid type annotations in `let` bindings and function parameter types.
- **FR-004**: System MUST reject `&mut x` at typeck when `x` is not declared `mut`. Error message identifies the binding and the missing `mut` keyword.
- **FR-005**: System MUST enforce Rust's aliasing rules at typeck (scope-level): for any binding `x`, at any point in time there are either (a) zero or more shared borrows `&x` and zero mutable borrows, OR (b) exactly one mutable borrow `&mut x` and zero shared borrows. Violations are typeck errors with span on the offending borrow expression.
- **FR-006**: System MUST emit `MemEvent::BorrowShared` and `MemEvent::BorrowMut` events at the point each borrow is taken. Each event carries a unique `BorrowId` and a `Pointee` identifying the target slot.
- **FR-007**: System MUST emit `MemEvent::BorrowEnd` events at the end of each borrow's lifetime (scope-level for M06). Each `BorrowEnd` references the original `BorrowId`.
- **FR-008**: System MUST extend the M04 `StateSnapshot` with a list of active borrows derivable from the event stream. Each entry identifies (a) the source slot (the binding holding the reference), (b) the target slot (what's being borrowed), and (c) the borrow kind (shared or mutable).
- **FR-009**: The M04/M05 page MUST render an SVG arrow overlay across the panels. Blue arrows for `Shared` borrows, red arrows for `Mut` borrows. Arrows update on every cursor step and animate (or just hide/show) when borrows end.
- **FR-010**: System MUST treat borrow expressions in function call arguments: `f(&x)` creates a borrow visible across the call boundary (parameter slot in callee's frame has an arrow back to the argument slot in the caller's frame).
- **FR-011**: System MUST ship at least 4 new reference programs under `tests/samples/m06_*.rs` + `web/samples/m06_*.rs` demonstrating: (a) shared borrow, (b) mutable borrow, (c) aliasing rule violation (deliberately failing), (d) nested-block borrow with `BorrowEnd` at the inner brace.
- **FR-012**: System MUST update the M05 sample dropdown with the new M06 sample entries.

### Key Entities

- **Borrow expression**: an AST node `&expr` or `&mut expr`. The expression must be a *place expression* (an identifier or path) — borrowing a value expression like `&(2 + 3)` is rejected at typeck.
- **Borrow type**: an AST node `&T` or `&mut T`. Used in type annotations.
- **`Ty::Ref { inner: Box<Ty>, mut: bool }`** (or equivalent shape): new type variant or pair representing a reference type. Plan-phase decides the exact shape.
- **`BorrowId`**: opaque identifier already declared in M03's event protocol. M06 fills the payloads.
- **`Pointee`**: enum `Slot(SlotId) | Heap(HeapAddr)` already declared in M03. For L2, only `Pointee::Slot` is used (heap arrives in M07).
- **Borrow tracker (typeck side-table)**: a borrow-checker state mapping each binding to its currently-active borrows. Used to detect aliasing violations. Lifetime: per typeck invocation.
- **Active-borrow view (StateSnapshot)**: a list of currently-active borrows derived from the event stream at the cursor's position. Used by the SVG overlay to render arrows.
- **SVG arrow overlay**: a top-layer DOM element positioned absolutely over the stacks panel, with arrows drawn between slot card positions. Plan-phase decides positioning strategy (DOM queries vs. layout pass).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After M06 ships, the M05 page accepts source containing `&` and `&mut` and produces a trace with `BorrowShared`/`BorrowMut`/`BorrowEnd` events within the existing 1-second SC-001 latency budget.
- **SC-002**: For a program with N shared borrows of the same binding, N blue arrows render in the SVG overlay simultaneously, each correctly originating from the borrowing slot. Verified by automated visual test or maintainer QA.
- **SC-003**: For a mutable borrow, exactly one red arrow renders, visually distinct from blue (different color, easy to tell apart at a glance).
- **SC-004**: Aliasing rule violations (mutable-while-shared, mutable-while-mutable, shared-while-mutable) are caught at typeck with: (a) a red wavy underline at the offending borrow span (via M05's existing error UX), (b) a status-bar message identifying the conflict, (c) all three violation patterns covered.
- **SC-005**: At least 4 new `m06_*.rs` reference programs ship (per FR-011), each loadable from the dropdown and visually demonstrating the relevant pedagogy.
- **SC-006**: Existing M01–M03.2, M04, M05 tests pass — M06 is additive at the lattice and event level. M01/M02/M03/M03.1/M03.2 snapshot tests may re-baseline if `Ty` or `Value` Debug formats shift (additive enum variants), but the diff is mechanical and predictable.
- **SC-007**: WASM bundle growth: per the relaxed budget policy (project memory) — new functionality (lexer + parser + typeck borrow rules + eval lifetime tracker + SVG renderer) is expected to add real code. Soft budget +50% vs M03.2 baseline (84 KB → ≤ 126 KB gzipped); hard ceiling stays M04's 2 MB SC-005.
- **SC-008**: Zero warnings under `RUSTFLAGS="-D warnings" cargo build --release` AND `cargo test`. Both host and WASM targets clean.

## Assumptions

- **Scope-level lifetimes, not NLL**: M06 borrow lifetimes extend from the borrow expression to the end of the enclosing scope (or function body). Rust's actual NLL ends lifetimes earlier when the borrow is no longer used. Per CLAUDE.md and MILESTONES.md, scope-level is sufficient for L2 pedagogy.
- **No deref operator (`*r`)**: M06 references are observable but not yet usable. Reading through a shared ref (`let y = *r;`) or writing through a mutable ref (`*r = 7;`) is deferred. A future revision (M06.1 or similar) can lift this.
- **No named lifetimes** (`<'a>`): Rust's lifetime-parameter syntax (`fn f<'a>(...)`) is explicitly deferred. Function signatures with reference params/returns use elision; the visualizer doesn't expose `'a` to learners in M06.
- **No returning references from functions**: `fn f() -> &i32` requires named lifetimes (or `'static`). Out of scope. Only parameter-side borrows are supported.
- **Place expressions only**: `&` and `&mut` must be applied to a *place* (identifier or path). Borrowing a value expression like `&(2 + 3)` or `&foo()` is a typeck error. This matches Rust's rule.
- **`Ty::Ref` representation**: plan-phase decides between `Ty::Ref { inner: Box<Ty>, mut: bool }` and a flatter alternative like `Ty::SharedRef(Box<Ty>) | Ty::MutRef(Box<Ty>)`. Either way the closed-enum-with-revisions rule (M03.1 precedent, M03.2 generalization) authorizes the additive growth.
- **Value representation for references**: a borrow value is `Value::Ref { borrow_id: BorrowId }` or similar pointing at the BorrowId of the live borrow. Stack slots holding references store this; the StateSnapshot derives the arrow from the borrow event stream + the BorrowId. Plan-phase confirms the exact Value extension.
- **SVG arrow positioning**: arrows are positioned absolutely based on the bounding boxes of the slot card DOM elements. The overlay re-renders on every cursor step + window resize. Plan-phase decides whether to use SVG or HTML+CSS curved lines.
- **Aliasing rules are checked at typeck**: not at eval. The borrow checker is part of typeck's static analysis pass, mirroring Rust's actual compiler architecture. Eval can trust that the events it emits respect the rules (no eval-time aliasing checks needed).
- **Multiple-error reporting**: per M01 convention, the borrow checker stops at the first error. Multi-error reporting is deferred.
- **Bundle-size budget is generous**: per the project memory on bundle-size policy, M06 is genuinely growing the lexer + parser + typeck + eval + UI surface. A 50% growth vs M03.2 (which is itself already over the +5% rolling budget) is the soft target. The hard ceiling is M04's 2 MB.
- **First Level-2 milestone**: M06 introduces the L2 lattice. M07 will add heap types; M08 adds threads. M06's borrow scaffolding is the foundation for the heap pointers (owning arrows) M07 will introduce.
- **Sized L** per the rubric: 4–5 modules touched (`parse/{lexer,parser,ast,token}`, `typeck`, `event`, `eval`, `ui`, plus a new SVG overlay component in JS), 3+ in-scope bullets, 2+ integration boundaries (Rust pipeline ↔ event stream ↔ UI ↔ SVG overlay).
