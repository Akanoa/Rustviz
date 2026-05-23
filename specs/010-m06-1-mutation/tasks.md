---

description: "Task list for M06.1 — mutation: assignment + deref read/write"
---

# Tasks: M06.1 — Mutation: Assignment + Deref Read/Write

**Input**: Design documents from `/specs/010-m06-1-mutation/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, quickstart.md ✓ (no contracts/ — protocol unchanged)

**Tests**: M01/M02/M03 should stay byte-identical (existing samples don't construct deref or assign). New `cargo test --lib pipeline::tests` covering direct assign, immutable-binding rejected, deref-read, deref-on-non-reference rejected, deref-write, deref-on-shared rejected, borrowed-binding-assignment rejected. Manual M05/M06 QA per the SC-008 procedure.

**Organization**: 3 user stories all P1. Sized M. ~4 source files modified, no protocol changes, no JS work, ~250 LOC net.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1/US2/US3 tag, mandatory on user-story phases only
- File paths relative to repo root

## Path Conventions

4 existing source files modified (no new files in src/). 3 sample pairs added. See `specs/010-m06-1-mutation/plan.md` Project Structure.

---

## Phase 1: Setup

**Purpose**: pre-flight — confirm starting state.

- [X] T001 Verify pre-conditions: branch `010-m06-1-mutation` checked out; `cargo test` from `main` passes (87 tests post-M06); M06 page loads, blue/red borrow arrows render. No code change in this task.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: AST additions + resolver traversal + parser additions. Required by all three user stories.

- [X] T002 In `src/parse/ast.rs`, add `Expr::Deref { inner: Box<Expr>, span: Span }` and `Stmt::Assign { lhs: Expr, rhs: Expr, span: Span }`. Update `Expr::span()` to include the new `Borrow`-style arm for `Deref`. The new variants compile but no consumer handles them yet (non-exhaustive match errors expected in T003 and downstream).

- [X] T003 In `src/resolve.rs`, add traversal for the new AST nodes: `Expr::Deref { inner, .. } => self.resolve_expr(inner)?;` in `resolve_expr`; `Stmt::Assign { lhs, rhs, .. } => { self.resolve_expr(lhs)?; self.resolve_expr(rhs)?; }` in `resolve_stmt`. No new BindingIds introduced (assignment doesn't declare names).

- [X] T004 In `src/parse/parser.rs`, add prefix `*` parsing AND assignment-statement parsing:
  - **Prefix `*`** in `parse_atom` (or the prefix-token dispatch): `TokenKind::Star` → consume, parse sub-expression at bp 70 (same precedence as `&`/`-`/`!`), return `Expr::Deref { inner, span }`.
  - **Assignment statement** in `parse_block`: after parsing a non-let-statement expression, peek for `TokenKind::Eq`. If found, consume, parse rhs expression, expect `;`, return `Stmt::Assign { lhs: <parsed expr>, rhs, span: <lhs.start..semi.end> }`. Otherwise fall through to the existing `Stmt::Expr` / tail-expression path.

**Checkpoint**: `cargo build` clean. All match-arm sites in typeck and eval still need the new variants — those errors persist into Phase 3 and are addressed per-user-story.

---

## Phase 3: User Story 1 — Direct assignment to `mut` binding (Priority: P1)

**Goal**: `fn main() { let mut x = 0; x = 7; }` typechecks, emits `SlotWrite` at the assignment step, and animates `x`'s slot value.

**Independent Test**: load `m06_1_assign_basic.rs` (created in T014), step through, observe `x` slot value animate `0_i32 → 7_i32`.

### Implementation

- [X] T005 [US1] In `src/typeck.rs`, add `typecheck_stmt`'s match arm for `Stmt::Assign { lhs, rhs, span }`. For US1, handle only `Expr::Ident(_, _)` lhs:
  - Place-expression check: anything other than `Expr::Ident(_, _)` or `Expr::Deref(Expr::Ident(_, _), _)` errors with span on lhs: `"left side of assignment must be a place expression"`. (US3 will exercise the Deref(Ident) branch; US1 only adds Ident handling and leaves a `todo!()` or an explicit "not yet supported" branch for Deref-shaped lhs — replaced in T011.)
  - Mutability check: resolve lhs's binding; require `BindingKind::Let { mutable: true, .. }`. Otherwise error with span on lhs: `cannot assign to immutable variable \`{name}\``.
  - Borrow-tracker check: if `tracker.active[binding].len() > 0`, error: `cannot assign to \`{name}\` because it is borrowed`.
  - Type check: typecheck rhs; coerce against lhs's type via existing `try_coerce_to` for literals; mismatched types → typeck error.
  - Return `Ty::Unit` (statement type).

