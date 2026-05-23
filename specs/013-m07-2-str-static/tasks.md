---

description: "Task list for M07.2 — `&str` + static memory"
---

# Tasks: M07.2 — `&str` + static memory

**Input**: Design documents from `/specs/013-m07-2-str-static/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/m07-2-protocol-delta.md ✓, quickstart.md ✓

**Tests**: M01/M02/M03 stay byte-identical (no existing L1 sample constructs string literals). M07's `run_pipeline_string_from` and `run_pipeline_string_push_str_realloc` re-baseline to assert both `StaticAlloc` + `HeapAlloc` events. New `cargo test --lib pipeline::tests` covering: string-literal-as-slice (no heap event), `String::from` emits both alloc events, literal dedup, `s.len()` on `&str`, `push_str` with both literals visible — ≥ 5 new tests. Manual M07.2 QA per the SC-008 procedure.

**Organization**: 3 user stories (US1 + US2 P1, US3 P2). Sized M. ~4 source files modified + new static-region visual + 3 sample pairs. **Smaller than M07.1** — slice infrastructure fully reused; only the static-region targeting + `Pointee::Static` propagation is new.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1/US2/US3 tag, mandatory on user-story phases only
- File paths relative to repo root

## Path Conventions

~4 existing source files modified + new web-side static-region rendering in `web/` + 3 sample pairs. See `specs/013-m07-2-str-static/plan.md` Project Structure.

---

## Phase 1: Setup

**Purpose**: pre-flight — confirm starting state.

- [X] T001 Verify pre-conditions: branch `013-m07-2-str-static` checked out; `cargo test` from `main` passes (110 tests post-M07.1); M07.1 page loads — slice arrows display with `[len: N]` annotation, hover highlights cover byte-cells and element labels. → 110 tests pass; branch confirmed.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: new newtype + protocol additions + Ty::Str sugar + StaticState scaffolding. Required by all three user stories. **Smaller than M07.1's Phase 2** — no parser changes, no AST changes; all additions are protocol + eval state + a single typeck variant.

- [X] T002 Amend M03's contract in `specs/004-m03-event-eval/contracts/m03-api.md`: note M07.2 as the 6th invocation of the closed-enum-with-revisions rule. Additions: `Pointee::Static(StaticAddr)`, `Ty::Str`, `MemEvent::StaticAlloc { addr, bytes, span }`, `ArrowTarget::Static(u32)`. **Removal**: `Value::Str(String)` (M07's transient — now dead since literals become `Value::Slice`). Cross-reference `specs/013-m07-2-str-static/contracts/m07-2-protocol-delta.md`.

- [X] T003 In `src/event.rs`: Add `StaticAddr(pub u32)`, extend `Pointee` with `Static(StaticAddr)`, extend `MemEvent` with `StaticAlloc`. **Value::Str removal deferred to T008** per the phase-ordering note (avoid intermediate compile breakage; eval still constructs Value::Str for `Expr::StrLit` until T008 rewrites that arm).

- [X] T004 (deferred to T008 per ordering note — Value::Str removal cascade happens atomically with the literal-as-slice rewrite).

- [X] T005 In `src/typeck.rs`:
  - Add `Ty::Str` variant (no fields — sugar over `Ty::Slice(Box::new(Ty::Int(IntKind::U8)))`).
  - Update `Ty::name(&self)` to render `Ty::Str` as `"&str"`.
  - Update `Ty::is_copy(&self)` to return `false` for `Ty::Str` (same as Slice).
  - Add a centralizing helper `fn is_str_like(ty: &Ty) -> bool` returning true for `Ty::Str` OR `Ty::Slice(inner)` where `**inner == Ty::Int(IntKind::U8)`. Use this helper anywhere typeck previously checked for slice-of-bytes (the existing `Ty::Slice` arm of method dispatch covers the slice form; `Ty::Str` arm covers the sugar form).
  - Change `Expr::StrLit` arm in `typecheck_expr_inner` to return `Ok(Ty::Str)` instead of `Ok(Ty::String)`.
  - Method dispatch: add `(Ty::Str, "len") -> Ok(Ty::Int(IntKind::U64))` row matching the existing `(Ty::Slice(_), "len")` row.

- [X] T006 In `src/eval.rs` — add static-region state:
  - Add `static_region: StaticState` field to `Evaluator`; initialize in `Evaluator::new`.
  - Define `struct StaticState { next_addr: u32, blocks: IndexMap<StaticAddr, StaticBlock>, by_content: HashMap<String, StaticAddr> }` and `struct StaticBlock { bytes: String }` (both private to eval).
  - Add helper `intern_static(&mut self, bytes: String, span: Span) -> StaticAddr`: if `by_content` already maps the bytes, return that addr without firing an event. Otherwise allocate fresh addr (monotonic `next_addr`), insert into both maps, emit `MemEvent::StaticAlloc { addr, bytes: bytes.clone(), span }`, return new addr.
  - Add helper `get_static_bytes(&self, addr: StaticAddr) -> Option<&str>` for `String::from` / `push_str` to extract the literal's content from the static region.
  - Fix the `value_size_bytes` cascade from T003's `Value::Str` removal — drop the `Value::Str(_) => 0` arm.
  - Fix the `render_value_for_note` cascade — drop the `Value::Str(s) => format!("\"{s}\"")` arm.

- [X] T007 In `src/ui.rs` — add static-region UI state + ArrowTarget extension:
  - Define `pub struct StaticView { pub addr: u32, pub bytes: String, pub size: u32, pub display: String }` with `Debug, Clone, PartialEq, Serialize, Deserialize` derives.
  - Add `pub static_region: Vec<StaticView>` field to `StateSnapshot` with `#[serde(default, skip_serializing_if = "Vec::is_empty")]`.
  - Add `static_region: Vec<StaticView>` field to `World` (private).
  - Extend `ArrowTarget` enum with `Static(u32)` variant (alongside existing `Slot(u32)` and `Heap(u32)`).
  - Fix the `render_value` cascade from T003's `Value::Str` removal — drop the `Value::Str(s) => format!("\"{s}\"")` arm.

