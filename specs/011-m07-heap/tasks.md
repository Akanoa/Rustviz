---

description: "Task list for M07 — Level 3: Heap (Box, Vec, String)"
---

# Tasks: M07 — Level 3: Heap (`Box`, `Vec`, `String`)

**Input**: Design documents from `/specs/011-m07-heap/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/m07-protocol-delta.md ✓, quickstart.md ✓

**Tests**: M01/M02/M03 should stay byte-identical (no existing sample constructs heap values or new-shape Value::Ref). New `cargo test --lib pipeline::tests` covering Box, Vec push/realloc/indexing/OOB, dangling-borrow, String push_str — ≥ 10 new tests. Manual M07 QA per the SC-008 procedure.

**Organization**: 3 user stories (US1 + US2 P1, US3 P2). Sized L bordering on XL. ~10 source files + new heap panel + arrow renderer extension + Value::Ref restructure cascade. **Largest milestone since M04.**

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1/US2/US3 tag, mandatory on user-story phases only
- File paths relative to repo root

## Path Conventions

~10 existing source files modified + new heap-related JS/CSS in `web/` + 3 sample pairs. See `specs/011-m07-heap/plan.md` Project Structure.

---

## Phase 1: Setup

**Purpose**: pre-flight — confirm starting state.

- [X] T001 Verify pre-conditions: branch `011-m07-heap` checked out; `cargo test` from `main` passes (94 tests post-M06.1); M06 page loads, borrow arrows render, mutation works. No code change in this task.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: lexer + parser + AST + Ty/Value scaffolding + Value::Ref restructure + heap state model. **Big phase** — required by all three user stories. The Value::Ref restructure is the cascade that touches M06/M06.1 code paths.

- [X] T002 Amend M03's contract in `specs/004-m03-event-eval/contracts/m03-api.md`: note M07 as the 4th invocation of the closed-enum-with-revisions rule. M03.1 added `MemEvent::ReturnValue`; M03.2 restructured `Ty` + `Value` (kind-based); M06 added `Ty::Ref` + `Value::Ref`. M07 adds `Ty::Box/Vec/String`, `Value::Box/Vec/String/Str`, AND restructures `Value::Ref` (`target_slot: SlotId` → `target: Pointee`). Cross-reference `specs/011-m07-heap/contracts/m07-protocol-delta.md`.

- [X] T003 In `src/parse/token.rs`, add `TokenKind::Str(String)`, `ColonColon`, `Dot`, `LBracket`, `RBracket` variants and their `describe()` strings. In `src/parse/lexer.rs`, lex:
  - `"..."` string literals with escapes `\n`, `\t`, `\\`, `\"`; unterminated string is a ParseError. The numeric-literal arm already consumes `.digit` greedily for floats; bare `.` becomes the `Dot` token.
  - `::` (peek next char after `:` for another `:`).
  - `.` standalone (single `Dot`).
  - `[` → `LBracket`, `]` → `RBracket`.

- [X] T004 In `src/parse/ast.rs`, add four `Expr` variants and one `Type` variant:
  - `Expr::StrLit(String, Span)`.
  - `Expr::Path { segments: Vec<String>, span }` (≥ 2 segments).
  - `Expr::MethodCall { receiver: Box<Expr>, name: String, args: Vec<Expr>, span }`.
  - `Expr::Index { receiver: Box<Expr>, index: Box<Expr>, span }`.
  - `Type::Generic { segments: Vec<String>, args: Vec<Type>, span }`.
  - Update `Expr::span()` to include the 4 new variants. Update `parser::type_span()` to include `Type::Generic`.

- [X] T005 In `src/resolve.rs`, add traversal for the new AST nodes:
  - `Expr::StrLit(_, _)` → no-op.
  - `Expr::Path(_, _)` → no-op (paths resolve at typeck via hardcoded tables; no BindingId).
  - `Expr::MethodCall { receiver, args, .. }` → resolve receiver + each arg.
  - `Expr::Index { receiver, index, .. }` → resolve both.

