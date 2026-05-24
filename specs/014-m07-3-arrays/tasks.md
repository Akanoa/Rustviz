---

description: "Task list for M07.3 — Arrays (`[T; N]`, stack-allocated sequences)"
---

# Tasks: M07.3 — Arrays (`[T; N]`, stack-allocated sequences)

**Input**: Design documents from `/specs/014-m07-3-arrays/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/m07-3-protocol-delta.md ✓, quickstart.md ✓

**Tests**: M01/M02/M03 should stay byte-identical (no existing L1 sample constructs `Ty::Array`). New `cargo test --lib pipeline::tests` covering: array literal + len, indexing, indexing OOB, slicing, slicing OOB, zero-heap-events assertion. ≥ 6 new tests. Manual M07.3 QA per the SC-008 procedure.

**Organization**: 3 user stories (US1 + US2 + US3 all P1). Sized M. ~4 source files modified + minor JS/CSS for inline stack-cells + 3 sample pairs.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1/US2/US3 tag, mandatory on user-story phases only
- File paths relative to repo root

## Path Conventions

~4 existing source files modified + small JS/CSS additions in `web/` + 3 sample pairs. See `specs/014-m07-3-arrays/plan.md` Project Structure.

---

## Phase 1: Setup

**Purpose**: pre-flight — confirm starting state.

- [X] T001 Verify pre-conditions: branch `014-m07-3-arrays` checked out; `cargo test` from `main` passes (119 tests). Baseline confirmed.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: parser additions + AST nodes + `Ty::Array` + `Value::Array` + UI scaffolding. Required by all three user stories. **Smaller than M07.2's Phase 2** — no new MemEvent variants, no Pointee changes, no protocol restructure.

- [X] T002 Amend M03's contract in `specs/004-m03-event-eval/contracts/m03-api.md` — done.

- [X] T003 In `src/parse/ast.rs`, add two new variants:
  - `Expr::ArrayLit { elements: Vec<Expr>, span }` (after the existing `Range` arm).
  - `Type::Array { inner: Box<Type>, size: u64, span }` (alongside existing `Slice`).
  - Update `Expr::span()` to cover `ArrayLit { span, .. } => *span`.
  - Update `parser::type_span()` (or equivalent) to handle `Type::Array { span, .. }`.

- [X] T004 In `src/parse/parser.rs`, two changes:
  - **Array literal at expression-atom position**: extend `parse_atom`. When seeing `LBracket`, consume it, parse comma-separated expressions (using `parse_expr(0)` per element), expect `RBracket`. Build `Expr::ArrayLit { elements, span: lbracket_span.merge(rbracket_span) }`. Place the new arm BEFORE the existing primary-expression parsing so `[1, 2, 3]` is recognized as a literal.
  - **Array type annotation in `parse_type`**: extend `parse_type`. When seeing `LBracket`, consume it, parse inner type via recursive `parse_type()`, expect `Semi`, expect `Int(n, _)` literal (reject negative or non-literal forms with a parse error: `"array size must be a non-negative integer literal"`), expect `RBracket`. Build `Type::Array { inner: Box::new(inner), size: n as u64, span: lbracket_span.merge(rbracket_span) }`.

- [X] T005 In `src/resolve.rs`, add traversal for `Expr::ArrayLit { elements, .. }` — done.

- [X] T006 In `src/typeck.rs`, add `Ty::Array(Box<Ty>, u64)` variant. Update:
  - `Ty::name(&self)` → render as `format!("[{}; {}]", inner.name(), size)`.
  - `Ty::is_copy(&self)` → return `inner.is_copy()` for `Array`. In M07.3 elements are primitives, so always true.
  - `ty_from_ast` → add arm for `Type::Array { inner, size, .. }` returning `Ty::Array(Box::new(ty_from_ast(inner)?), *size)`.

- [X] T007 In `src/event.rs`, add `Value::Array { elements, elem_ty }` variant + type_name "[]" — done.

- [X] T008 In `src/ui.rs`, scaffold the inline-cells UI:
  - Add `pub struct InlineCellsView { pub size: u32, pub used: u32, pub elements: Vec<String> }` with the standard derives.
  - Add `pub inline_cells: Option<InlineCellsView>` field to `SlotRowView` with `#[serde(default, skip_serializing_if = "Option::is_none")]`.