**Checkpoint**: `cargo build` clean. `cargo test` passes — M01/M02/M03 byte-identical (no L1 sample constructs literals); M07's existing tests pass because `Expr::StrLit` typing change isn't observable yet (eval doesn't yet construct the Static-targeted Value::Slice — that's T009). Actually: this checkpoint will FAIL on M07's `run_pipeline_string_from` because the typeck-side `Expr::StrLit` now returns `Ty::Str` but the eval still creates `Value::Str` (removed!). Re-order if needed: keep `Value::Str` until T009 lands, OR accept temporary breakage and finish through Phase 3.

> **Phase ordering note**: T003 (remove Value::Str) and T004/T006/T007 (cascade fixes) are interlocked. The eval-side `String::from` arm still references Value::Str at this point. Re-pin: defer the `Value::Str` removal to T009 (US1 implementation) where the new Value::Slice flow replaces it atomically. Update T003 to NOT remove Value::Str; T009 removes it as part of the literal-flow rewrite.

---

## Phase 3: User Story 1 — String literal is `&'static str` (Priority: P1)

**Goal**: `let s = "toto";` typechecks as `s : &str`, evaluates to a `Value::Slice` with `target: Pointee::Static(_)`, emits `StaticAlloc` + `BorrowShared` events. The static-memory visual region renders the block; a blue slice arrow with `[len: 4]` annotation connects `s` to the static block. Zero `HeapAlloc` events for the literal.

**Independent Test**: load `m07_2_str_literal.rs`, step to the binding, observe `s : &str` row + static-memory region with `"toto"` block + blue slice arrow with `[len: 4]` annotation.

