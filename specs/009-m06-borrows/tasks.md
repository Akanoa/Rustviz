---

description: "Task list for M06 — Level 2: references and borrows"
---

# Tasks: M06 — Level 2: References and Borrows

**Input**: Design documents from `/specs/009-m06-borrows/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/m06-protocol-delta.md ✓, quickstart.md ✓

**Tests**: M01/M02/M03 should pass byte-identically (existing samples don't construct `Value::Ref` or `Ty::Ref`). New `cargo test --lib` tests for typeck (borrow tracker + place-expression check), eval (BorrowShared/Mut/End emission), pipeline (end-to-end shared/mut/aliasing/scoped scenarios). Manual M05+M06 QA per the SC-008 procedure.

**Organization**: 4 user stories (US1/US2/US3 all P1, US4 P2). Largest milestone since M03/M04 — sized L. ~8 source files + new SVG overlay component.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1/US2/US3/US4 tag, mandatory on user-story phases only
- File paths relative to repo root

## Path Conventions

8 existing source files modified + 1 inline borrow_tracker mod in typeck.rs + new SVG overlay in web/. See `specs/009-m06-borrows/plan.md` Project Structure.

---

## Phase 1: Setup

**Purpose**: pre-flight — confirm starting state.

- [X] T001 Verify pre-conditions: branch `009-m06-borrows` checked out; `cargo test` from `main` passes (79 tests post-M03.2); M05 page loads and the editor accepts source. No code change in this task.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: lexer tokens + AST nodes + Ty/Value variant additions + the cascade refactor of Ty's `Copy` derive. Both US1 and US2 build on this.

- [X] T002 [P] Amend M03's contract in `specs/004-m03-event-eval/contracts/m03-api.md`: note M06 as the third invocation of the closed-enum-with-revisions rule (after M03.1 added `MemEvent::ReturnValue`, M03.2 restructured `Ty`+`Value`). M06 adds `Ty::Ref` and `Value::Ref` — pure additive growth, no restructure. Cross-reference `specs/009-m06-borrows/contracts/m06-protocol-delta.md`.

- [X] T003 [P] Lex `&` and `&mut`. In `src/parse/token.rs`, add `TokenKind::Amp` and `TokenKind::AmpMut` variants + their `describe()` strings (`"`&`"` and `"`&mut`"`). In `src/parse/lexer.rs`, replace M01's outright rejection of `&` with: see `&`, peek for `mut` keyword immediately following (no whitespace), if so consume both and emit `AmpMut`; otherwise emit `Amp`. Update / remove any M01 test that asserted the rejection (likely 1 negative test in `tests/m01.rs` or its snapshots).

- [X] T004 AST + parser changes for borrows. In `src/parse/ast.rs`, add `Expr::Borrow { inner: Box<Expr>, mutable: bool, span: Span }` and `Type::Ref { inner: Box<Type>, mutable: bool, span: Span }`. Update `Expr::span()` to handle the new variant. In `src/parse/parser.rs`, extend `parse_atom` (or wherever prefix unary operators are dispatched) to recognize `TokenKind::Amp` → `Expr::Borrow { mutable: false, ... }` and `TokenKind::AmpMut` → `Expr::Borrow { mutable: true, ... }`. Extend `parse_type` to handle `&T` / `&mut T`. Depends on T003.

- [X] T005 In `src/typeck.rs`, add the `Ty::Ref { inner: Box<Ty>, mutable: bool }` variant. **Drop the `Copy` derive on `Ty`** (Box<Ty> isn't Copy). Update `Ty::name(&self) -> String` (allocates — uses `format!` for refs, delegates to `IntKind::name()`/`FloatKind::name()` for leaves) and `Ty::is_copy(&self) -> bool` (returns `true` for `Int`/`Float`/`Bool`/`Unit`/`Ref { mutable: false }`; `false` for `Ref { mutable: true }`). At this point the code WILL NOT compile — every `Ty::I32`/etc. site previously taking `Ty` by value now needs `&Ty` or `.clone()`. T006 fixes them.

- [X] T006 Mechanical cascade refactor: every method taking `Ty` by value becomes `&Ty`, and every call site that consumed `Ty` and then needed it again does `.clone()`. Search `git grep -nE 'fn [a-z_]+\(.*Ty\)' src/` to find the per-value method sites; search `git grep -n 'ty:' src/` and similar for field accesses. Update `IntKind::contains(self, v) -> bool` to stay `(self)` since it's already Copy; same for `FloatKind`. Audit `TypeMap`'s storage (still stores `Ty` by value — but as `IndexMap<Span, Ty>`, the value is owned, fine). Expect ~50 sites; the refactor is uniform.

- [X] T007 In `src/event.rs`, add `Value::Ref { borrow_id: BorrowId, target_slot: SlotId, mutable: bool }` variant. Keep existing derives (`Clone, Debug, PartialEq, Serialize, Deserialize` — no `Eq`). Update `Value::type_name(&self)` to return `"&T"` style for refs (or just `"&"` / `"&mut"` plus delegated inner type). In `src/ui.rs::render_value`, add a case for `Value::Ref { mutable, target_slot, .. }` — render as `format!("&slot{}", target_slot)` for shared or `format!("&mut slot{}", target_slot)` for mut. Plan-phase QA may tune the visual.

**Checkpoint**: `cargo build` clean. `cargo test` may show snapshot drift on a single M01 test if it asserted the `&` rejection — re-baseline that one snapshot. M02 / M03 should stay byte-identical (no Value::Ref or Ty::Ref in existing samples).

---

## Phase 3: User Story 1 — Shared borrows visible (Priority: P1)

**Goal**: `let r = &x;` typechecks, emits `BorrowShared` + `BorrowEnd` at scope exit, renders a blue SVG arrow in the overlay.

**Independent Test**: load `m06_shared_borrow.rs` (created in T020), step through, observe the blue arrow appearing at the borrow step and disappearing at scope close.

### Implementation

- [X] T008 [US1] In `src/typeck.rs`, typecheck `Expr::Borrow { inner, mutable: false, span }`: (a) verify `inner` is a place expression (only `Expr::Ident(_, _)` for M06; reject anything else with span on `inner`); (b) typecheck `inner` to get its type `T`; (c) return `Ty::Ref { inner: Box::new(T), mutable: false }`. Also typecheck `Type::Ref { inner, mutable, span }` via the existing `ty_from_ast` path — recurse on `inner`, wrap in `Ty::Ref`. Integration with `let` annotation already handled by existing typecheck_stmt (annotation vs init Ty comparison).

- [X] T009 [US1] In `src/eval.rs`, evaluate `Expr::Borrow { inner, mutable, span }`: (a) resolve `inner` to a `BindingId` (must be `Expr::Ident`); (b) look up the slot id holding that binding via `lookup_local_value`'s slot — actually we need a new helper `lookup_local_slot(binding_id) -> Option<SlotId>`. Add that helper. (c) Allocate a new `BorrowId` via `next_borrow_id` field on the Evaluator struct (add the field, init to 0 in `new`). (d) Emit `MemEvent::BorrowShared { borrow_id, target: Pointee::Slot(slot_id), span }`. (e) Return `Value::Ref { borrow_id, target_slot: slot_id, mutable: false }`. The let-stmt machinery does the rest (SlotAlloc + SlotWrite for `r`).

- [X] T010 [US1] In `src/eval.rs`, track borrows per scope. Add `borrows: Vec<BorrowId>` field to the existing `Scope` struct. On `Expr::Borrow` emission (T009), push the new BorrowId to the current scope's `borrows`. In `drop_current_scope`, BEFORE iterating locals to emit SlotDrops, iterate `scope.borrows` in reverse and emit `MemEvent::BorrowEnd { borrow_id, span: <scope_end_span> }`. Use the scope's closing brace span — or fall back to the function decl span if not easily available.

- [X] T011 [US1] In `src/ui.rs`, extend `World` and `StateSnapshot` to track active borrows. Add `borrows: Vec<ActiveBorrowState>` field to `World` (struct: `{ borrow_id, source_slot, target_slot, mutable }`). In `apply_event`, handle `MemEvent::BorrowShared` (push) and `MemEvent::BorrowEnd` (remove by borrow_id). Add `BorrowView { source_slot, target_slot, mutable }` public type with serde derives. Extend `StateSnapshot` with `pub borrows: Vec<BorrowView>` field; populate from World in `state_snapshot()`. The source_slot is derived from which slot holds the corresponding `Value::Ref` after the upcoming `SlotWrite`; for the initial emission (between BorrowShared and SlotWrite) there may be no source_slot yet — track this in World by also handling `SlotWrite { value: Value::Ref { borrow_id, .. }, slot_id }` to bind source_slot to an existing ActiveBorrowState.

- [X] T012 [US1] SVG overlay scaffolding. In `web/index.html`, add `<svg id="arrow-overlay" style="position: absolute; pointer-events: none; ..."></svg>` element positioned over the `<main>` content. In `web/style.css`, add `.arrow-shared` (blue: e.g. `stroke: #2a6fa5; fill: #2a6fa5;`), plus base SVG sizing rules. Define an arrowhead `<marker>` in the SVG. In `web/index.js`, add `data-slot-id="<id>"` attribute on every slot row when rendering (in the existing `render` loop). Add `renderArrows(borrows)` function that: clears existing arrows, then for each `BorrowView`, queries the source and target slot's DOM positions via `getBoundingClientRect()`, computes start/end points + a curve control point, and appends an `<path>` element to the overlay with class `arrow-shared` (or `arrow-mut` — added in US2 phase). Call `renderArrows(state.borrows)` from the existing `render` function. Add `window.addEventListener('resize', () => render(latestState))` (with `latestState` cached at the module level).