- [X] T006 [US1] In `src/eval.rs`, add `eval_stmt`'s match arm for `Stmt::Assign { lhs, rhs, span }`. For US1, handle only `Expr::Ident(_, _)` lhs:
  - Evaluate `rhs` to a Value. Halt-guard.
  - Resolve lhs's binding → look up its slot via `lookup_local_slot`.
  - Emit `MemEvent::SlotWrite { slot_id, value: rhs_v.clone(), span: *span }`.
  - Add new helper `update_slot_value(slot_id, value)`: walk `self.frames`, find LocalSlot with `slot_id`, update its `value` field in-place. Panic if not found (typeck guarantees).
  - Call `update_slot_value(slot_id, rhs_v)` so subsequent reads (US2's deref-read) see the new value.

- [X] T007 [US1] In `src/pipeline.rs::tests`, add ≥ 2 tests:
  - `run_pipeline_assign_basic` — `fn main() { let mut x = 0; x = 7; }` — verify a `SlotWrite` event fires with the assignment statement's span, and a subsequent reads-x point sees the new value (assertion: a second `let y = x; let z = y;` chain reads `7`, not `0`).
  - `run_pipeline_assign_immutable_rejected` — `fn main() { let x = 0; x = 7; }` — typeck error.

**Checkpoint**: direct assignment to `mut` bindings works end-to-end; the M05 page animates the slot value change.

---

## Phase 4: User Story 2 — Read through a reference (Priority: P1)

**Goal**: `fn main() { let x = 42; let r = &x; let y = *r; }` typechecks, `y` is `42_i32`, blue arrow persists.

**Independent Test**: load `m06_1_deref_read.rs`, step through, observe `y = 42_i32` after the deref-read; blue arrow visible.

### Implementation

- [X] T008 [US2] In `src/typeck.rs`, add `typecheck_expr_inner`'s match arm for `Expr::Deref { inner, span }`:
  - typecheck `inner`.
  - Require inner's type to be `Ty::Ref { inner: T, .. }`. Otherwise error: `"cannot dereference value of type \`{T}\`; expected a reference"` with span on the inner expression.
  - Return `(*T).clone()` (deref produces the inner type, regardless of mutability — works on both `&T` and `&mut T` for reading).

- [X] T009 [US2] In `src/eval.rs`, add `eval_expr`'s match arm for `Expr::Deref { inner, .. }`:
  - Evaluate `inner` to a Value. Halt-guard.
  - Expect `Value::Ref { target_slot, .. }` (typeck guarantees).
  - Add new helper `lookup_slot_value(slot_id) -> Option<Value>`: walk `self.frames`, find LocalSlot with `slot_id`, return `Some(local.value.clone())`. Returns `None` if not found (shouldn't happen — but defensive).
  - Return the looked-up value.

- [X] T010 [US2] In `src/pipeline.rs::tests`, add ≥ 2 tests:
  - `run_pipeline_deref_read_shared` — `fn main() { let x = 42; let r = &x; let y = *r; }` — verify `y`'s SlotWrite has `value` = 42.
  - `run_pipeline_deref_on_non_reference_rejected` — `fn main() { let x = 5; let y = *x; }` — typeck error.

**Checkpoint**: deref-as-rvalue works; values flow through references.

---

## Phase 5: User Story 3 — Write through a mutable reference (Priority: P1)

**Goal**: `fn main() { let mut x = 5; let r = &mut x; *r = 10; }` typechecks; at the `*r = 10` step, `x`'s slot value animates from `5` to `10` while the red arrow persists.

**Independent Test**: load `m06_1_deref_write.rs`, step through, observe `x = 10_i32` updated at the deref-write step; red arrow still visible.

### Implementation

- [X] T011 [US3] In `src/typeck.rs`, extend the `Stmt::Assign` typecheck arm added in T005 to handle `Expr::Deref(Expr::Ident(r), _)` lhs:
  - Recognize the Deref(Ident) shape as a valid place expression (replace any T005 "not yet supported" branch).
  - typecheck `r`; require its type to be `Ty::Ref { mutable: true, .. }`. Otherwise error: `"cannot assign through \`&T\`; need \`&mut T\`"` with span on the lhs.
  - **No** borrow-tracker check for through-ref assignment (per R-008 in research.md — the `&mut` itself is what permits the write; nothing else can take a conflicting borrow during its lifetime).
  - Type check rhs against the deref's type (the inner of the Ref). Existing literal-coercion applies.
  - Return `Ty::Unit`.

- [X] T012 [US3] In `src/eval.rs`, extend the `Stmt::Assign` eval arm added in T006 to handle `Expr::Deref(Expr::Ident(r), _)` lhs:
  - Evaluate `rhs` to a Value. Halt-guard.
  - Resolve `r`'s binding → look up its LocalSlot; read its `Value::Ref { target_slot, .. }` value.
  - Emit `MemEvent::SlotWrite { slot_id: target_slot, value: rhs_v.clone(), span: *assign_span }`.
  - Call `update_slot_value(target_slot, rhs_v)` to keep the in-memory state in sync.

- [X] T013 [US3] In `src/pipeline.rs::tests`, add ≥ 3 tests:
  - `run_pipeline_deref_write_basic` — `fn main() { let mut x = 5; let r = &mut x; *r = 10; }` — verify a SlotWrite for `x`'s slot with value `10` fires, and that the BorrowMut event still active at the next position (no premature BorrowEnd).
  - `run_pipeline_deref_write_through_shared_rejected` — `fn main() { let x = 5; let r = &x; *r = 10; }` — typeck error.
  - `run_pipeline_assign_to_borrowed_rejected` — `fn main() { let mut x = 5; let r = &x; x = 7; }` — typeck error from US1's borrow-tracker check (re-affirms US1 + US3 integrate correctly).

**Checkpoint**: through-ref mutation works end-to-end; the headline pedagogy is delivered.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: 3 sample pairs + dropdown entries + warnings + bundle + audit log + stage.

- [X] T014 [P] Create 3 M06.1 sample pairs (6 files total). Identical content in `tests/samples/` and `web/samples/`:

  - `m06_1_assign_basic.rs`:
    ```rust
    fn main() {
        let mut x = 0;
        x = 7;
    }
    ```
  - `m06_1_deref_read.rs`:
    ```rust
    fn main() {
        let x = 42;
        let r = &x;
        let y = *r;
    }
    ```
  - `m06_1_deref_write.rs`:
    ```rust
    fn main() {
        let mut x = 5;
        let r = &mut x;
        *r = 10;
    }
    ```

- [X] T015 [P] In `web/index.html`, add 3 new `<option>` entries to the sample dropdown after the M06 group:

  ```html
  <option value="m06_1_assign_basic">Direct assignment (M06.1)</option>
  <option value="m06_1_deref_read">Deref read (M06.1)</option>
  <option value="m06_1_deref_write">Deref write (M06.1)</option>
  ```

- [X] T016 [P] Verify SC-007 (bundle size ≤ +20% vs M06 baseline 87,354 B gzipped → ≤ 104,825 B) AND SC-008 (zero warnings):
  - `RUSTFLAGS="-D warnings" cargo build --release` — clean.
  - `RUSTFLAGS="-D warnings" cargo test` — full test suite clean (m01/m02/m03 byte-identical).
  - `cargo build --release --target wasm32-unknown-unknown` — WASM clean.
  - `gzip -kc target/wasm32-unknown-unknown/release/rustviz.wasm | wc -c` — should be ≤ 104,825 B (expected ~90 KB given M06's +4% precedent for additive work).

- [X] T017 Final clean verification: `cargo clean && RUSTFLAGS="-D warnings" cargo build --release && RUSTFLAGS="-D warnings" cargo test && cargo build --release --target wasm32-unknown-unknown`. Full pipeline must pass clean from scratch.

- [X] T018 Append post-implementation audit log to `specs/010-m06-1-mutation/checklists/requirements.md`. Table covering SC-001 through SC-008. SC-001 / SC-002 (visual animation) deferred to maintainer (visual QA). Note: M01/M02/M03 should be byte-identical (no snapshot re-baseline expected). Document any surprises (e.g. if `let mut r = &x; r = &y;` is encountered in QA, whether the borrow lifetime edge case bit).

- [X] T019 Stage all changed files:

  ```bash
  git add Cargo.toml Cargo.lock \
          src/parse/ast.rs src/parse/parser.rs \
          src/resolve.rs src/typeck.rs src/eval.rs src/pipeline.rs \
          tests/samples/m06_1_*.rs web/samples/m06_1_*.rs \
          web/index.html \
          specs/010-m06-1-mutation/ \
          CLAUDE.md
  ```

  Run `git status` and report. **Do not commit** — maintainer's QA pass happens between stage and commit per the UI QA-split convention.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: no dependencies.
- **Phase 2 (Foundational)**: T002 → T003 → T004 sequential (each later step depends on the AST shapes from T002). All three modify different files; sequential because each adds match-arm sites the next ones use.
- **Phase 3 (US1)**: depends on Phase 2 complete. T005 → T006 → T007 sequential.
- **Phase 4 (US2)**: depends on Phase 2 complete. T008 → T009 → T010 sequential. **Independent of Phase 3** — US2 doesn't need US1's assignment work.
- **Phase 5 (US3)**: depends on Phase 3 + Phase 4. T011 extends T005's typeck; T012 extends T006's eval; T013 cross-references US1's borrow-tracker check.
- **Phase 6 (Polish)**: depends on all prior. T014 / T015 / T016 parallel; T017 → T018 → T019 sequential.

### Story-Level Dependencies

- **US1 (direct assign) and US2 (deref-read)** are independent. After Phase 2, they could in principle be tackled by different agents in parallel.
- **US3 (deref-write)** depends on both: T011 extends T005's match arm; T012 extends T006's match arm.

### Parallel Opportunities

- **T014 + T015 + T016**: sample files vs. dropdown HTML vs. read-only audits. Different files. [P] ✓
- **US1 vs US2 (T005–T007 vs T008–T010)**: in a multi-agent setup, parallelizable. Sequential for a single agent.

---

## Parallel Example: Phase 6 polish

```bash
# All three independent in parallel:
Task T014: "Create 3 m06_1_*.rs sample pairs (tests/ + web/)"
Task T015: "Add 3 dropdown entries in web/index.html"
Task T016: "Run warnings + bundle size audits (read-only)"
```

---

## Implementation Strategy

### MVP First (US1 alone)

1. **Phase 1** (T001): pre-flight.
2. **Phase 2** (T002–T004): AST + resolver + parser additions.
3. **Phase 3** (T005–T007): direct assignment.
4. **STOP and VALIDATE**: `cargo test` passes; `let mut x = 5; x = 7;` works in the page. M03's cosmetic `let mut` finally gains meaning. **At this point M06.1's first pedagogical win is shippable** as a smaller increment (US2/US3 would defer to a follow-up).

US2 (deref-read) is independent and small — natural next increment. US3 (deref-write) builds on US1's assignment infrastructure + US2's deref typing.

### Single-Agent Strategy

1. T001 → T002 → T003 → T004 (Phase 1 + 2 sequential).
2. T005 → T006 → T007 (US1).
3. T008 → T009 → T010 (US2).
4. T011 → T012 → T013 (US3).
5. T014 + T015 + T016 (parallel polish), T017 → T018 → T019 (sequential close).

---

## Notes

- [P] tasks = different files, no incomplete-task dependencies.
- [Story] tag mandatory on user-story phases.
- **No new Rust deps**. No new JS deps. **No `Cargo.toml` changes**.
- **No new MemEvent variants** — `SlotWrite` carries all M06.1 mutations.
- **No `Ty` / `Value` extensions** — reuse what exists.
- **No M03 contract amendment** — protocol unchanged.
- **No JS / CSS work** — visualization is "free" via existing SlotWrite animation.
- **M01/M02/M03 byte-identical expected** — no existing samples construct deref or assign; no snapshot re-baseline anticipated.
- **`let mut r = &x; r = &y;` is a known edge case** for ref reassignment — old borrow's BorrowEnd doesn't fire at reassignment, only at scope close. Documented in quickstart.md; left as a known limitation unless QA surfaces it as a blocker.
- **Sized M** per the rubric: 4 source modules + 3 sample pairs + ~7 unit tests. Estimated ~250 LOC net.
- Avoid: implementing compound assignment / multi-level deref / re-borrows through deref / assignment as expression / field/index lhs. All explicitly deferred.