### Implementation

- [X] T008 [US1] In `src/eval.rs`, rewrite the `Expr::StrLit(s, span)` arm in `eval_expr`:
  - Call `intern_static(s.clone(), *span)` → get `addr: StaticAddr`. This emits the StaticAlloc (first occurrence) or reuses (subsequent).
  - Allocate a fresh `borrow_id` via `alloc_borrow_id()`.
  - Emit `MemEvent::BorrowShared { borrow_id, target: Pointee::Static(addr), span: *span }`.
  - Register the borrow in the current scope's borrow list (so `BorrowEnd` fires at scope exit — same path as M07.1 slice borrows).
  - Return `Value::Slice { borrow_id, target: Pointee::Static(addr), start: 0, len: s.len() as u64, mutable: false, byte_offset: 0, byte_len: s.len() as u64 }`.
  - **Remove `Value::Str(String)` from `event.rs`** in this task (delayed from T003 to avoid intermediate breakage).
  - Update `eval_path_call(["String", "from"], args, span)`: the arg now evaluates to `Value::Slice { target: Pointee::Static(addr), .. }`; extract bytes via `self.static_region.blocks[&addr].bytes.clone()`. Replace the existing `match self.eval_expr(&args[0]) { Value::Str(s) => s, .. }` accordingly. The heap allocation flow (HeapAlloc, HeapObject::Str storage) stays unchanged.
  - Update `eval_method_call(receiver, "push_str", args, span)` arm similarly: extract suffix bytes from the static block instead of `Value::Str`.

- [X] T009 [US1] In `src/ui.rs::apply_event`:
  - Handle `MemEvent::StaticAlloc { addr, bytes, span }`: push a new `StaticView { addr: addr.0, bytes: bytes.clone(), size: bytes.len() as u32, display: format!("\"{bytes}\"") }` to `world.static_region` (never remove — static blocks persist).
  - In the existing `SlotWrite` arm where `Value::Slice` is destructured (added in M07.1), the `target` field is already a `Pointee` — no code change needed if the match expression already handles `Pointee::Static(_)` symmetrically via existing arms. Verify: the existing `if let Value::Slice { target, .. } = value` and downstream `match target` sites should accept the new `Pointee::Static(addr)` arm and produce `ArrowTarget::Static(addr.0)`. Add the new match arm to whichever site dispatches on `Pointee` for slice rendering (typically the same site populating `ActiveBorrowState.target`).
  - Extend the existing `BorrowShared` apply-event arm — the `target: Pointee` field already accepts the new variant; map `Pointee::Static(addr)` to `BorrowTarget::Static(addr.0)` (NEW variant on the private `BorrowTarget` enum in ui.rs, mirroring the ArrowTarget extension).
  - Update `state_snapshot()` to populate `static_region` from `world.static_region`.
  - In the arrows-from-borrows builder, add the `BorrowTarget::Static(addr) => ArrowTarget::Static(addr)` arm.