- [X] T006 In `src/parse/parser.rs`, add:
  - String-literal atom: `TokenKind::Str(s)` → `Expr::StrLit(s.clone(), tok.span)`.
  - Multi-segment path: in `parse_atom`, after consuming an Ident, if next is `ColonColon`, loop consuming `:: Ident` to build `segments`; wrap as `Expr::Path` if ≥ 2 segments (else stay as `Expr::Ident`).
  - Postfix `.method(args)`: after `parse_atom`, loop: peek for `Dot`. If found, consume + ident + `(` + comma-separated args + `)`, return `Expr::MethodCall`.
  - Postfix `[expr]`: in same postfix loop, peek for `LBracket`. If found, consume + expr + `RBracket`, return `Expr::Index`.
  - Both postfix forms bind at the same precedence (~bp 90, tighter than binary ops). Chained postfix supported by the loop naturally.
  - In `parse_type`, after parsing an Ident segment, peek for `<`. If present, parse comma-separated types + `>`, return `Type::Generic`.

- [X] T007 In `src/typeck.rs`, add `Ty::Box(Box<Ty>)`, `Ty::Vec(Box<Ty>)`, `Ty::String` variants. Update `Ty::name(&self)` to render as `"Box<i32>"`, `"Vec<i32>"`, `"String"`. Update `Ty::is_copy(&self)` to return `false` for all three. Update `ty_from_ast` to handle `Type::Generic` for `Box<T>` and `Vec<T>` (validate arity = 1, recurse on the arg) and `Type::Path { segments: ["String"], .. }` for `Ty::String`.

