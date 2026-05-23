---

description: "Task list for M07.1 — Slices (`&[T]`, range indexing, fat pointers)"
---

# Tasks: M07.1 — Slices (`&[T]`, range indexing, fat pointers)

**Input**: Design documents from `/specs/012-m07-1-slices/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/m07-1-protocol-delta.md ✓, quickstart.md ✓

**Tests**: M01/M02/M03 should stay byte-identical (no existing sample constructs `Ty::Slice` or `Value::Slice`). New `cargo test --lib pipeline::tests` covering range parsing, slice typing, range-indexed borrow producing slice, `Slice::len()`, OOB ranges, slice dangling after realloc, all four range forms, mutable-slice rejection. ≥ 7 new tests. Manual M07.1 QA per the SC-008 procedure.

**Organization**: 3 user stories (all P1). Sized L on the small end. ~5 source files modified + minor JS/CSS for length-annotation rendering + 3 sample pairs. **Smallest milestone since M06.1.**

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1/US2/US3 tag, mandatory on user-story phases only
- File paths relative to repo root

## Path Conventions

~5 existing source files modified + small JS/CSS additions in `web/` + 3 sample pairs. See `specs/012-m07-1-slices/plan.md` Project Structure.

---

## Phase 1: Setup

**Purpose**: pre-flight — confirm starting state.

- [X] T001 Verify pre-conditions: branch `012-m07-1-slices` checked out; `cargo test` from `main` passes (post-M07 test count); M07 page loads — heap panel renders Box/Vec/String, owning arrows display, Vec realloc dangling-borrow detection fires. No code change in this task. → 102 tests pass; branch confirmed.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: lexer token + AST Range/Slice + Ty::Slice + Value::Slice + ArrowView `len` field. Required by all three user stories. **Smaller than M07's Phase 2** — no cascade refactor, all additions are additive variants.

- [X] T002 Amend M03's contract in `specs/004-m03-event-eval/contracts/m03-api.md`: note M07.1 as the 5th invocation of the closed-enum-with-revisions rule. Pure additive: `Ty::Slice(Box<Ty>)`, `Value::Slice { borrow_id, target: Pointee, len: u64, mutable: bool }`. No event-variant changes, no restructure of existing variants. Cross-reference `specs/012-m07-1-slices/contracts/m07-1-protocol-delta.md`.

- [X] T003 In `src/parse/token.rs`, add `TokenKind::DotDot` variant and its `describe()` string (`"`..`"`). In `src/parse/lexer.rs`, lex `..` as `DotDot` via two-char lookahead in the punctuation arm — after consuming the first `.`, peek the next char; if `.`, advance and emit `DotDot`. Otherwise the existing `Dot` token fires. Place the new arm BEFORE the existing `Dot` arm so the two-char form wins. Add a brief comment noting the float-literal arm's greedy `digit.digit` consumption prevents conflict with `1.0..2.0` etc.

- [X] T004 In `src/parse/ast.rs`, add one `Expr` variant and one `Type` variant:
  - `Expr::Range { start: Option<Box<Expr>>, end: Option<Box<Expr>>, span }`.
  - `Type::Slice { inner: Box<Type>, mutable: bool, span }`.
  - Update `Expr::span()` to handle the new `Range` arm.
  - Update `parser::type_span()` (or equivalent) to handle `Type::Slice`.

- [X] T005 In `src/resolve.rs`, add traversal for the new AST nodes:
  - `Expr::Range { start, end, .. }` → resolve each present bound (recurse on `start.as_deref()` and `end.as_deref()`).
  - No new BindingIds (range bounds are scalar expressions).

- [X] T006 In `src/parse/parser.rs`, two changes:
  - **Slice-type annotation**: extend `parse_type`. After seeing `Amp` or `AmpMut`, peek the next token; if `LBracket`, consume it, parse inner type, expect `RBracket`, return `Type::Slice { inner, mutable, span }` (with mutable based on the token consumed). Otherwise fall back to the existing `Type::Ref` path.
  - **Range parsing inside `[ ]`**: factor the inside-brackets logic into `parse_index_inner` (or extend the existing index parser):
    1. Peek the next token. If `DotDot` → consume; `start = None`. Then: if `RBracket`, `end = None` → return `Expr::Range { start: None, end: None, span }`. Else parse `end` expression, expect `RBracket` → return `Expr::Range { start: None, end: Some(end), span }`.
    2. Else parse a primary expression (potential `start`). Peek:
       - If `RBracket` → scalar index path (existing behavior): return `Expr::Index { index: <first>, .. }`.
       - If `DotDot` → consume; peek `RBracket` → return `Range { start: Some(first), end: None, .. }`; else parse `end` expression, expect `RBracket` → return `Range { start: Some(first), end: Some(end), .. }`.
  - Range is ONLY accepted inside `[ ]` — `parse_expr` does NOT recognize `..` at expression-start or as an infix operator. Standalone `..` outside index produces a parse error: `"unexpected `..` (range expressions are only valid inside `[ ]` in M07.1)"`.

- [X] T007 In `src/typeck.rs`, add `Ty::Slice(Box<Ty>)` variant. Update `Ty::name(&self)` to render as `"&[<inner_name>]"` (always with leading `&` since slice is reference-shaped). Update `Ty::is_copy(&self)` to return `false` for `Ty::Slice(_)`. Update `ty_from_ast` to handle `Type::Slice { inner, mutable, .. }`:
  - If `mutable: true` → return typeck error `"mutable slices are out of scope in M07.1 — only &[T] (shared) is supported"`.
  - Otherwise → recurse on `inner` to get the element Ty, return `Ty::Slice(Box::new(inner_ty))`.

- [X] T008 In `src/event.rs`, add `Value::Slice { borrow_id: BorrowId, target: Pointee, len: u64, mutable: bool }` variant. Update `Value::type_name(&self)` to return `"&[]"` for `Value::Slice { .. }` (short tag; full type comes from the Ty layer).

- [X] T009 In `src/ui.rs`, extend `ArrowView` with `len: Option<u64>` field, defaulting to `None`. Use `#[serde(default, skip_serializing_if = "Option::is_none")]` so non-slice arrows omit the field in JSON output (wire-format backwards-compat). Update `World.arrows` (the in-memory equivalent) with the same field. Existing borrow/owning-arrow construction sites set `len: None` explicitly.