**Checkpoint**: `cargo build` should compile cleanly. Match-exhaustiveness will flag any `Ty` or `Value` sites that need `Array` arms — fix them as warnings appear (typically `ty_size_bytes`, `value_size_bytes`, `render_value`, `render_value_for_note`). `cargo test` passes: M01/M02/M03 byte-identical (no existing sample constructs Array). The Ty/Value/AST scaffolding is in place; no user-facing behavior yet.

---

## Phase 3: User Story 1 — Stack-allocated array literal (Priority: P1)

**Goal**: `let t = [10, 20, 30];` typechecks as `[i32; 3]`; produces a `Value::Array { elements: [10, 20, 30], elem_ty: I32 }` written to t's slot; the trace contains **zero** `HeapAlloc`/`HeapRealloc`/`HeapFree` events; the page renders t's slot with 12 inline byte-cells (3 elements × 4 bytes).

**Independent Test**: load `m07_3_array_basic.rs`, step past `let t = [10, 20, 30]`, observe inline cells in t's slot, no heap activity. Step past `let n = t.len()`, observe `n: u64 = 3_u64`.

### Implementation

- [X] T009 [US1] In `src/typeck.rs`, typecheck `Expr::ArrayLit`:
  - If `elements.is_empty()` → typeck error "cannot infer element type for empty array literal — add a type annotation like `let t: [i32; 0] = [];`" with span on the literal.
  - Typecheck the first element to get baseline `T`.
  - For each remaining element, typecheck and attempt `try_coerce_to(expr, ty, T)` to handle literal-narrowing. On failure, error "array elements must all have the same type, found `<other>` (expected `<T>`)" with span on the mismatched element.
  - Result type: `Ty::Array(Box::new(T), elements.len() as u64)`.

- [X] T010 [US1] In `src/typeck.rs`, extend let-annotation handling — the existing annotation-vs-RHS type-equality check (`Ty::Array` equality compares both element type AND size) naturally catches `[i32; 3] = [1, 2]` length mismatches. Verified with `let t: [i32; 3] = [1, 2];` → typeck error "expected [i32; 3], found [i32; 2]" (existing mismatch wording from typecheck_let). No additional code needed.

- [X] T011 [US1] In `src/typeck.rs`, extend method dispatch with `(Ty::Array(_, _), "len") -> u64` — done (merged with existing Slice/Str arm).

- [X] T012 [US1] In `src/eval.rs`, evaluate `Expr::ArrayLit { elements, .. }`:
  - Look up the literal's expression type from `self.types.expr_types[&expr.span()]` to get `Ty::Array(elem_ty, _)`.
  - Eval each element expression into a `Value`, collect into `Vec<Value>`.
  - Return `Value::Array { elements, elem_ty: (*elem_ty).clone() }`.

- [X] T013 [US1] In `src/eval.rs`, fix the `ty_size_bytes` + `value_size_bytes` cascades for Array — done.

- [X] T014 [US1] In `src/eval.rs`, extend `eval_method_call` with `(Value::Array, "len")` — done.

- [X] T015 [US1] In `src/ui.rs`, in `apply_event` SlotWrite arm: populate `inline_cells` from `Value::Array` — done. LiveSlot + SlotRowView threaded with the new field.
  - `size = elements.len() * ty_size_bytes_ui(elem_ty)` (use the existing or import `ty_size_bytes` helper).
  - `used = size` (arrays are always fully populated).
  - `elements = elements.iter().map(render_value).collect()` (per-element display strings).
  - Suppress the text `value` field (set to empty string or None).
  - The existing `Value::Box/Vec/String/Slice` empty-string suppression pattern already exists; extend with Array.

- [X] T016 [US1] Web-side: render inline byte-cells in stack slots when SlotRowView has `inline_cells` — done. JS extension to renderStacks + CSS for `.stack-inline-cells` (gray-tinted) + `.stack-elem-labels`.
  - `web/index.js`: in `renderStacks()` (or wherever slot rows are rendered), when `slot.inline_cells` is present, build a `<div class="stack-inline-cells">` containing one `<span class="byte-cell byte-used">` per byte (matching `slot.inline_cells.size`). For per-element labels, build a `<div class="stack-elem-labels">` with one `<span class="elem-cell" data-elem-idx="i">` per element (drives hover-highlight). Add `data-slot-id` to the parent slot row element if not already present.
  - `web/style.css`: add `.stack-inline-cells` styling — `display: flex; gap: 1px;` plus gray-tinted byte-cell background (e.g. `.stack-inline-cells .byte-cell { background: #d8d6d2; border-color: #b5b5b3; }` for filled; lighter for unused). Add `.stack-elem-labels` styling — `display: flex; gap: 0.5rem; font-family: monospace; font-size: 11px;`.