- [X] T008 **Cascade refactor**: restructure `Value::Ref` in `src/event.rs` from `Ref { borrow_id, target_slot: SlotId, mutable }` to `Ref { borrow_id, target: Pointee, mutable }`. The `Pointee` enum already exists (Slot(SlotId) | Heap(HeapAddr)). Then fix every consumer:
  - `src/eval.rs::Expr::Borrow` eval arm — construct `Value::Ref { target: Pointee::Slot(slot_id), .. }`.
  - `src/eval.rs::Stmt::Assign` Deref(Ident) lhs — destructure `Value::Ref { target: Pointee::Slot(slot_id), .. }` (for M06.1 case; M07 adds Heap case in T017).
  - `src/eval.rs::Expr::Deref` eval arm — same destructure.
  - `src/ui.rs::apply_event` SlotWrite arm — when value is `Value::Ref { target: Pointee::Slot(slot_id), .. }`, bind source_slot for the matching ActiveBorrowState.
  - `src/ui.rs::render_value` Value::Ref arm — render based on target variant; for Slot use `&x`/`&mut x` lookup (M06.1's existing path); for Heap, emit `&heap` placeholder (refined in T017).
  - `src/pipeline.rs::tests` — update any pattern that destructures `target_slot` to use `target: Pointee::Slot(_)`.
  Estimated ~10-15 sites; mechanical replacement.

- [X] T009 In `src/event.rs`, add `Value::Box { addr: HeapAddr }`, `Value::Vec { addr: HeapAddr }`, `Value::String { addr: HeapAddr }`, `Value::Str(String)` variants. Update `Value::type_name(&self)` to return `"Box"`, `"Vec"`, `"String"`, `"&str"` (no T info available without the heap state — sufficient for the existing pipeline).

- [X] T010 In `src/eval.rs`, add:
  - `struct HeapState { next_addr: u32, objects: IndexMap<HeapAddr, HeapObject> }`.
  - `enum HeapObject { Box(Value), Vec { elements: Vec<Value>, capacity: usize, elem_ty: Ty }, Str { bytes: String, capacity: usize } }`.
  - Add `heap: HeapState` field to `Evaluator` struct + initialize in `Evaluator::new`.
  - Helpers: `alloc_heap(obj, ty_name, span) -> HeapAddr` (emits HeapAlloc, returns new addr); `realloc_heap(old_addr, obj, new_size, span) -> HeapAddr` (emits HeapRealloc, returns new addr; old_addr removed); `free_heap(addr, span)` (emits HeapFree, removes from objects); `get_heap(addr) -> &HeapObject`; `get_heap_mut(addr) -> &mut HeapObject`. Add `next_heap_addr` counter + `alloc_heap_addr()` allocator method.

**Checkpoint**: `cargo build` clean. M01/M02/M03 tests pass (no behavioral change — Value::Ref restructure is wire-format only; existing samples don't construct Box/Vec/String).

---

## Phase 3: User Story 1 — `Box` owning arrow (Priority: P1)

**Goal**: `let b = Box::new(5);` typechecks, emits HeapAlloc + SlotWrite (Value::Box) and at scope-exit emits HeapFree. Heap panel shows a labeled box; a black owning arrow connects `b` to it.

**Independent Test**: load `m07_box.rs`, step through; observe heap box appear with owning arrow, then disappear at scope close.

### Implementation

- [X] T011 [US1] In `src/typeck.rs`, implement path-fn dispatch table. Recognize `Expr::Path { segments: vec!["Box", "new"], .. }` and treat as a callable. When invoked as `Expr::Call { callee: Box::<Expr::Path>, args: vec![v] }`, typecheck the arg to get its type `T`, return `Ty::Box(Box::new(T))`. Reject unknown path patterns with a typeck error. Add a helper `typecheck_path_call(path_segments, args, span) -> Result<Ty, ParseError>` for the dispatch logic.

- [X] T012 [US1] In `src/eval.rs`, evaluate `Box::new(v)`:
  - Detect via the `Expr::Call { callee: Expr::Path { segments: ["Box", "new"], .. }, args, .. }` shape.
  - Eval the single arg to get the inner Value.
  - Call `alloc_heap(HeapObject::Box(value), "Box<...>", span)` to get a fresh HeapAddr (emits HeapAlloc).
  - Return `Value::Box { addr }`.
  - In `drop_current_scope`, BEFORE emitting SlotDrop for non-Copy bindings, look at each local's value: if it's `Value::Box/Vec/String { addr }`, emit `MemEvent::HeapFree { addr, span: local.decl_span }` and remove from heap state.

- [X] T013 [US1] In `src/ui.rs`, restructure for M07's arrows + heap view:
  - **Rename** `BorrowView` → `ArrowView`. Update `StateSnapshot.borrows` → `StateSnapshot.arrows`. Add `kind: ArrowKind { Shared, Mut, Owning }` and `target: ArrowTarget { Slot(u32), Heap(u32) }` fields. Drop the old `mutable: bool` and `target_slot: u32` fields.
  - **`World.borrows`** → `World.arrows`. The struct gets the `kind` + `target` fields too.
  - Add `pub struct HeapView { addr: u32, ty_name: String, display: String, size: u32 }`.
  - Add `pub heap: Vec<HeapView>` field to `StateSnapshot`.
  - Add `heap: Vec<HeapAllocState>` to `World` (struct: addr, ty_name, contents-display string, size).
  - In `apply_event`:
    - `MemEvent::HeapAlloc { addr, size, ty_name, .. }` → push to `world.heap`.
    - `MemEvent::HeapFree { addr, .. }` → remove from `world.heap` by addr.
    - `MemEvent::SlotWrite { slot_id, value: Value::Box/Vec/String { addr }, .. }` → register an owning arrow: push `ArrowView { source_slot: slot_id.0, target: Heap(addr.0), kind: Owning }` (or equivalent state). (Existing SlotWrite of Value::Ref still binds source_slot on the matching active borrow.)
  - Update existing M06 borrow-event apply arms to set `kind = Shared/Mut`.
  - Refresh `state_snapshot()` to populate `arrows` (from `world.arrows`) and `heap` (from `world.heap`).
  - Update `render_value` cases: `Value::Box { addr }` → `"Box → heap{addr}"` or similar; `Value::Vec`/`String` similar.

- [X] T014 [US1] Web-side wiring (HTML + CSS + JS):
  - `web/index.html`: remove `<p class="placeholder">Heap (Level 3+)</p>` from `#heap`. In the `<svg id="arrow-overlay">` `<defs>`, add a third arrowhead marker: `<marker id="arrow-head-owning" ... fill="#000" />`. Dropdown grows 3 entries (added in T025).
  - `web/style.css`: add `.heap-box { display: inline-flex; border: 1px solid var(--frame-border); border-radius: 4px; padding: 0.3rem 0.5rem; background: var(--frame-bg); transition: transform 300ms ease-out, opacity 200ms; }`. Add `#heap { display: flex; flex-wrap: wrap; gap: 0.5rem; padding: 1rem; align-content: flex-start; }`. Add `.arrow-owning { stroke: #000; fill: none; stroke-width: 1.5; marker-end: url(#arrow-head-owning); }`.
  - `web/index.js`:
    - Add `renderHeap(heap)` function: maintains a `heapElements: Map<addr, HTMLElement>`. For each `HeapView` in `state.heap`, create or update the matching DOM element (with `data-heap-addr={addr}`). Remove elements for addrs not in state.heap (HeapFree).
    - Update `renderArrows`: rename `state.borrows` → `state.arrows`. For each arrow, look up source via `data-slot-id` AND target via `data-slot-id` OR `data-heap-addr` based on `arrow.target.Slot` vs `arrow.target.Heap`. Pick CSS class from `arrow.kind` (`arrow-shared`, `arrow-mut`, `arrow-owning`).
    - Call `renderHeap(state.heap)` from `render()`, BEFORE `requestAnimationFrame(renderArrows(...))` so heap DOM exists when arrows query positions.

- [X] T015 [US1] In `src/pipeline.rs::tests`, add ≥ 2 tests:
  - `run_pipeline_box_basic` — `fn main() { let b = Box::new(5); }` — trace contains exactly one HeapAlloc + one HeapFree event.
  - `run_pipeline_box_drop_order` — verify HeapFree fires BEFORE SlotDrop at scope exit (relevant for future M07.x checks).

**Checkpoint**: Box samples render heap boxes with black owning arrows in the page; arrows disappear at scope exit alongside the heap box.

---

## Phase 4: User Story 2 — `Vec` realloc animation + dangling-borrow (Priority: P1)

**Goal**: Vec realloc demo works. `let mut v: Vec<i32> = Vec::new(); v.push(1); v.push(2); let r = &v[0]; v.push(3);` — at the last push, HeapRealloc fires, the borrow becomes dangling, a `Note { RuntimeError }` underlines `&v[0]`.

**Independent Test**: load `m07_vec_realloc.rs`, step through, observe (a) heap box appears at first push, (b) reallocates on each subsequent capacity-doubling push, (c) blue arrow at `&v[0]`, (d) RuntimeError note + editor underline at the realloc step.

### Implementation

- [X] T016 [US2] In `src/typeck.rs`, extend the path-fn dispatch table for `Vec::new` (returns `Ty::Vec(T)` where T is inferred from let-annotation — emit a typeck error "type annotation needed" if not inferrable). Add the method dispatch table for:
  - `Vec<T>::push(self: &mut Vec<T>, x: T) -> ()` — typecheck arg against T; reject if receiver binding isn't `mut` (use existing M06.1 mut-binding check).
  - `Vec<T>::len(self: &Vec<T>) -> Ty::Int(IntKind::U64)`.
  - Typecheck `Expr::Index { receiver, index, span }`: receiver must be `Ty::Vec(T)`, index any `Ty::Int(_)`, returns T.
  Add helper `typecheck_method_call(receiver_ty, name, args, span) -> Result<Ty, ParseError>` for dispatch. Hardcoded match on (Ty, name).

- [X] T017 [US2] In `src/eval.rs`, implement Vec eval:
  - `Vec::new()` (path call) — return `Value::Vec { addr }` where the HeapObject::Vec is allocated with empty elements + capacity 0. **No HeapAlloc event** for empty Vec (per R-015 — first push triggers the first alloc).
  - `v.push(x)` (method call): look up v's HeapObject::Vec; check `len + 1 <= capacity`; if not, double capacity (1 if 0, else cap*2), emit `HeapRealloc { from, to, new_size, span }`, replace v's `Value::Vec.addr` with the new addr, copy elements over, update slot value via `update_slot_value`. Then push x to elements. ALSO emit dangling-borrow Notes (T018).
  - `v.len()` — return `Value::Int { kind: U64, bits: elements.len() as i128 }`.
  - `Expr::Index`: eval receiver to Value::Vec; look up HeapObject::Vec; bounds-check index against elements.len(); if OOB emit `Note { RuntimeError, "index out of bounds: ..." }` and halt-or-return-Unit (plan-phase: halt). On success return `elements[i].clone()`.
  - HeapFree at scope exit for Vec (same path as Box in T012).
  - Borrows of heap elements: `Expr::Borrow` of an `Expr::Index { receiver: Vec, index, .. }` evaluates to `Value::Ref { target: Pointee::Heap(vec.addr), .. }`. Emit `BorrowShared` with `target: Pointee::Heap(vec.addr)`. (Typeck-side: extend borrow-tracker to NOT enforce aliasing rules for heap borrows in M07 — simplification documented; full aliasing-into-heap is harder.)

- [X] T018 [US2] In `src/eval.rs`'s `realloc_heap` helper (or wherever HeapRealloc emits), implement dangling-borrow detection: after emitting `HeapRealloc { from, to, .. }`, scan the World's active-borrow registry (eval-side: track borrows by `borrow_id → Value::Ref` mapping). For each borrow with `target == Pointee::Heap(from)`, emit `MemEvent::Note { kind: NoteKind::RuntimeError, message: "dangling reference: borrow at <span> now points at invalidated memory", span: <original borrow's span> }`. Trace does NOT halt; the dangling borrow's `target_slot` reference in the World is updated to `Heap(to)` so subsequent renders still draw the (now-stale) arrow.
  Actually — simpler: leave the borrow's `target` pointing at `Heap(from)`. The renderer-side query for `data-heap-addr="from"` will fail (that addr is gone), so the arrow simply doesn't draw. The Note + editor highlight carries the pedagogy.

- [X] T019 [US2] In `web/style.css`, ensure the realloc animation is visible. The `.heap-box { transition: transform 300ms ease-out, ... }` from T014 handles flex-reflow moves automatically. Add a brief **border-flash polish** (optional per R-024): on element receiving a `data-just-realloc="true"` attribute, animate the border color from `var(--error)` to default over 600ms. JS-side: when an addr's element is reused for a realloc (same DOM element, new addr), set the attribute briefly. (Skip the border-flash if T020 QA shows the implicit reflow is sufficient.)

- [X] T020 [US2] In `src/pipeline.rs::tests`, add ≥ 5 tests:
  - `run_pipeline_vec_empty` — `Vec::new()` alone, no HeapAlloc.
  - `run_pipeline_vec_push_grows` — push 3 elements, expect 3 events related to growth (1 alloc + 2 realloc for capacities 1, 2, 4).
  - `run_pipeline_vec_index_basic` — `let mut v: Vec<i32> = Vec::new(); v.push(5); let x = v[0];` → x is `5_i32`.
  - `run_pipeline_vec_index_oob` — index empty Vec → `Note { RuntimeError }`.
  - `run_pipeline_vec_dangling_borrow` — the canonical demo; verify a `Note { RuntimeError }` fires at the realloc-push event.

**Checkpoint**: the Vec realloc demo works end-to-end. Headline pedagogy delivered.

---

## Phase 5: User Story 3 — `String` allocation + `push_str` (Priority: P2)

**Goal**: `let mut s = String::from("hi"); s.push_str("!");` typechecks, emits HeapAlloc + (potentially) HeapRealloc on grow. Heap box displays the string content.

**Independent Test**: load `m07_string.rs`, step through, observe heap box with `"hi"` then update to `"hi!"` (with realloc if capacity grew).

### Implementation

- [X] T021 [US3] In `src/typeck.rs`, extend path-fn dispatch for `String::from(s: StrLit) -> String` and method dispatch for `String::push_str(self: &mut String, s: StrLit) -> ()`. The arg type check: the arg expression must be `Expr::StrLit(_, _)` (typeck rejects any other expression as the arg, with a clear "expected string literal" error).

- [X] T022 [US3] In `src/eval.rs`, evaluate:
  - `String::from(StrLit(s))` — alloc `HeapObject::Str { bytes: s.clone(), capacity: s.len() }`, emit HeapAlloc, return `Value::String { addr }`.
  - `s.push_str(StrLit(suffix))` — append suffix bytes to s; if len + suffix.len() > capacity, double capacity until it fits, emit HeapRealloc with the new size. Update slot value.

- [X] T023 [US3] In `src/pipeline.rs::tests`, add ≥ 2 tests:
  - `run_pipeline_string_from` — `String::from("hi")` — verify HeapAlloc with size 2.
  - `run_pipeline_string_push_str_realloc` — `String::from("hi"); s.push_str("world");` — verify HeapRealloc.

**Checkpoint**: String demos work; heap box content updates on push_str.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: samples + dropdown + warnings + bundle + audit + stage.

- [X] T024 [P] Create 3 M07 sample pairs (6 files total). Identical content in `tests/samples/` and `web/samples/`:

  - `m07_box.rs`:
    ```rust
    fn main() {
        let b = Box::new(5);
    }
    ```
  - `m07_vec_realloc.rs`:
    ```rust
    fn main() {
        let mut v: Vec<i32> = Vec::new();
        v.push(1);
        v.push(2);
        let r = &v[0];
        v.push(3);
    }
    ```
  - `m07_string.rs`:
    ```rust
    fn main() {
        let mut s = String::from("hi");
        s.push_str("!");
    }
    ```

- [X] T025 [P] In `web/index.html`, add 3 new `<option>` entries to the sample dropdown after the M06.1 group:

  ```html
  <option value="m07_box">Box (M07)</option>
  <option value="m07_vec_realloc">Vec realloc (M07)</option>
  <option value="m07_string">String (M07)</option>
  ```

- [X] T026 [P] Verify SC-007 (bundle size ≤ +60% vs M06.1 baseline 88,841 B → ≤ 142,146 B) AND SC-008 (zero warnings):
  - `RUSTFLAGS="-D warnings" cargo build --release` — clean.
  - `RUSTFLAGS="-D warnings" cargo test` — full test suite clean.
  - `cargo build --release --target wasm32-unknown-unknown` — WASM clean.
  - `gzip -kc target/wasm32-unknown-unknown/release/rustviz.wasm | wc -c` — should be ≤ 142,146 B. Expected ~120 KB given the variant + UI growth.

- [X] T027 Final clean verification: `cargo clean && RUSTFLAGS="-D warnings" cargo build --release && RUSTFLAGS="-D warnings" cargo test && cargo build --release --target wasm32-unknown-unknown`. Full pipeline must pass clean from scratch.

- [X] T028 Append post-implementation audit log to `specs/011-m07-heap/checklists/requirements.md`. Table covering SC-001 through SC-008. SC-001 / SC-002 / SC-003 / SC-004 deferred to maintainer (visual QA — realloc animation is hard to verify in code). Document the `Value::Ref` cascade impact (how many sites refactored, surprises). Document any M03 snapshot re-baselines (expected: none, since L1 samples don't construct heap or new-Ref values). Note the dangling-borrow detection's simplification (M07 tracks per-borrow target but not per-element borrows). Audit growth + warnings results.

- [X] T029 Stage all changed files:

  ```bash
  git add Cargo.toml Cargo.lock \
          src/parse/token.rs src/parse/lexer.rs src/parse/ast.rs src/parse/parser.rs \
          src/resolve.rs src/typeck.rs src/event.rs src/eval.rs src/ui.rs src/lib.rs src/pipeline.rs \
          tests/samples/m07_*.rs web/samples/m07_*.rs \
          web/index.html web/index.js web/style.css \
          specs/004-m03-event-eval/contracts/m03-api.md specs/011-m07-heap/ \
          CLAUDE.md
  ```

  Run `git status` and report. **Do not commit** — maintainer's QA pass happens between stage and commit per the UI QA-split convention.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: no dependencies.
- **Phase 2 (Foundational)**: T002 parallel to T003-T010. Then T003 → T004 → T005 → T006 sequential (lexer → AST → resolver → parser, each builds on prior). T007 → T008 → T009 → T010 sequential (Ty → Value::Ref cascade → Value heap variants → HeapState).
- **Phase 3 (US1 Box)**: depends on Phase 2 complete. T011 → T012 → T013 → T014 → T015 sequential (typeck → eval → ui struct → web wiring → tests).
- **Phase 4 (US2 Vec)**: depends on Phase 3 complete (Vec builds on Box's heap infrastructure + ArrowView/HeapView already in place). T016 → T017 → T018 → T019 → T020 sequential.
- **Phase 5 (US3 String)**: depends on Phase 2 (StrLit) + Phase 3 (heap infrastructure). T021 → T022 → T023 sequential.
- **Phase 6 (Polish)**: depends on all prior. T024 / T025 / T026 parallel; T027 → T028 → T029 sequential.

### Story-Level Dependencies

- **US1 (Box)** is the foundational user story — establishes heap panel, HeapView, ArrowView restructure, owning arrows, HeapAlloc/HeapFree events.
- **US2 (Vec)** depends on US1's heap infrastructure. Adds methods + indexing + realloc + dangling-borrow.
- **US3 (String)** depends on Phase 2 (StrLit) + Phase 3 (heap infrastructure). Independent of US2.

### Parallel Opportunities

- **T002 + T003**: M03 contract amend vs. lexer tokens. Different files. [P] ✓
- **T024 + T025 + T026**: sample files vs. dropdown HTML vs. read-only audits. [P] ✓
- **US2 vs US3** (after Phase 3 done): both depend on US1's heap infrastructure but don't conflict. In a multi-agent setup, parallel. Sequential for single agent.

---

## Parallel Example: Phase 6 polish

```bash
# All three independent in parallel:
Task T024: "Create 3 m07_*.rs sample pairs (tests/ + web/)"
Task T025: "Add 3 dropdown entries in web/index.html"
Task T026: "Run warnings + bundle size audits (read-only)"
```

---

## Implementation Strategy

### MVP First (US1 Box alone)

1. **Phase 1** (T001): pre-flight.
2. **Phase 2** (T002–T010): foundational. Lexer + parser + AST + Ty + Value::Ref cascade + heap state.
3. **Phase 3** (T011–T015): Box typeck + eval + ui + web + tests.
4. **STOP and VALIDATE**: `cargo test` passes; `let b = Box::new(5);` renders a heap box with a black owning arrow in the page. **At this point M07's heap-infrastructure is shippable** as a smaller increment — but per MILESTONES.md's `do not ship without realloc animation`, this would be M07a (with M07b following for Vec+String+realloc).

US2 (Vec realloc + dangling) is the headline pedagogy — the natural next phase after Box.
US3 (String) is the smallest remaining piece, can defer to M07.1 if scope pressure demands.

### Single-Agent Strategy

1. T001 → T002 → T003 → T004 → T005 → T006 → T007 → T008 → T009 → T010 (Phase 1 + 2 sequential).
2. T011 → T012 → T013 → T014 → T015 (US1 sequential).
3. T016 → T017 → T018 → T019 → T020 (US2 sequential).
4. T021 → T022 → T023 (US3 sequential).
5. T024 + T025 + T026 (parallel polish), T027 → T028 → T029 (sequential close).

---

## Notes

- [P] tasks = different files, no incomplete-task dependencies.
- [Story] tag mandatory on user-story phases.
- **No new Rust deps**. No new JS deps. **No `Cargo.toml` changes**.
- **No new MemEvent variants** — HeapAlloc/HeapRealloc/HeapFree already exist in M03's protocol with payloads typed; M07 just emits them.
- **Value::Ref restructure** is the biggest cascade — affects M06/M06.1 code in ~10-15 sites. Mechanical.
- **`ArrowView` rename** (BorrowView → ArrowView) affects JS-side state access: `state.borrows` → `state.arrows`, with restructured target + kind fields.
- **M01/M02/M03 byte-identical expected** — no existing samples construct heap values or new-Ref shapes. If snapshots drift, investigate.
- **Vec aliasing-rule simplification**: M07 does NOT enforce aliasing rules for borrows into heap memory (e.g. `&v[0]` while `&mut v[1]` exists is permitted). Documented as a known simplification; future revision could add per-element borrow tracking.
- **Per-element borrows are tracked AT THE ALLOCATION GRANULARITY**: a borrow of `v[0]` targets `Pointee::Heap(v.addr)` — whole-allocation. Dangling-borrow detection at HeapRealloc invalidates ALL borrows of that addr, not just the specific element. Pedagogically clean; technically a simplification.
- **Realloc animation** uses implicit flex reflow + CSS transition. If the visual isn't dramatic enough during QA, T019's border-flash polish provides a fallback.
- **Bundle-size budget +60%** from M06.1. Generous because M07 genuinely adds new functionality (heap infrastructure + UI rendering + new lexer/parser/AST features). Hard ceiling stays M04's 2 MB.
- **Sized L bordering on XL** per the rubric: ~10 source modules + heap panel + arrow renderer extension + cascade refactor + 3 sample pairs + ≥ 10 new tests. ~1200-1500 LOC net change.
- **Maintainer QA between stage and commit** — same pattern as prior milestones.
- Avoid: HashMap / Rc / RefCell / threads / Vec<non-Copy T> / indexing assignment / Box re-borrows / slice borrows / iterator methods / Vec::with_capacity. All explicitly deferred per spec.