- [X] T013 [US1] Add unit tests in `src/pipeline.rs::tests`. Cover: (a) `run_pipeline_shared_borrow` — `fn main() { let x = 5; let r = &x; }` produces a `BorrowShared` event followed by `BorrowEnd`; (b) `run_pipeline_shared_borrow_multiple` — `let r1 = &x; let r2 = &x;` produces two BorrowShared events (both valid, no aliasing error). At least 2 tests.

**Checkpoint**: typing `fn main() { let x = 5; let r = &x; }` in the page produces a trace; the stacks panel shows `r` as a Ref slot; the SVG overlay draws a blue arrow from `r` to `x`; arrow disappears at frame exit.

---

## Phase 4: User Story 2 — Mutable borrows visible (Priority: P1)

**Goal**: `let r = &mut x;` (where `x` is `mut`) typechecks, emits `BorrowMut` + `BorrowEnd`, renders a red SVG arrow.

**Independent Test**: load `m06_mut_borrow.rs`, step through, observe the red arrow.

### Implementation

- [X] T014 [US2] In `src/typeck.rs`, typecheck `Expr::Borrow { inner, mutable: true, span }`: same as T008 but additionally verify the borrowed binding is declared `mut`. Look up the `BindingDecl` via `Resolution`; check `BindingKind::Let { mutable: true, .. }`. If not mutable, error with span on the borrow: `cannot borrow `{name}` as mutable; not declared as mutable`. Return `Ty::Ref { inner: Box::new(T), mutable: true }`.