- [X] T017 [US1] In `src/pipeline.rs::tests`, add ≥ 2 tests for US1:
  - `run_pipeline_array_basic` — `fn main() { let t = [10, 20, 30]; let n = t.len(); }` → SlotWrite of `Value::Array { elements: [Int{30}, Int{20}, Int{10}-ish], elem_ty: Int(I32) }` (3 elements) AND SlotWrite of `n = Value::Int { kind: U64, bits: 3 }`.
  - `run_pipeline_array_no_heap` — same source — assert **zero** events match `HeapAlloc { .. } | HeapRealloc { .. } | HeapFree { .. }`. This is the structural pedagogy gate.

**Checkpoint**: array literal renders inline in stack slot; `t.len()` returns 3; heap panel stays empty.

---

## Phase 4: User Story 2 — Indexing an array (Priority: P1)

**Goal**: `let t = [10, 20, 30]; let x = t[1];` evaluates `x = 20_i32`. Out-of-bounds index fires `Note { RuntimeError }`.

**Independent Test**: load `m07_3_array_index.rs`, step past `let x = t[1]`, observe `x: i32 = 20_i32`.

### Implementation

- [X] T018 [US2] In `src/typeck.rs`, extend `Expr::Index` typecheck for `Ty::Array(T, _)` receivers — done.

- [X] T019 [US2] In `src/eval.rs`, extend `eval_index` for `Value::Array { elements, .. }` — done.

- [X] T020 [US2] In `src/pipeline.rs::tests`, add ≥ 2 tests for US2 — done.
  - `run_pipeline_array_index` — `fn main() { let t = [10, 20, 30]; let x = t[1]; }` → SlotWrite of `x = Value::Int { kind: I32, bits: 20 }`.
  - `run_pipeline_array_index_oob` — `fn main() { let t = [10, 20]; let x = t[5]; }` → `Note { RuntimeError }` with message containing "index out of bounds: array len is 2".

**Checkpoint**: array indexing works; OOB fires runtime error.

---

## Phase 5: User Story 3 — Slicing an array (Priority: P1)

**Goal**: `let t = [1, 2, 3, 4]; let s = &t[1..3];` produces `Value::Slice { target: Pointee::Slot(t_slot), len: 2, byte_offset: 4, byte_len: 8, .. }`; renders a blue slice arrow from `s`'s slot to `t`'s slot (slot-to-slot routing); hover reveals `[len: 2]` + highlights covered cells/elements.

**Independent Test**: load `m07_3_array_slice.rs`, step past `let s = &t[1..3]`, observe blue slice arrow from `s` to `t` slot, hover shows `[len: 2]` + cells 4-11 of `t` light up.

### Implementation

- [X] T021 [US3] In `src/typeck.rs`, extend `typecheck_slice_borrow` for `Ty::Array(T, _)` receivers — done.

- [X] T022 [US3] In `src/eval.rs`, extend `eval_slice_borrow` for `Value::Array` receivers + skip BorrowShared/BorrowEnd for Slot targets — done.

- [X] T023 [US3] In `src/ui.rs`, verify lazy-materialization for Slot-target slices — already works (the match in SlotWrite slice arm constructs `BorrowTarget::Slot(_)`).

- [X] T024 [US3] In `web/index.js`, extend hover-highlight resolver to handle Slot targets via `.stack-inline-cells` / `.stack-elem-labels` — done.

- [X] T025 [US3] In `src/pipeline.rs::tests`, add ≥ 2 tests for US3 — done.
  - `run_pipeline_array_slice` — `fn main() { let t = [1, 2, 3, 4]; let s = &t[1..3]; }` → SlotWrite of `s = Value::Slice { target: Pointee::Slot(_), start: 1, len: 2, byte_offset: 4, byte_len: 8, mutable: false, .. }`. Also assert zero `BorrowShared` events with `Pointee::Slot` target (skipped per M07.2 pattern).
  - `run_pipeline_array_slice_oob` — `fn main() { let t = [1, 2]; let s = &t[0..5]; }` → `Note { RuntimeError }` with message containing "slice end out of bounds" (M07.1 wording reused).