**Checkpoint**: `cargo build` clean ✓. `cargo test` 102 passed (byte-identical M01/M02/M03) ✓. The Ty/Value/AST scaffolding is in place; no user-facing behavior yet.

---

## Phase 3: User Story 1 — Partial-range slice (Priority: P1)

**Goal**: `let s = &v[1..3];` typechecks; produces a `Value::Slice` with `len: 2`; emits a `BorrowShared` event targeting the Vec's heap addr; the page renders a blue arrow with `[len: 2]` annotation.

**Independent Test**: load `m07_1_slice_range.rs`, step past the slice-binding step, observe blue arrow with `[len: 2]` annotation pointing at the Vec's heap block.

### Implementation

- [X] T010 [US1] In `src/typeck.rs`, typecheck `Expr::Range`:
  - Each present bound (`start`, `end`) must typecheck to `Ty::Int(_)` (any integer kind). Reject non-integer bounds with `"range bound must be integer, found {ty_name}"` at the bound's span.
  - Range type itself: there is no first-class "range type" in M07.1 — `Expr::Range` is only valid as an `Expr::Index.index`. To enforce: typeck threads a flag `in_index_position: bool` through recursive typecheck calls. When typeck-ing an `Expr::Index { index, .. }`, set the flag true for `index` and false otherwise. When typeck-ing `Expr::Range` with the flag false, error: `"range expressions are only valid inside index brackets in M07.1"`. With the flag true, return a sentinel `Ty::Unit` (the actual result type is computed by `Expr::Index`'s typeck below).

- [X] T011 [US1] In `src/typeck.rs`, extend `Expr::Index` typecheck:
  - Existing scalar-index path: `receiver: Ty::Vec(T)` + `index: Ty::Int(_)` → returns `T`. Unchanged.
  - NEW range-index path: when `index` is `Expr::Range { .. }`, after typecheck-ing the receiver (`Ty::Vec(T)`) and the range bounds, return `Ty::Slice(T.clone())`. Result type signals to `Expr::Borrow`'s parent that this is the slice-producing variant.

- [X] T012 [US1] In `src/typeck.rs`, extend `Expr::Borrow` typecheck:
  - When `inner` is `Expr::Index { index: Expr::Range(_), .. }`:
    - If `mutable: true` → typeck error: `"mutable slices are out of scope in M07.1 — only &[T] (shared) is supported"`.
    - Otherwise → typecheck `inner` (which returns `Ty::Slice(T)` from T011), and **return `Ty::Slice(T)` directly** — do NOT wrap in `Ty::Ref { inner: Ty::Slice(T), .. }`. The leading `&` is absorbed into the slice type. Add a comment explaining this peephole rule.
  - When `inner` is anything else: existing M06 path (returns `Ty::Ref { inner: T, mutable }`).

- [X] T013 [US1] In `src/eval.rs`, evaluate `Expr::Index { receiver, index: Expr::Range(start_opt, end_opt), span }`:
  - Eval receiver to `Value::Vec { addr }`. Look up `HeapObject::Vec` to get `elements.len() as i128`.
  - Compute concrete start/end as `i128`: start defaults to 0, end defaults to vec_len.
  - Bounds check (emit `Note { RuntimeError }` + halt on failure):
    - `start < 0 || start > vec_len` → `"slice start out of bounds: start is {start}, vec len is {vec_len}"`
    - `end < 0 || end > vec_len` → `"slice end out of bounds: end is {end}, vec len is {vec_len}"`
    - `start > end` → `"slice start > end: start is {start}, end is {end}"`
  - On success: allocate a fresh `BorrowId`. Emit `MemEvent::BorrowShared { borrow_id, target: Pointee::Heap(addr), span }`. Register the borrow in the eval-side active-borrows registry (same machinery M07 uses for `&v[0]`) so the dangling-detection scan catches it on later realloc.
  - Return `Value::Slice { borrow_id, target: Pointee::Heap(addr), len: (end - start) as u64, mutable: false }`.

- [X] T014 [US1] In `src/ui.rs::apply_event`, extend `SlotWrite` handling: when the written value is `Value::Slice { borrow_id, target, len, mutable }`, register an arrow in `world.arrows`:
  - `kind: Shared` (mutable: false in M07.1)
  - `target: ArrowTarget::Heap(addr)` extracted from `Pointee::Heap(addr)` (M07.1 doesn't construct slot-target slices)
  - `len: Some(len)`
  - `source_slot: slot_id.0`
  Update `render_value` for `Value::Slice { .. }`: return `""` (empty string) — the arrow IS the visualization, matching the M07 Box/Vec/String convention.

- [X] T015 [US1] Web-side wiring for length-annotation rendering:
  - `web/index.js`: in `renderArrows()`, after appending the SVG `<path>` for each arrow, if `arrow.len !== undefined` (Some), create a `<text class="arrow-len-label">` element with content `[len: ${arrow.len}]`. Position it at the arrow's mid-point along the routed path. For arrows routed in a lane above the heap row (typical for slice arrows), the label sits in the same lane, offset by ~2px above the arrow's mid-X. Append the text element to the SVG overlay.
  - `web/style.css`: add `.arrow-len-label { font-size: 10px; fill: #4d8fcd; font-family: monospace; user-select: none; pointer-events: none; }`. The blue color matches the shared-borrow arrow color.

- [X] T016 [US1] In `src/pipeline.rs::tests`, add ≥ 3 tests for US1:
  - `run_pipeline_slice_range` — `let mut v: Vec<i32> = Vec::new(); v.push(10); v.push(20); v.push(30); v.push(40); let s = &v[1..3];` — trace contains exactly one `BorrowShared` event with `target: Pointee::Heap(_)`; the s-slot's value is `Value::Slice { len: 2, .. }`.
  - `run_pipeline_slice_oob_end` — `let mut v: Vec<i32> = Vec::new(); v.push(1); let s = &v[0..5];` — emits a `Note { RuntimeError }` with "slice end out of bounds" message.
  - `run_pipeline_slice_oob_start_gt_end` — `let mut v: Vec<i32> = Vec::new(); v.push(1); v.push(2); let s = &v[2..1];` — emits a `Note { RuntimeError }` with "slice start > end" message.

**Checkpoint**: partial-range slices typecheck, evaluate, and render with `[len: 2]` annotation visible. `cargo test` 105 passed (was 102, +3 US1 tests).

---

## Phase 4: User Story 2 — Full-vec slice + `s.len()` (Priority: P1)

**Goal**: `let s = &v[..]; let n = s.len();` works; all four range forms parse + typecheck; the slice's length method returns `u64`.

**Independent Test**: load `m07_1_slice_basic.rs`, step through, observe full-Vec slice arrow with `[len: 3]` annotation and `s.len()` returning `3_u64`.

### Implementation

- [X] T017 [US2] In `src/typeck.rs`, extend the method dispatch table with one row:
  - `(Ty::Slice(_), "len") → signature: (&self) -> Ty::Int(IntKind::U64)`.
  The existing `Vec::len` row stays. Dispatcher matches on `(receiver_ty, name)`; add the Slice arm.

- [X] T018 [US2] In `src/eval.rs`, evaluate `s.len()` for slice receivers:
  - In the method-call eval arm, when receiver evaluates to `Value::Slice { len, .. }`, return `Value::Int { kind: IntKind::U64, bits: len as i128 }`. Add adjacent to the existing `Vec::len` arm.

- [X] T019 [US2] In `src/pipeline.rs::tests`, add ≥ 2 tests for US2:
  - `run_pipeline_slice_basic` — `let mut v: Vec<i32> = Vec::new(); v.push(1); v.push(2); v.push(3); let s = &v[..]; let n = s.len();` — `s`'s value is `Value::Slice { len: 3, .. }`; `n`'s value is `Value::Int { kind: U64, bits: 3 }`.
  - `run_pipeline_slice_all_forms` — `let mut v: Vec<i32> = Vec::new(); v.push(1); v.push(2); v.push(3); let a = &v[..]; let b = &v[1..]; let c = &v[..2]; let d = &v[0..2];` — produces four slice values with len 3, 2, 2, 2 respectively. Verifies all four range forms.

**Checkpoint**: full-vec slices work; `s.len()` returns `u64`; all four range forms parse + typecheck. 107 tests pass.

---

## Phase 5: User Story 3 — Slice dangles after Vec realloc (Priority: P1)

**Goal**: a slice taken before a Vec realloc triggers a `Note { RuntimeError }` at the realloc step, same pedagogy as M07's `&v[0]`-after-`push` case.

**Independent Test**: load `m07_1_slice_dangling.rs`, step past the realloc-triggering push, observe RuntimeError note on the slice-binding span.

### Implementation

- [X] T020 [US3] Verify M07's existing dangling-detection scan in `realloc_heap` catches slice borrows. **Finding**: M07's scan inspects `Value::Ref { target: Pointee::Heap(_) }` directly on locals (NOT via a separate registry). Extended the match to also handle `Value::Slice { target: Pointee::Heap(_) }` in `src/eval.rs::realloc_heap`. One-arm addition; same span source (`local.decl_span`), same error message.

- [X] T021 [US3] In `src/pipeline.rs::tests`, add ≥ 1 test for US3:
  - `run_pipeline_slice_dangling` — `let mut v: Vec<i32> = Vec::new(); v.push(1); v.push(2); let s = &v[..]; v.push(3);` — at the third push (capacity 2 → 4 realloc), expect a `Note { RuntimeError }` event with message containing "dangling reference" and span on the `&v[..]` source position.

**Checkpoint**: slice dangling fires the same RuntimeError pedagogy as single-element borrows. All three P1 user stories functionally complete. 108 tests pass (was 102; +6 M07.1 tests).

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: samples + dropdown + mutable-slice rejection test + warnings + bundle + audit + stage.

- [X] T022 [P] Create 3 M07.1 sample pairs (6 files total). Identical content in `tests/samples/` and `web/samples/`:

  - `m07_1_slice_basic.rs`:
    ```rust
    fn main() {
        let mut v: Vec<i32> = Vec::new();
        v.push(1);
        v.push(2);
        v.push(3);
        let s = &v[..];
        let n = s.len();
    }
    ```
  - `m07_1_slice_range.rs`:
    ```rust
    fn main() {
        let mut v: Vec<i32> = Vec::new();
        v.push(10);
        v.push(20);
        v.push(30);
        v.push(40);
        let s = &v[1..3];
    }
    ```
  - `m07_1_slice_dangling.rs`:
    ```rust
    fn main() {
        let mut v: Vec<i32> = Vec::new();
        v.push(1);
        v.push(2);
        let s = &v[..];
        v.push(3);
    }
    ```

- [X] T023 [P] In `web/index.html`, add 3 new `<option>` entries to the sample dropdown after the M07 group:

  ```html
  <option value="m07_1_slice_basic">Slice basic (M07.1)</option>
  <option value="m07_1_slice_range">Slice range (M07.1)</option>
  <option value="m07_1_slice_dangling">Slice dangling (M07.1)</option>
  ```

- [X] T024 [P] In `src/pipeline.rs::tests`, add ≥ 1 typeck-rejection test:
  - `run_pipeline_mut_slice_rejected` — `let mut v: Vec<i32> = Vec::new(); v.push(1); let s = &mut v[..];` — typeck error containing "mutable slices are out of scope in M07.1".
  - `run_pipeline_standalone_range_rejected` — `let r = 1..3;` — typeck error containing "range expressions are only valid inside index brackets in M07.1".

- [X] T025 [P] Verify SC-008 (bundle size ≤ +25% vs M07 baseline 905,170 B uncompressed → ≤ 1,131,463 B) AND SC-009 (zero warnings): → raw WASM 273,852 B, gzipped 103,302 B; -D warnings clean for host + WASM.
  - `RUSTFLAGS="-D warnings" cargo build --release` — clean.
  - `RUSTFLAGS="-D warnings" cargo test` — full test suite clean.
  - `cargo build --release --target wasm32-unknown-unknown` — WASM clean.
  - `stat -c%s target/wasm32-unknown-unknown/release/rustviz.wasm` — should be ≤ 1,131,463 B uncompressed (and gzipped baseline similarly should fit within +25% of the gzipped M07 baseline). Expected ~950 KB given the small variant + UI additions.

- [X] T026 Final clean verification: `cargo clean && RUSTFLAGS="-D warnings" cargo build --release && RUSTFLAGS="-D warnings" cargo test && cargo build --release --target wasm32-unknown-unknown`. Full pipeline must pass clean from scratch. → 110 tests passed; 0 warnings; WASM builds clean.

- [X] T027 Append post-implementation audit log to `specs/012-m07-1-slices/checklists/requirements.md`. Table covering SC-001 through SC-009. SC-001 / SC-002 (visual rendering checks) deferred to maintainer QA. Document: how many sites needed the dangling-detection scan refactor (T020 — likely 0 or 1), whether the existing borrow registry was already shape-agnostic. Document any M03 snapshot drift (expected: none, since all variants additive). Note any UI bugs observed during manual QA. Audit growth + warnings results.

- [X] T028 Stage all changed files: → 29 files staged (10 modified + 19 added). No commit per UI-QA-split convention.

  ```bash
  git add Cargo.toml Cargo.lock \
          src/parse/token.rs src/parse/lexer.rs src/parse/ast.rs src/parse/parser.rs \
          src/resolve.rs src/typeck.rs src/event.rs src/eval.rs src/ui.rs src/pipeline.rs \
          tests/samples/m07_1_*.rs web/samples/m07_1_*.rs \
          web/index.html web/index.js web/style.css \
          specs/004-m03-event-eval/contracts/m03-api.md specs/012-m07-1-slices/ \
          CLAUDE.md
  ```

  Run `git status` and report. **Do not commit** — maintainer's QA pass happens between stage and commit per the UI QA-split convention.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: no dependencies.
- **Phase 2 (Foundational)**: T002 parallel to T003. Then T003 → T004 → T005 → T006 sequential (token → AST → resolver → parser, each builds on prior). T007 → T008 → T009 sequential (Ty::Slice → Value::Slice → ArrowView.len). T007 / T008 / T009 can also be done [P] (different files, independent additions).
- **Phase 3 (US1 partial-range slice)**: depends on Phase 2 complete. T010 → T011 → T012 → T013 → T014 → T015 → T016 sequential (typeck Range → typeck Index → typeck Borrow → eval → ui → web → tests). T010 → T011 → T012 form one logical typeck flow.
- **Phase 4 (US2 full-vec slice + len)**: depends on Phase 3 (the slice machinery from US1 is reused). T017 → T018 → T019 sequential.
- **Phase 5 (US3 dangling slice)**: depends on Phase 3 (slice borrows must be in the registry). T020 → T021 sequential.
- **Phase 6 (Polish)**: depends on all prior. T022 / T023 / T024 / T025 parallel; T026 → T027 → T028 sequential.

### Story-Level Dependencies

- **US1 (partial-range slice)** is the foundational user story — establishes range parsing, slice typing, slice value, slice borrow, length-annotation rendering. Everything else builds on it.
- **US2 (full-vec slice + `len()`)** depends on US1's slice machinery. Adds the `Slice::len` method dispatch + verifies all four range forms parse.
- **US3 (dangling slice)** depends on US1's slice registration in the borrow registry. Verifies M07's existing dangling-detection catches slices (usually no new code needed).

### Parallel Opportunities

- **T002 + T003**: M03 contract amend vs. lexer token. Different files. [P] ✓
- **T007 + T008 + T009**: Ty::Slice, Value::Slice, ArrowView.len in three different files (typeck.rs, event.rs, ui.rs). All additive, no cross-deps. [P] ✓
- **T022 + T023 + T024 + T025**: sample files vs. dropdown HTML vs. rejection tests vs. read-only audits. [P] ✓

---

## Parallel Example: Phase 2 additive variants

```bash
# Three independent additive variants in parallel:
Task T007: "Add Ty::Slice(Box<Ty>) in src/typeck.rs"
Task T008: "Add Value::Slice variant in src/event.rs"
Task T009: "Add ArrowView.len: Option<u64> in src/ui.rs"
```

## Parallel Example: Phase 6 polish

```bash
# Four independent polish tasks in parallel:
Task T022: "Create 3 m07_1_*.rs sample pairs (tests/ + web/)"
Task T023: "Add 3 dropdown entries in web/index.html"
Task T024: "Add typeck rejection tests in src/pipeline.rs::tests"
Task T025: "Run warnings + bundle size audits (read-only)"
```

---

## Implementation Strategy

### MVP First (US1 partial-range slice alone)

1. **Phase 1** (T001): pre-flight.
2. **Phase 2** (T002–T009): foundational. Lexer + AST + Ty/Value/ArrowView additions.
3. **Phase 3** (T010–T016): partial-range slice — typeck + eval + ui + web + tests.
4. **STOP and VALIDATE**: `cargo test` passes; `let s = &v[1..3];` produces a slice arrow with `[len: 2]` annotation. **At this point the slice infrastructure is shippable** — though US2's `Slice::len` and US3's dangling pedagogy are quick follow-ups.

US2 and US3 are small additions on top of US1 — single-line method dispatch (US2) and verifying-existing-machinery (US3).

### Single-Agent Strategy

1. T001 → T002 → T003 → T004 → T005 → T006 → T007 → T008 → T009 (Phase 1 + 2 sequential).
2. T010 → T011 → T012 → T013 → T014 → T015 → T016 (US1 sequential).
3. T017 → T018 → T019 (US2 sequential).
4. T020 → T021 (US3 sequential).
5. T022 + T023 + T024 + T025 (parallel polish), T026 → T027 → T028 (sequential close).

---

## Notes

- [P] tasks = different files, no incomplete-task dependencies.
- [Story] tag mandatory on user-story phases.
- **No new Rust deps**. No new JS deps. **No `Cargo.toml` changes**.
- **No new MemEvent variants** — slices reuse existing `BorrowShared` / `BorrowEnd` with `Pointee::Heap(_)` targets (M07 already started producing these).
- **No restructure of any existing variant** — `Ty::Slice` and `Value::Slice` are pure additive; ArrowView gets one optional field with serde-skip.
- **M01/M02/M03 byte-identical expected** — additive variants don't change existing variants' Debug output. If snapshots drift, investigate.
- **Slice = parallel variant to Ref, not extension of Ref**: `Value::Slice` and `Value::Ref` are siblings. Slices carry an extra `len` field; renderer dispatches on the variant rather than checking an Option.
- **Leading `&` is absorbed into `Ty::Slice`**: `Ty::Slice(T)` IS `&[T]`. The peephole rule in `Expr::Borrow`'s typeck (T012) short-circuits the normal `Ty::Ref` wrap when the inner is range-indexed.
- **Range AST is single-variant with Option-pair bounds** (T004): forward-compatible with future standalone-range support (`for i in 1..10`).
- **Range parsing only inside `[ ]` in M07.1** (T006): tight scope. Standalone range produces parse error.
- **Standalone Range in AST is typeck-rejected** (T010 in_index_position flag): forward-compatible — when standalone ranges become useful, drop the flag.
- **Mutable slice typeck-rejection** (T012, T024): out of scope; will be dropped when M07.x adds mutable slices.
- **Dangling-detection reuse from M07** (T020): the existing scan should work unchanged IF the borrow registry is shape-agnostic. If not, light refactor to make Value::Slice borrows visible to the scan.
- **Length annotation visual** (T015): inline SVG `<text>` element at the arrow's mid-point. Small blue monospace, matches arrow color.
- **Bundle-size budget +25%** from M07 baseline (905,170 B → ≤ 1,131,463 B uncompressed). Generous because slice infrastructure is mostly typeck + eval work + a few UI lines; should fit easily.
- **Sized L on the small end** per the rubric: ~5 source modules + minor JS/CSS + 3 sample pairs + ≥ 7 new tests. ~500-700 LOC net change.
- **Maintainer QA between stage and commit** — same pattern as prior milestones.
- **Foundational for M07.2** (`&str` + static memory): the slice type, length-annotation visual, and borrow infrastructure will be reused as-is. M07.2 adds the static-memory region + `Pointee::Static` variant on top.
- Avoid: mutable slices / iterator methods / slice methods beyond len() / slicing a slice / standalone ranges / non-Vec receivers / array types. All explicitly deferred per spec.