- [X] T015 [US2] In `src/eval.rs`, the `Expr::Borrow` evaluation already handles both shared and mutable (T009 took the `mutable` flag). Update the emitted event variant: if `mutable`, emit `MemEvent::BorrowMut` instead of `BorrowShared`. Same payload shape; just the variant differs.

- [X] T016 [US2] In `web/style.css`, add `.arrow-mut` rule (red: e.g. `stroke: #c62828; fill: #c62828;`). In `web/index.js::renderArrows`, switch between `arrow-shared` and `arrow-mut` classes based on `borrow.mutable`. The marker (arrowhead) can be shared between both (uses currentColor in SVG).

- [X] T017 [US2] Add unit tests in `src/pipeline.rs::tests`: (a) `run_pipeline_mut_borrow` — `fn main() { let mut x = 5; let r = &mut x; }` produces `BorrowMut` + `BorrowEnd`; (b) `run_pipeline_mut_borrow_on_non_mut_binding` — `let x = 5; let r = &mut x;` → typeck error.

**Checkpoint**: mutable borrows render with red arrows; non-mut bindings reject the `&mut`.

---

## Phase 5: User Story 3 — Aliasing rule violations caught at typeck (Priority: P1)

**Goal**: typeck enforces Rust's borrow-checker aliasing rules statically. Violations show as red-wavy-underline editor errors (reusing M05's error UX).