**Checkpoint**: slice arrows render slot-to-slot; hover highlights the covered byte-cells AND element labels in the source slot. All three P1 user stories functionally complete.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: samples + dropdown + warnings + bundle + audit + stage.

- [X] T026 [P] Create 3 M07.3 sample pairs (6 files total). Identical content in `tests/samples/` and `web/samples/`:

  - `m07_3_array_basic.rs`:
    ```rust
    fn main() {
        let t = [10, 20, 30];
        let n = t.len();
    }
    ```
  - `m07_3_array_index.rs`:
    ```rust
    fn main() {
        let t = [10, 20, 30];
        let x = t[1];
    }
    ```
  - `m07_3_array_slice.rs`:
    ```rust
    fn main() {
        let t = [1, 2, 3, 4];
        let s = &t[1..3];
    }
    ```

- [X] T027 [P] In `web/index.html`, add 3 new `<option>` entries to the sample dropdown after the M07.2 group — done.

  ```html
  <option value="m07_3_array_basic">Array basic (M07.3)</option>
  <option value="m07_3_array_index">Array index (M07.3)</option>
  <option value="m07_3_array_slice">Array slice (M07.3)</option>
  ```

- [X] T028 [P] Verify SC-008 + SC-009 → raw WASM 294,655 B (+5.0% vs M07.2 baseline 280,519 B); -D warnings clean for host + WASM.
  - `RUSTFLAGS="-D warnings" cargo build --release` — clean.
  - `RUSTFLAGS="-D warnings" cargo test --release` — full test suite clean (~125 tests post-M07.3).
  - `cargo build --release --target wasm32-unknown-unknown` — WASM clean.
  - `stat -c%s target/wasm32-unknown-unknown/release/rustviz.wasm` — should be within the +15% ceiling.

- [X] T029 Final clean verification — done; 125 tests pass; 0 warnings; WASM clean from scratch.

- [X] T030 Append post-implementation audit log to `specs/014-m07-3-arrays/checklists/requirements.md` — done.

- [X] T031 Stage all changed files — done; 27 files staged (10 modified + 17 added). No commit per UI-QA-split convention.

  ```bash
  git add Cargo.toml Cargo.lock \
          src/parse/ast.rs src/parse/parser.rs \
          src/resolve.rs src/typeck.rs src/event.rs src/eval.rs src/ui.rs src/pipeline.rs \
          tests/samples/m07_3_*.rs web/samples/m07_3_*.rs \
          web/index.html web/index.js web/style.css \
          specs/004-m03-event-eval/contracts/m03-api.md specs/014-m07-3-arrays/ \
          CLAUDE.md
  ```

  Run `git status` and report. **Do not commit** — maintainer's QA pass happens between stage and commit per the UI QA-split convention.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: no dependencies.