- [X] T010 [US1] Web-side static-region rendering (HTML + CSS + JS):
  - `web/index.html`: add a new `<section id="static" aria-label="static memory">` between the `<section id="stacks">` and `<section id="heap">` sections. Add a `<header>` inside with the label "static memory (RO)" (italic, small).
  - `web/style.css`: add `#static { display: flex; flex-wrap: wrap; gap: 0.5rem; padding: 0.5rem 1rem; background: linear-gradient(to right, #f0f0ef, #e8e8e7); border-right: 1px solid var(--frame-border); align-content: flex-start; min-height: 60px; }`. Add `.static-block` styling (similar to `.heap-box` but with a gray border + lighter background). Add `.static-block .byte-cell` overrides (`background: #ccc` for filled; static bytes are always "used" since there's no capacity/used distinction).
  - `web/index.js`:
    - Add `renderStaticRegion(staticRegion)` function: maintain `staticElements: Map<addr, HTMLElement>`. For each `StaticView`, create or update a `<div class="static-block" data-static-addr={addr}>` with the display label + byte-cells (one per byte, all marked used). Never remove entries.
    - Call `renderStaticRegion(state.static_region || [])` from the main `render()` function, before `renderHeap` so the static region exists when arrows query positions.
    - Update `renderArrows`: the target resolver gains a `Static` arm. When `arrow.target.Static !== undefined`, look up the element via `document.querySelector(\`[data-static-addr="${a.target.Static}"]\`)` and set `targetIsHeap = false` plus a parallel `targetIsStatic = true` flag. Routing: use the same "enter from above" rectilinear path as heap targets (the static region is above heap; enter from its top edge). Lane stagger applies.
    - Update the slice-arrow hover-highlight: when `targetIsStatic` (and `byte_offset` / `byte_len` are present), look up the static block's `.byte-cell` elements and toggle the `byte-slice-highlighted` class on indices `[byte_offset, byte_offset + byte_len)`. Element-span highlight is skipped for static (no Vec-style structured display).

- [X] T011 [US1] In `src/pipeline.rs::tests`, add ≥ 2 tests for US1:
  - `run_pipeline_str_literal` — `fn main() { let s = "toto"; }` — assert: trace contains exactly one `MemEvent::StaticAlloc` event with `bytes == "toto"`; trace contains exactly one `MemEvent::BorrowShared` event with `target == Pointee::Static(_)`; trace contains **zero** `MemEvent::HeapAlloc` events; the SlotWrite for `s` carries `Value::Slice { len: 4, .. }`.
  - `run_pipeline_literal_dedup` — `fn main() { let a = "hi"; let b = "hi"; }` — assert: exactly one `StaticAlloc` event (content-dedup); two `BorrowShared` events (one per `let`).

**Checkpoint**: string literals now correctly typecheck as `&str`, evaluate to slice values targeting the static region, and render with the static-region visual + slice arrow + `[len: N]` annotation. The static region appears in the page layout.

---

## Phase 4: User Story 2 — `String::from` copies static bytes to heap (Priority: P1)

**Goal**: `let s = String::from("hi");` produces both a static `"hi"` block AND a heap `String` block, visible side by side. The black owning arrow points from `s` to the heap block; the slice-into-static arrow is transient (live during the `String::from` call's evaluation, gone after).

**Independent Test**: load `m07_2_string_from.rs`, observe both blocks at the post-call step, with the owning arrow on the heap block.

### Implementation

- [X] T012 [US2] Re-baseline M07's existing string tests in `src/pipeline.rs::tests`:
  - `run_pipeline_string_from`: was `assert_eq!(alloc_count, 1)`. Update to assert separately: `let heap_count = events.iter().filter(|e| matches!(e, crate::MemEvent::HeapAlloc { .. })).count(); let static_count = events.iter().filter(|e| matches!(e, crate::MemEvent::StaticAlloc { .. })).count(); assert_eq!(heap_count, 1, "String buffer heap alloc"); assert_eq!(static_count, 1, "literal interned in static");`.
  - `run_pipeline_string_push_str_realloc`: similar update — verify both literals are interned in static (`static_count == 2` for `"hi"` + `"world"`) AND realloc events fire as before.

- [X] T013 [US2] In `src/pipeline.rs::tests`, add a new test for US2:
  - `run_pipeline_string_from_static_visible` — `fn main() { let s = String::from("hi"); }` — assert: the StateSnapshot at the post-call cursor position has BOTH `static_region.len() == 1` (the `"hi"` static block) AND `heap.len() == 1` (the new String). To inspect snapshots, use the existing `ui::Cursor::snapshot` API or equivalent — match the existing test patterns for snapshot-based assertions if present, otherwise inspect events directly (`HeapAlloc` for String's buffer + `StaticAlloc` for the literal).

- [X] T014 [US2] Verify the existing M07 sample `m07_string.rs` (canonical String demo) still renders correctly:
  - The Editor still shows the same source.
  - At runtime: static region shows `"hi"` and `"!"`; heap region shows the growing String. No new sample needed in the test for US2 — the existing sample is unchanged on disk, only its behavior gains the static-region visualization.

**Checkpoint**: `String::from` and `push_str` show both static + heap blocks side-by-side. M07's existing tests pass with the re-baseline.

---

## Phase 5: User Story 3 — `push_str` takes `&str` consistently (Priority: P2)

**Goal**: the `"!"` argument in `s.push_str("!")` flows through the same static-region machinery as the literal binding from US1 — no separate heap allocation for the argument, just a byte copy from the static block into `s`'s heap buffer.

**Independent Test**: load `m07_2_push_str.rs`, observe two static blocks (`"hi"` and `"!"`) and one heap block (`s`'s String).

### Implementation

- [X] T015 [US3] Verified: T008 already added the static-region lookup in the `push_str` method-call arm.

- [X] T016 [US3] In `src/pipeline.rs::tests`, add a test for US3:
  - `run_pipeline_push_str_static` — `fn main() { let mut s = String::from("hi"); s.push_str("!"); }` — assert: `static_count == 2` (one for `"hi"`, one for `"!"`); `heap_count == 1` (only the String's buffer); the trace contains a `HeapRealloc` event (or in-place growth — depends on the cap=2 → 3-byte case; either is acceptable for this test, just verify the trace is well-formed).

- [X] T017 [US3] Add `run_pipeline_str_len` test in `src/pipeline.rs::tests`:
  - `fn main() { let s = "toto"; let n = s.len(); }` — assert: `n`'s SlotWrite has `Value::Int { kind: U64, bits: 4 }`. Verifies the `Ty::Str` method dispatch.

**Checkpoint**: `push_str`'s argument flows from static; `s.len()` works on `&str`. All three P1/P2 user stories functionally complete.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: samples + dropdown + warnings + bundle + audit + stage.

- [X] T018 [P] Create 3 M07.2 sample pairs (6 files total). Identical content in `tests/samples/` and `web/samples/`:

  - `m07_2_str_literal.rs`:
    ```rust
    fn main() {
        let s = "toto";
    }
    ```
  - `m07_2_string_from.rs`:
    ```rust
    fn main() {
        let s = String::from("hi");
    }
    ```
  - `m07_2_push_str.rs`:
    ```rust
    fn main() {
        let mut s = String::from("hi");
        s.push_str("!");
    }
    ```

- [X] T019 [P] In `web/index.html`, add 3 new `<option>` entries to the sample dropdown after the M07.1 group:

  ```html
  <option value="m07_2_str_literal">Str literal (M07.2)</option>
  <option value="m07_2_string_from">String::from + static (M07.2)</option>
  <option value="m07_2_push_str">push_str + static (M07.2)</option>
  ```

- [X] T020 [P] Verify SC-008 (bundle size ≤ +15% vs M07.1 baseline) AND SC-009 (zero warnings): → raw WASM 280,519 B (+2.0% vs M07.1 baseline 274,947 B); -D warnings clean for host + WASM; 115 tests pass.
  - `RUSTFLAGS="-D warnings" cargo build --release` — clean.
  - `RUSTFLAGS="-D warnings" cargo test --release` — full test suite clean (now ~117 tests post-M07.2).
  - `cargo build --release --target wasm32-unknown-unknown` — WASM clean.
  - `stat -c%s target/wasm32-unknown-unknown/release/rustviz.wasm` — record raw WASM size. M07.1 baseline (raw cargo build) was 274,947 B; +15% ceiling is ~316,000 B. Expected ~290 KB given the small additive surface.

- [X] T021 Final clean verification: `cargo clean && RUSTFLAGS="-D warnings" cargo build --release && RUSTFLAGS="-D warnings" cargo test && cargo build --release --target wasm32-unknown-unknown`. → 115 tests pass; 0 warnings; WASM builds clean from scratch.

- [X] T022 Append post-implementation audit log to `specs/013-m07-2-str-static/checklists/requirements.md`. Table covering SC-001 through SC-009. Done.

- [X] T023 Stage all changed files: → 25 files staged (10 modified + 15 added). No commit per UI-QA-split convention.

  ```bash
  git add Cargo.toml Cargo.lock \
          src/event.rs src/typeck.rs src/eval.rs src/ui.rs src/pipeline.rs \
          tests/samples/m07_2_*.rs web/samples/m07_2_*.rs \
          web/index.html web/index.js web/style.css \
          specs/004-m03-event-eval/contracts/m03-api.md specs/013-m07-2-str-static/ \
          CLAUDE.md
  ```

  Run `git status` and report. **Do not commit** — maintainer's QA pass happens between stage and commit per the UI QA-split convention.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: no dependencies.
- **Phase 2 (Foundational)**: T002 parallel to T003. Then T003 → T004 → T005 → T006 → T007 sequential (event protocol → cascade fix in event.rs → typeck Ty::Str → eval StaticState → ui StaticView). T005 / T006 / T007 touch different files; can be done [P] in parallel after T003.
  - **Important**: defer the `Value::Str` removal from T003 to T008 (US1 implementation) to avoid intermediate compile breakage. T004/T006/T007's "drop Value::Str arm" steps move to T008. Update tasks accordingly during execution.
- **Phase 3 (US1 string-literal-as-slice)**: depends on Phase 2 complete. T008 → T009 → T010 → T011 sequential (eval rewrite → ui apply_event → web rendering → tests).
- **Phase 4 (US2 String::from copies)**: depends on Phase 3 (the eval rewrite in T008 handles String::from's new arg-extraction path). T012 → T013 → T014 sequential.
- **Phase 5 (US3 push_str + s.len())**: depends on Phase 3. T015 → T016 → T017 sequential.
- **Phase 6 (Polish)**: depends on all prior. T018 / T019 / T020 parallel; T021 → T022 → T023 sequential.

### Story-Level Dependencies

- **US1 (string-literal-as-&str)** is the foundational user story — establishes the static region, the `Pointee::Static` borrow path, the `Value::Slice` constructor for literals, and the static-region UI rendering. Everything else builds on it.
- **US2 (String::from copies)** depends on US1's literal-as-slice machinery (the arg now evaluates as `Value::Slice`, and the eval needs to extract bytes via static-region lookup).
- **US3 (push_str + s.len())** depends on US1 (similar arg-extraction path) but is otherwise independent of US2.

### Parallel Opportunities

- **T002 + T003**: M03 contract amend vs. event.rs newtype + variant additions. Different files. [P] ✓
- **T005 + T006 + T007**: typeck Ty::Str, eval StaticState, ui StaticView in three different files. All additive after T003 lands. [P] ✓
- **T018 + T019 + T020**: sample files vs. dropdown HTML vs. read-only audits. [P] ✓

---

## Parallel Example: Phase 2 protocol additions

```bash
# Three independent additive changes in parallel after T003:
Task T005: "Add Ty::Str variant in src/typeck.rs"
Task T006: "Add StaticState in src/eval.rs"
Task T007: "Add StaticView + ArrowTarget::Static in src/ui.rs"
```

## Parallel Example: Phase 6 polish

```bash
# Three independent polish tasks in parallel:
Task T018: "Create 3 m07_2_*.rs sample pairs (tests/ + web/)"
Task T019: "Add 3 dropdown entries in web/index.html"
Task T020: "Run warnings + bundle size audits (read-only)"
```

---

## Implementation Strategy

### MVP First (US1 string-literal-as-`&str` alone)

1. **Phase 1** (T001): pre-flight.
2. **Phase 2** (T002–T007): foundational. Protocol additions (StaticAddr, Pointee::Static, MemEvent::StaticAlloc, Ty::Str, ArrowTarget::Static) + StaticState + StaticView. **Defer Value::Str removal to T008**.
3. **Phase 3** (T008–T011): string literal as slice — eval + ui + web + tests.
4. **STOP and VALIDATE**: `cargo test` passes; `let s = "toto";` renders `s : &str` with a static block + slice arrow + `[len: 4]` annotation. **At this point the static-region infrastructure is shippable** — US2/US3 are smaller follow-ups using the same machinery.

US2 (String::from copies) is a 2-test re-baseline + 1 new snapshot test.
US3 (push_str + s.len()) is mostly verification work + 2 small new tests.

### Single-Agent Strategy

1. T001 → T002 → T003 → T004 → T005 → T006 → T007 (Phase 1 + 2 sequential).
2. T008 → T009 → T010 → T011 (US1 sequential).
3. T012 → T013 → T014 (US2 sequential).
4. T015 → T016 → T017 (US3 sequential).
5. T018 + T019 + T020 (parallel polish), T021 → T022 → T023 (sequential close).

---

## Notes

- [P] tasks = different files, no incomplete-task dependencies.
- [Story] tag mandatory on user-story phases.
- **No new Rust deps**. No new JS deps. **No `Cargo.toml` changes**.
- **One new MemEvent variant** (`StaticAlloc`) — 6th invocation of the closed-enum-with-revisions rule.
- **`Value::Str` removed** — second variant removal in the project after M03.1's `FrameEnter.params`. Both are dead-code cleanups, not breaking changes (no shipped sample constructs `Value::Str` from anything but `Expr::StrLit` which is rewritten in T008).
- **M01/M02/M03 byte-identical expected** — no L1 sample constructs string literals. If snapshots drift, investigate.
- **M07 string tests re-baselined** (T012) — `run_pipeline_string_from` and `_push_str_realloc` now check both `StaticAlloc` and `HeapAlloc` event counts.
- **Slice machinery fully reused** from M07.1: `Value::Slice` shape, `[len: N]` arrow annotation, hover-highlight on byte-cells. Static blocks slot into the existing patterns by adding the `Pointee::Static` / `ArrowTarget::Static` arms.
- **`Ty::Str` is sugar**, not a wholly separate type. Method dispatch + borrow tracking treat it interchangeably with `Ty::Slice(Box::new(Ty::Int(U8)))`. The distinction is only at `Ty::name()` rendering for pedagogical clarity (`"&str"` vs `"&[u8]"`).
- **Content-dedup via HashMap<String, StaticAddr>**: O(1) lookup; matches Rust linker behavior; deterministic rendering via the parallel IndexMap preserving insertion order.
- **No `StaticFree` event** — static blocks persist for the trace's lifetime. The dangling-detection scan in `realloc_heap` ignores `Pointee::Static(_)` targets.
- **Hover highlight on static blocks**: byte-cells only (no element-span highlight — static blocks render raw bytes, no Vec-style structured display in M07.2).
- **Bundle-size budget +15%** from M07.1 baseline. Small additive surface should fit easily.
- **Sized M** per the rubric: ~4 source modules + minor JS additions + 3 sample pairs + ≥ 5 new tests. ~400-600 LOC net change.
- **Maintainer QA between stage and commit** — same pattern as prior milestones.
- **Foundation for future work**: `&str` slicing (`&"hello"[1..3]`), `&str` method receivers beyond `len()`, format-style APIs — all build on this milestone's `Pointee::Static` + `Ty::Str` infrastructure.
- Avoid: `&str` slicing / string indexing / `format!` / `println!` / `+`/`+=` on strings / generalized `&str` args in `push_str`/`String::from` / UTF-8 char-level pedagogy / `static` items beyond string literals. All explicitly deferred per spec.