**Independent Test**: load `m06_aliasing_error.rs`, observe a typeck error at the second borrow with a clear message; stacks panel doesn't advance.

### Implementation

- [X] T018 [US3] In `src/typeck.rs`, add an inline `mod borrow_tracker` with the data model from data-model.md: `BorrowTracker { active: IndexMap<BindingId, Vec<ActiveBorrow>> }`, `ActiveBorrow { kind, scope_depth, borrow_span }`, `BorrowKind { Shared, Mut }`, `AliasConflict { existing_kind, existing_span }`. Methods: `new()`, `try_take_shared(b, depth, span) -> Result<(), AliasConflict>`, `try_take_mut(b, depth, span) -> Result<(), AliasConflict>`, `pop_scope(leaving_depth)`. Unit tests in the same mod for each rule: success cases (empty active, multiple shared OK), failure cases (shared + mut conflict, mut + anything conflict).

- [X] T019 [US3] Integrate the BorrowTracker into typeck. Add a `borrow_tracker: BorrowTracker` field to the `Typechecker` struct; add a `scope_depth: u32` counter. In `typecheck_block`, increment `scope_depth` on entry, decrement on exit, call `borrow_tracker.pop_scope(scope_depth)` on the way out. In `Expr::Borrow` typeck (extends T008/T014): before returning `Ty::Ref`, call `try_take_shared` or `try_take_mut` on the target binding. On `AliasConflict`, return a `ParseError` with span on the new borrow and message: `cannot borrow \`{name}\` as {new_kind} because it is already borrowed as {existing_kind} here` (use `existing_span` for the "here" reference — could be in the message text or just on the new borrow's span; plan-phase decision).

- [X] T020 [US3] Add unit tests in `src/pipeline.rs::tests`: (a) `run_pipeline_shared_then_mut_conflict` — `let mut x = 5; let r1 = &x; let r2 = &mut x;` → typeck error; (b) `run_pipeline_mut_then_mut_conflict` — `let r1 = &mut x; let r2 = &mut x;` → typeck error; (c) `run_pipeline_mut_then_shared_conflict` — `let r1 = &mut x; let r2 = &x;` → typeck error; (d) `run_pipeline_multiple_shared_ok` — `let r1 = &x; let r2 = &x;` → no error.

**Checkpoint**: all three violation patterns produce typeck errors with spans; multiple-shared remains legal.

---

## Phase 6: User Story 4 — Borrows end at scope close (Priority: P2)

**Goal**: borrows taken inside a nested block end at the inner block's closing brace. Verified end-to-end.

**Independent Test**: load `m06_scoped_borrow.rs`, observe the arrow disappearing at the inner `}` while the outer `x` persists.

### Implementation

- [X] T021 [US4] Verify the BorrowEnd timing implemented in T010 is correct for nested blocks. Step through `m06_scoped_borrow.rs` in cursor unit tests and assert: at position N (just after the inner `BorrowEnd` event), `state.borrows.len() == 0` and `state.frames[0].slots.len() == 1` (only `x` remains). Add this as `run_pipeline_scoped_borrow_ends_at_inner_brace` in `src/pipeline.rs::tests`.

**Checkpoint**: BorrowEnd fires precisely at the scope where the borrow was taken, not at the outer function exit.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: 4 sample pairs + dropdown entries + warnings + bundle size + audit log + stage.

- [X] T022 [P] Create 4 M06 sample pairs (8 files total). Identical content in `tests/samples/` and `web/samples/`:

  - `m06_shared_borrow.rs`:
    ```rust
    fn main() {
        let x = 5;
        let r = &x;
    }
    ```
  - `m06_mut_borrow.rs`:
    ```rust
    fn main() {
        let mut x = 5;
        let r = &mut x;
    }
    ```
  - `m06_aliasing_error.rs`:
    ```rust
    fn main() {
        let mut x = 5;
        let r1 = &x;
        let r2 = &mut x;
    }
    ```
  - `m06_scoped_borrow.rs`:
    ```rust
    fn main() {
        let x = 5;
        {
            let r = &x;
        }
    }
    ```

- [X] T023 [P] In `web/index.html`, add 4 new `<option>` entries to the sample dropdown after the M03.2 group:

  ```html
  <option value="m06_shared_borrow">Shared borrow (M06)</option>
  <option value="m06_mut_borrow">Mutable borrow (M06)</option>
  <option value="m06_aliasing_error">Aliasing error (M06)</option>
  <option value="m06_scoped_borrow">Scoped borrow (M06)</option>
  ```

- [X] T024 [P] Verify SC-007 (bundle size ≤ +50% vs M03.2 baseline 84,007 B gzipped → ≤ 126,011 B) AND SC-008 (zero warnings). Commands:
  - `RUSTFLAGS="-D warnings" cargo build --release` — clean.
  - `RUSTFLAGS="-D warnings" cargo test` — full test suite clean (note: m01 may need 1 snapshot re-baseline from T003; m02/m03 stay byte-identical).
  - `cargo build --release --target wasm32-unknown-unknown` — WASM clean.
  - `gzip -kc target/wasm32-unknown-unknown/release/rustviz.wasm | wc -c` — should be ≤ 126,011 B.

- [X] T025 Run final clean verification: `cargo clean && RUSTFLAGS="-D warnings" cargo build --release && RUSTFLAGS="-D warnings" cargo test && cargo build --release --target wasm32-unknown-unknown`. Full pipeline must pass clean from scratch.

- [X] T026 Append post-implementation audit log to `specs/009-m06-borrows/checklists/requirements.md`. Table covering SC-001 through SC-008. SC-001 / SC-002 / SC-003 / SC-005 deferred to maintainer (visual QA). Document the `Ty` `Copy`-drop cascade (how many sites refactored, any surprises). Document any M01 snapshot re-baseline. Note any QA-driven follow-ups.

- [X] T027 Stage all changed files:

  ```bash
  git add Cargo.toml Cargo.lock \
          src/parse/token.rs src/parse/lexer.rs src/parse/ast.rs src/parse/parser.rs \
          src/typeck.rs src/event.rs src/eval.rs src/ui.rs src/lib.rs \
          tests/snapshots/ tests/samples/m06_*.rs web/samples/m06_*.rs \
          web/index.html web/index.js web/style.css \
          specs/004-m03-event-eval/contracts/m03-api.md specs/009-m06-borrows/ \
          CLAUDE.md
  ```

  Run `git status` and report. **Do not commit** — maintainer's QA pass happens between stage and commit per the UI QA-split convention.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: no dependencies.
- **Phase 2 (Foundational)**: T002 parallel to T003. T004 depends on T003 (parser needs tokens). T005 + T006 sequential (refactor follows Ty change). T007 parallel-able with T005+T006 but in practice sequential (both touch shared types).
- **Phase 3 (US1)**: depends on Phase 2 complete. T008 → T009 → T010 → T011 → T012 → T013. Mostly sequential because each step's data flow depends on the previous (typeck → eval emits events → World tracks → UI renders → tests verify).
- **Phase 4 (US2)**: depends on Phase 3 complete. T014 → T015 → T016 → T017. Same data flow.
- **Phase 5 (US3)**: depends on Phase 3 + Phase 4 (BorrowTracker integration needs both kinds of borrows). T018 → T019 → T020.
- **Phase 6 (US4)**: depends on US1's BorrowEnd implementation (T010). T021 is verification + test addition.
- **Phase 7 (Polish)**: depends on all prior phases. T022 / T023 / T024 parallel; T025 → T026 → T027 sequential.

### Story-Level Dependencies

- **US1 and US2** are nearly parallel — they share most infrastructure (typeck::Borrow, eval emission, SVG renderer). US2 is incremental on top of US1's scaffolding (adds the `mut` flag handling + red color).
- **US3 (aliasing)** depends on both US1 and US2 — the BorrowTracker checks both kinds.
- **US4 (scoped)** is mostly a test/verification on top of US1's scope-exit BorrowEnd implementation.

### Parallel Opportunities

- **T002 + T003**: M03 contract amendment vs. lexer-side token additions. Different files. [P] ✓
- **T022 + T023 + T024**: sample files vs. dropdown HTML vs. read-only audits. [P] ✓
- **US1/US2 vs. US3**: in a multi-agent setup, the BorrowTracker (US3) can be built in parallel with the eval emission (US1/US2) since they don't conflict at the file level. Sequential in practice for a single agent.

---

## Parallel Example: Phase 7 polish

```bash
# All three independent in parallel:
Task T022: "Create 4 m06_*.rs sample pairs (tests/ + web/)"
Task T023: "Add 4 dropdown entries in web/index.html"
Task T024: "Run warnings + bundle size audits (read-only)"
```

---

## Implementation Strategy

### MVP First (US1 alone, no mut, no aliasing)

1. **Phase 1** (T001): pre-flight.
2. **Phase 2** (T002–T007): foundational lexer + parser + Ty/Value variants + cascade refactor.
3. **Phase 3** (T008–T013): shared borrow typeck + eval + UI overlay + tests.
4. **STOP and VALIDATE**: `cargo test` passes; shared borrows render blue arrows in the page. **At this point M06's headline pedagogy is shippable** as a smaller milestone (US2/US3/US4 would defer to a follow-up).

US2 (mutable borrows) is the natural next increment — small delta on top of US1's scaffolding. US3 (aliasing) is the largest still-remaining piece. US4 (scoped) is essentially a free verification test.

### Single-Agent Strategy

1. T001 → T002 + T003 (parallel-able, but in practice T002 first since it's pure docs) → T004 (depends on T003) → T005 → T006 (cascade) → T007.
2. T008 → T009 → T010 → T011 → T012 → T013 (US1 sequential).
3. T014 → T015 → T016 → T017 (US2 sequential).
4. T018 → T019 → T020 (US3 sequential).
5. T021 (US4 single task).
6. Phase 7: T022 + T023 + T024 (parallel-able read/new-file work), T025 → T026 → T027.

---

## Notes

- [P] tasks = different files, no incomplete-task dependencies.
- [Story] tag mandatory on user-story phases.
- **No new Rust deps**. No new JS deps.
- **No new MemEvent variants** — `BorrowShared` / `BorrowMut` / `BorrowEnd` already exist in M03's protocol with their payload types pre-declared. M06 just fills them.
- **`Ty` drops `Copy`** — the biggest invasive change in M06's foundational phase. Estimated ~50 method/site refactors. Mechanical but should be done as a single PR-style batch to avoid intermediate broken states.
- **M01 may have 1 snapshot re-baseline** from the lexer's `&` rejection being removed. M02/M03 should stay byte-identical.
- **Aliasing rules are statically enforced at typeck**, not eval. Eval trusts emitted events respect the rules.
- **Scope-level lifetimes only**, not NLL. Borrow ends at enclosing `}`.
- **No deref (`*r`), no named lifetimes, no reference-returning functions** — all deferred per spec.
- **SVG overlay** is the most user-visible new piece. Plan-phase chose SVG over canvas/CSS curves; renderer queries `data-slot-id` attributes via DOM, positions arrows via `getBoundingClientRect()`.
- **Window resize re-renders** the arrow overlay.
- **Bundle-size budget +50%** from M03.2 baseline (per project memory on bundle-size policy — variant-growth + new SVG renderer expected to be significant).
- **Maintainer QA between stage and commit** — same pattern as prior milestones.
- Avoid: implementing deref / NLL / named lifetimes / reference-returning fns. All deferred.