- **Phase 2 (Foundational)**: T002 parallel to T003. Then T003 → T004 → T005 sequential (AST → parser → resolver, each builds on prior). T006 / T007 / T008 in three different files — can be done [P] in parallel after T005.
- **Phase 3 (US1 array literal + inline cells)**: depends on Phase 2 complete. T009 → T010 → T011 → T012 → T013 → T014 sequential (typeck pieces → eval pieces). T015 → T016 sequential (UI scaffolding → JS rendering). T017 verifies.
- **Phase 4 (US2 indexing)**: depends on Phase 3 (uses `Value::Array` from US1's eval). T018 → T019 → T020 sequential.
- **Phase 5 (US3 slicing)**: depends on Phase 3. T021 → T022 → T023 → T024 → T025 sequential.
- **Phase 6 (Polish)**: depends on all prior. T026 / T027 / T028 parallel; T029 → T030 → T031 sequential.

### Story-Level Dependencies

- **US1 (array literal + len)** is the foundational user story — establishes `Value::Array`, inline byte-cell rendering, and the zero-heap-event guarantee. Everything else builds on it.
- **US2 (indexing)** depends on US1's eval. Adds the Array arm to `eval_index`.
- **US3 (slicing)** depends on US1. Adds the Array arm to `typecheck_slice_borrow` + `eval_slice_borrow`, exercises the `Pointee::Slot(_)` target on `Value::Slice`.

### Parallel Opportunities

- **T002 + T003**: M03 contract amend vs. AST additions. Different files. [P] ✓
- **T006 + T007 + T008**: Ty::Array in typeck.rs, Value::Array in event.rs, InlineCellsView in ui.rs. Three different files; all additive. [P] ✓
- **T026 + T027 + T028**: sample files vs. dropdown HTML vs. read-only audits. [P] ✓

---

## Parallel Example: Phase 2 additive variants

```bash
# Three independent additive changes in parallel after T005:
Task T006: "Add Ty::Array variant in src/typeck.rs"
Task T007: "Add Value::Array variant in src/event.rs"
Task T008: "Add InlineCellsView + SlotRowView field in src/ui.rs"
```

## Parallel Example: Phase 6 polish

```bash
# Three independent polish tasks in parallel:
Task T026: "Create 3 m07_3_*.rs sample pairs (tests/ + web/)"
Task T027: "Add 3 dropdown entries in web/index.html"
Task T028: "Run warnings + bundle size audits (read-only)"
```

---

## Implementation Strategy

### MVP First (US1 array literal alone)

1. **Phase 1** (T001): pre-flight.
2. **Phase 2** (T002–T008): foundational. AST + parser + Ty/Value/SlotRowView additions.
3. **Phase 3** (T009–T017): array literal + inline-cell rendering + `t.len()`.
4. **STOP and VALIDATE**: `cargo test` passes; `let t = [10, 20, 30]` renders inline cells in t's slot; `t.len()` returns 3; heap panel stays empty.

US2 (indexing) is a small extension to `eval_index`. US3 (slicing) extends `typecheck_slice_borrow` + `eval_slice_borrow` and exercises the slot-target slice path. Both are small follow-ups.

### Single-Agent Strategy

1. T001 → T002 → T003 → T004 → T005 → T006 → T007 → T008 (Phase 1 + 2 sequential).
2. T009 → T010 → T011 → T012 → T013 → T014 → T015 → T016 → T017 (US1 sequential).
3. T018 → T019 → T020 (US2 sequential).
4. T021 → T022 → T023 → T024 → T025 (US3 sequential).
5. T026 + T027 + T028 (parallel polish), T029 → T030 → T031 (sequential close).

---

## Notes

- [P] tasks = different files, no incomplete-task dependencies.
- [Story] tag mandatory on user-story phases.
- **No new Rust deps**. No new JS deps. **No `Cargo.toml` changes**.
- **No new MemEvent variants** — `SlotAlloc` + `SlotWrite` carry array values.
- **No new Pointee variants** — `Slot` (M03) finally exercised by `Value::Slice` targets.
- **No restructure of any existing variant** — `Ty::Array` and `Value::Array` are pure additive.
- **M01/M02/M03 byte-identical expected** — additive variants don't change existing variants' Debug output. If snapshots drift, investigate.
- **Slot-target slice lazy materialization**: reuses M07.2's Static-target pattern in `apply_event` SlotWrite arm. Likely no eval-side `BorrowShared`/`BorrowEnd` emission needed for Slot targets either.
- **Inline byte-cell rendering** is the headline UI addition. Visually distinct from heap cells (gray-tint vs blue).
- **Zero heap events** for array-only programs is the key pedagogical signal. Verified by `run_pipeline_array_no_heap` (T017).
- **Bundle-size budget +15%** from M07.2 baseline (~280 KB raw). Small additive surface should fit easily.
- **Sized M** per the rubric: ~4 source modules + minor JS/CSS + 3 sample pairs + ≥ 6 new tests. ~500-700 LOC net change.
- **Maintainer QA between stage and commit** — same pattern as prior milestones.
- **Closes the slice trilogy**: `Value::Slice { target: Pointee::Slot(_) }` finally constructed. After M07.3 the slice abstraction covers all three Rust memory regions (Stack/Heap/Static = Slot/Heap/Static).
- Avoid: repeat syntax `[v; N]` / multi-dimensional arrays / non-Copy elements / mutation through index / iterator methods / slicing temporaries / const generics in size. All explicitly deferred per spec.
