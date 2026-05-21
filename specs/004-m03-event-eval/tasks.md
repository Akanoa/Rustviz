---

description: "Task list for M03 — Event model + Level 1 evaluator"
---

# Tasks: M03 — Event Model + Level 1 Evaluator

**Input**: Design documents from `/specs/004-m03-event-eval/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/m03-api.md ✓, quickstart.md ✓

**Tests**: Tests ARE part of M03's deliverable — `MILESTONES.md` › M03 demo + spec SC-001 demand snapshot tests. Plus one in-source unit test for the unexercised `SlotMove` variant (FR-006 / research R-008).

**Organization**: tasks grouped by user story. MVP = US1 (Phase 3). US2 and US3 add property assertions on top of US1's output (spans, enum-completeness).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1/US2/US3 tag, mandatory on user-story phases only
- File paths relative to repo root

## Path Conventions

Single Rust library crate. New M03 code under `src/event.rs` + `src/eval.rs`; tests under `tests/m03.rs` + `tests/samples/m03_*.rs`.

---

## Phase 1: Setup

**Purpose**: register the new `m03` test target and create empty M03 source/test files.

- [X] T001 Edit `Cargo.toml` to append a new `[[test]]` block with `name = "m03"` and `path = "tests/m03.rs"`. Keep existing `m01`/`m02` entries and the `indexmap` dep unchanged. No new dependencies.
- [X] T002 Create empty placeholder files: `src/event.rs` (one-line `//!` doc naming the module's role per `specs/004-m03-event-eval/plan.md` Project Structure), `src/eval.rs` (same), `tests/m03.rs` (one-line `//!` doc). Confirm `cargo build` still succeeds with these placeholders (`lib.rs` won't declare them yet).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: implement the full `MemEvent` enum and supporting types so every user-story phase has them in place. Stub the evaluator entry point.

**⚠️ CRITICAL**: no user-story phase begins until Phase 2 closes (`cargo build` succeeds with the full M03 public type surface compiled).

- [X] T003 [P] In `src/event.rs`, implement all M03 public types per `specs/004-m03-event-eval/data-model.md`:
  - Newtype wrappers (`SlotId`, `FrameId`, `HeapAddr`, `BorrowId`) — `pub struct X(pub u32)`, derives `Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord`.
  - `pub enum Pointee { Slot(SlotId), Heap(HeapAddr) }`.
  - `pub enum Value { Int(i64), Bool(bool), Unit }` — derives `Debug, Clone, PartialEq`. Helper method `Value::name() -> &'static str` returning `"i32"` / `"bool"` / `"()"`.
  - `pub enum NoteKind { RuntimeError, Info }` — derives `Debug, Clone, PartialEq`.
  - `pub enum MemEvent` with all 19 variants per data-model.md (Threads: 3, Frames: 2, Stack slots: 4, Heap: 3, Borrows: 3, Sync: 4, Pedagogy: 1, totaling 20 — adjust count to match data-model and FR-001 / SC-009). Each variant carries a `span: Span` field. Imports from `crate::parse::span::Span` and `crate::typeck::Ty`. Derives `Debug, Clone, PartialEq`.
  - Module-level `#[cfg(test)] mod tests` with at minimum **one** unit test that constructs a `MemEvent::SlotMove { from: SlotId(0), to: SlotId(1), value: Value::Int(5), span: Span::new(0, 1, FileId(1)) }` and asserts the Debug output is non-empty (research R-008 / FR-006).
- [X] T004 [P] In `src/eval.rs`, define the public entry stub and internal scaffold per `specs/004-m03-event-eval/data-model.md`:
  - `pub fn evaluate(_program: &ast::Program, _resolution: &Resolution, _types: &TypeMap) -> Result<Vec<MemEvent>, ParseError> { unimplemented!("T007 implements the evaluator") }`.
  - Private `struct Evaluator<'a>` with fields per data-model.md (`program`, `resolution`, `types`, `fn_decls: HashMap<BindingId, &'a ast::FnDecl>`, `frames: Vec<Frame>`, `next_slot_id: u32`, `next_frame_id: u32`, `events: Vec<MemEvent>`, `halted: bool`).
  - Private `struct Frame { frame_id: FrameId, fn_binding: BindingId, scopes: Vec<Scope> }`.
  - Private `struct Scope { locals: Vec<LocalSlot> }`.
  - Private `struct LocalSlot { binding_id: BindingId, slot_id: SlotId, name: String, value: Value, decl_span: Span }`.
- [X] T005 Edit `src/lib.rs` to expose the new modules and re-exports per `specs/004-m03-event-eval/contracts/m03-api.md`: add `pub mod event;` and `pub mod eval;`, then `pub use eval::evaluate;` and `pub use event::{BorrowId, FrameId, HeapAddr, MemEvent, NoteKind, Pointee, SlotId, Value};`. Keep all existing M01/M02 re-exports unchanged.
- [X] T006 Run `cargo build` and `cargo test --test m02` and `cargo test --test m01`. All MUST succeed. The stub `evaluate()` body will produce `unreachable_code` warnings — acceptable until T007. M01/M02 tests MUST remain unchanged.

**Checkpoint**: M03 public type surface compiles. Evaluator is a stub. Ready to fill in the walker.

---

## Phase 3: User Story 1 — Emit a complete event stream for L1 (Priority: P1) 🎯 MVP

**Goal**: a valid L1 program goes through `parse → resolve → typeck → evaluate` and produces a deterministic `Vec<MemEvent>` covering function-call boundaries, slot allocation/write/drop, block-scope teardown, branch selection, and runtime errors. Snapshot tests pin output for representative samples.

**Independent Test**: `cargo test --test m03` passes with snapshots for at least 8 samples (arithmetic, fn call, if-then, if-else, shadowing, nested block, short-circuit, division-by-zero runtime error).

### Implementation

- [X] T007 [US1] Implement the evaluator in `src/eval.rs` per `specs/004-m03-event-eval/research.md` R-009 / R-010 / R-011 / R-013. The full walker:
  - `evaluate()` body: build `fn_decls: HashMap<BindingId, &FnDecl>` from `program.items` by looking up each fn's `BindingId` via `resolution.bindings` (find by name + `BindingKind::Fn`). Find the `main` function's BindingId (look up by name `"main"` — if not present, return `Ok(vec![])`). Call `eval.call_fn(main_binding_id, vec![], main_decl.span)` and return `eval.events` (or `Err(ParseError)` if a static invariant fails).
  - `Evaluator::call_fn(fn_binding: BindingId, args: Vec<Value>, call_span: Span)`: check `recursion_depth() > 100` → emit `Note { kind: RuntimeError, message: "recursion depth exceeded (100 frames)" }`, set `halted = true`, return `Value::Unit`. Allocate a fresh `FrameId`. Look up `fn_decls[fn_binding]` for the AST. Emit `FrameEnter { frame_id, fn_name, params: <slot_id+name+value per param>, span: fn_decl.span }`. Push a new `Frame` onto `frames`. Push a fresh `Scope` onto that frame. For each param (in declaration order), allocate a `SlotId`, push `LocalSlot { binding_id: <param BindingId>, slot_id, name, value: args[i], decl_span: param.span }` into the current scope, emit `SlotAlloc { slot_id, name, ty: <typeck binding_types[param_binding] as Ty>, span: param.span }` and `SlotWrite { slot_id, value, span: param.span }`. Then `eval_block(&fn_decl.body)` — returns the body's value. Drop the scope (emit SlotDrop for each local in LIFO order, pointing at `decl_span` per research R-014 open-question default). Pop the frame. Emit `FrameLeave { frame_id, return_value, span: fn_decl.body.span }` (using body span as the FrameLeave point since `name_span` of the fn would point at the whole decl).
  - `eval_block(&Block) -> Value`: push scope. For each stmt, `eval_stmt`. If `halted`, return `Value::Unit` immediately. Then evaluate tail expression (or `Value::Unit` if none). Drop scope (LIFO SlotDrops). Return tail value.
  - `eval_stmt(&Stmt)`: dispatch. For `Stmt::Let(let_stmt)`: evaluate `init` to a Value (if halted, return). Allocate fresh SlotId, push LocalSlot into current scope, emit SlotAlloc with `ty` looked up from `types.binding_types[let_binding]` (Var case), emit SlotWrite. For `Stmt::Expr(expr)`: evaluate, discard.
  - `eval_expr(&Expr) -> Value`: bottom-up evaluation. LitInt → `Value::Int(*v)`. LitBool → `Value::Bool(*b)`. Ident → look up `binding_id = resolution.uses[span]`; walk current frame's scopes inner-to-outer for a `LocalSlot.binding_id == binding_id`; return its value. Unary: evaluate operand, apply op (`Neg` → wrap_or-detect i64 overflow → if would overflow i32 bounds emit RuntimeError Note + halt + return Unit; otherwise `Value::Int(-x)`; `Not` → `Value::Bool(!b)`). Binary: evaluate lhs, then rhs (with short-circuit: for `BinOp::And`, if lhs is `Bool(false)`, return `Bool(false)` without evaluating rhs; for `BinOp::Or`, if lhs is `Bool(true)`, return `Bool(true)` without evaluating rhs). For arithmetic ops, detect overflow / div-by-zero → emit RuntimeError Note + halt. Return the computed Value. Call: look up callee BindingId via `resolution.uses[callee_span]`; evaluate each arg; call `call_fn(callee_binding, arg_values, call_span)`. Paren: pass through. Block: `eval_block(b)`. If: evaluate cond. If `Bool(true)`, eval then_block. Else if else_block is Some, eval that. Else return `Value::Unit`. Honor `halted` everywhere — if set, abort current evaluation and return Unit.
  - Sequential SlotId/FrameId allocation: `next_slot_id` / `next_frame_id` start at 0 and increment.
  - Determinism: emit events in walk order; never iterate `fn_decls` HashMap (only look up by key).
- [X] T008 [P] [US1] Create samples under `tests/samples/`:
  - `m03_arithmetic.rs`: `fn main() { let x = 2 + 3; }` — US1 AS-1.
  - `m03_fn_call.rs`: `fn add(a: i32, b: i32) -> i32 { a + b } fn main() { let r = add(2, 3); }` — US1 AS-2.
  - `m03_if_then.rs`: `fn main() { let v = if true { 1 } else { 2 }; }` — only `then` branch fires; US1 AS-3 (the `true` case).
  - `m03_if_else.rs`: `fn main() { let v = if false { 1 } else { 2 }; }` — only `else` branch fires; US1 AS-3 (the `false` case).
  - `m03_shadow.rs`: `fn main() { let x = 5; let x = 10; }` — distinct SlotIds, LIFO drop; US1 AS-4.
  - `m03_nested_block.rs`: `fn main() { { let y = 1; }; let z = 2; }` — inner block's `y` dropped before outer block's `z` is allocated; US1 AS-5.
  - `m03_short_circuit.rs`: `fn main() { let a = true; let b = a || (1 / 0 == 0); let c = false; let d = c && (1 / 0 == 0); }` — RHS of `||` and `&&` not evaluated, so the div-by-zero never runs and no RuntimeError appears; FR-008.
  - `m03_div_by_zero.rs`: `fn main() { let x = 1 / 0; }` — RuntimeError Note + stream stops; SC-001(f).
- [X] T009 [US1] Implement `tests/m03.rs` driver mirroring `tests/m02.rs`. Define `fn analyze_sample(name) -> AnalyzeResult` that runs `parse → resolve → typeck → evaluate`; returns either `Ok(Vec<MemEvent>)` or `Err(ParseError)` (boxed for snapshotting). Provide a `sample_test!` macro identical in shape to M02's, snapshotting via `insta::assert_debug_snapshot!` with `snapshot_path => "snapshots"` and `prepend_module_to_snapshot => false`. Add a `#[test]` per sample: `emits_arithmetic`, `emits_fn_call`, `emits_if_then`, `emits_if_else`, `emits_shadow`, `emits_nested_block`, `emits_short_circuit`, `emits_div_by_zero_note`.
- [X] T010 [US1] Run `INSTA_UPDATE=always cargo test --test m03`. Visually inspect each snapshot:
  - `m03_arithmetic`: FrameEnter(main) → SlotAlloc(x) → SlotWrite(x, Int(5)) → SlotDrop(x) → FrameLeave(main, Unit).
  - `m03_fn_call`: FrameEnter(main) → SlotAlloc(r) → FrameEnter(add) → SlotAlloc(a)+SlotWrite(2) → SlotAlloc(b)+SlotWrite(3) → SlotDrop(b) → SlotDrop(a) → FrameLeave(add, Int(5)) → SlotWrite(r, Int(5)) → SlotDrop(r) → FrameLeave(main, Unit).
  - `m03_if_then`: events for the `then` branch only; no events from the `else` branch.
  - `m03_shadow`: two distinct SlotIds for the two `let x`; SlotDrop in LIFO order (inner first).
  - `m03_short_circuit`: no RuntimeError appears even though div-by-zero is syntactically present (RHS never evaluated).
  - `m03_div_by_zero`: ends with `MemEvent::Note { kind: RuntimeError, message: <div-by-zero>, span: <of the division expr> }`; the rest of the program is NOT evaluated.
  Re-run `cargo test --test m03` (no env var) to confirm all snapshots pass deterministically.

**Checkpoint**: 8 snapshot tests pass. The MVP ships: M03 emits the L1 event stream and surfaces runtime errors as Notes.

---

## Phase 4: User Story 2 — Spans on every event (Priority: P1)

**Goal**: every emitted event carries a non-empty span. M04 will rely on this for editor highlighting.

**Independent Test**: a single test enumerates every event in every successful snapshot's output and asserts `span.start < span.end` (or zero-length only on legitimate end-of-input positions).

### Implementation

- [X] T011 [US2] Extend the `sample_test!` macro in `tests/m03.rs`: for successful (`Ok(events)`) results, before snapshotting, walk the events list and assert each event's `span` field. Assertion: extract span (via a helper that pattern-matches every `MemEvent` variant — match-completeness will flag missing arms later) and confirm `span.end >= span.start` (always) AND `span.end > span.start` unless the variant is one of `{ FrameLeave at empty-body, Note at end-of-input }` (zero-length end-of-input is permitted). If any event fails the check, the test fails before the snapshot is even written. Re-run `cargo test --test m03` — all US1 tests should still pass with the added assertion in place. This satisfies SC-002 and User Story 2's independent test criterion.

**Checkpoint**: 8 tests still pass, now with embedded span assertion.

---

## Phase 5: User Story 3 — Full event enum exposed for downstream extension (Priority: P2)

**Goal**: every CLAUDE.md event category appears as a `MemEvent` variant. M06–M08 can fill in payloads without adding new variants.

**Independent Test**: a smoke unit test constructs at least one representative variant from each CLAUDE.md category and asserts the `Debug` output is non-empty. Catches drift where a variant gets accidentally removed.

### Implementation

- [X] T012 [US3] Extend the `#[cfg(test)] mod tests` block at the bottom of `src/event.rs` with **smoke unit tests** for the forward-compat variants (the ones M03 doesn't emit but must compile):
  - `constructs_thread_spawn`: `MemEvent::ThreadSpawn { thread_id: 0, span: <dummy> }` — Debug non-empty.
  - `constructs_heap_alloc`: `MemEvent::HeapAlloc { addr: HeapAddr(0), size: 8, ty_name: "i32".into(), span: <dummy> }`.
  - `constructs_borrow_shared`: `MemEvent::BorrowShared { borrow_id: BorrowId(0), target: Pointee::Slot(SlotId(0)), span: <dummy> }`.
  - `constructs_lock_acquire`: `MemEvent::LockAcquire { addr: HeapAddr(0), span: <dummy> }`.
  - `constructs_note_info`: `MemEvent::Note { kind: NoteKind::Info, message: "hello".into(), span: <dummy> }`.
  - `enum_variant_count`: assert all variants from the spec list can be constructed (this is implicit in the above tests but the count assertion can be a doc comment listing the 19 variant identifiers; the compiler enforces presence). This satisfies SC-009 and User Story 3's independent test.
  - The existing T003 unit test for `SlotMove` already covers that category.

**Checkpoint**: M03's `cargo test --lib` runs the unit tests inside `event.rs`; all variant constructors compile and Debug-render. Catches any future variant removal.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: verify cross-cutting success criteria (SC-005, SC-006, SC-007, SC-008) and close out the audit log.

- [X] T013 [P] Verify SC-007 (zero warnings): `RUSTFLAGS="-D warnings" cargo build --release` and `RUSTFLAGS="-D warnings" cargo test`. Both MUST exit clean.
- [X] T014 [P] Verify SC-006 (LOC cap): `find src/event.rs src/eval.rs -name '*.rs' -print0 | xargs -0 wc -l`. Confirm ≤ 1500. If exceeded, identify the bloat (likely `eval.rs`'s operator table or runtime-error handling). Bump cap with rationale appended to the audit log if the work is legitimately L1-scope.
- [X] T015 [P] Verify SC-005 (determinism): `cargo test --test m03` twice in succession. Confirm no `.snap.new` files appear (snapshot diff = nothing).
- [X] T016 [P] Verify SC-008 (M01 + M02 regression): `cargo test --test m01 && cargo test --test m02`. All previous snapshots must still pass. If any drift, M03 has accidentally modified shared code (likely `lib.rs` re-exports or `Ty`/`Resolution` types) and must be fixed before committing.
- [X] T017 Append post-implementation audit log to `specs/004-m03-event-eval/checklists/requirements.md` (mirror M01/M02 pattern): table of SC-001…SC-009 with PASS/FAIL + notes, any deviations from research/data-model/contract, and the test summary (`cargo test` output line count).
- [X] T018 Final clean verification: `cargo clean && RUSTFLAGS="-D warnings" cargo build --release && RUSTFLAGS="-D warnings" cargo test`. Full suite (m01 + m02 + m03 + any unit tests) MUST pass clean.
- [X] T019 Stage changed files: `git add Cargo.toml Cargo.lock src/event.rs src/eval.rs src/lib.rs tests/m03.rs tests/samples/m03_*.rs tests/snapshots/ specs/004-m03-event-eval/ CLAUDE.md`. Run `git status` and report. **Do not commit** — maintainer's call.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: no dependencies. T001 (Cargo.toml) then T002 (file scaffolding) sequential.
- **Phase 2 (Foundational)**: depends on Phase 1. T003 + T004 parallel (different files). T005 depends on both. T006 depends on T005.
- **Phase 3 (US1)**: depends on Phase 2 closing. T007 (evaluator impl) is the main work. T008 (samples) parallel with T007. T009 (driver) depends on T007 + T008. T010 (run+accept) depends on T009.
- **Phase 4 (US2)**: depends on Phase 3 (modifies the test driver from T009).
- **Phase 5 (US3)**: depends on Phase 2 (touches event.rs only). Can run in parallel with Phase 4.
- **Phase 6 (Polish)**: depends on Phases 4 and 5. T013–T016 parallel (read-only audits). T017–T019 sequential.

### Story-Level Dependencies

- US1 strictly first.
- US2 and US3 can be done in either order or in parallel after US1.

### Parallel Opportunities

- **T003 vs T004**: different files in Phase 2. [P] ✓
- **T007 vs T008**: implementation file vs sample text files. [P] ✓
- **T013–T016**: read-only audits. [P] ✓
- **Phase 4 vs Phase 5**: different files (`tests/m03.rs` vs `src/event.rs`). [P] for parallel agents.

---

## Parallel Example: Phase 2 Foundational

```bash
Task T003: "Implement MemEvent + all M03 types in src/event.rs (incl. SlotMove unit test)"
Task T004: "Stub Evaluator + evaluate() in src/eval.rs"
# Then sequential:
Task T005: "Re-export public surface in src/lib.rs"
Task T006: "cargo build + cargo test --test m01/m02 verification"
```

## Parallel Example: Phases 4 and 5

```bash
# Agent A — US2
Task T011: "Extend sample_test! macro in tests/m03.rs to assert non-empty spans"

# Agent B — US3, concurrently
Task T012: "Add forward-compat variant smoke unit tests in src/event.rs"

# No file conflicts (different files); merge serially.
```

---

## Implementation Strategy

### MVP First (US1)

1. **Phase 1** (T001–T002): scaffold.
2. **Phase 2** (T003–T006): public types compile. Stub evaluator. M01/M02 tests still pass.
3. **Phase 3** (T007–T010): evaluator + 8 snapshot tests.
4. **STOP and VALIDATE**: `cargo test --test m03` exits 0; eyeball each snapshot for correctness.
5. MVP ships: M03 emits L1 event streams. M04 has its input source.

### Incremental Delivery

1. **MVP** = Phases 1–3 (US1 ✓).
2. **Hardening 1** = Phase 4 (US2 ✓). Embedded span assertion in driver.
3. **Hardening 2** = Phase 5 (US3 ✓). Smoke tests for forward-compat variants.
4. **Ready to commit** = Phase 6 polish closed clean (SC-001 through SC-009 verified).

### Single-Agent Strategy

One AI agent sequentially:
1. Phase 1 → Phase 2 (T003 and T004 in parallel writes; T005 + T006 after).
2. Phase 3: T007 (the meat) before T008/T009/T010.
3. Phase 4 → Phase 5 in either order.
4. Phase 6: audits → audit log → stage.

### Parallel-Agent Strategy

After Phase 2:
- Agent A: T007 (evaluator) — owns `src/eval.rs`.
- Agent B: T008 (samples) — owns `tests/samples/`.
- Then sequentially: T009 (driver) → T010 (accept).
- Phase 4 (Agent A on `tests/m03.rs`) and Phase 5 (Agent B on `src/event.rs`) concurrent.

---

## Notes

- [P] tasks = different files, no incomplete-task dependencies.
- [Story] tag mandatory on user-story phases (T007–T012).
- **No new production dependencies** in M03. `indexmap` already in for M02, `insta` already dev-dep. Evaluator uses internal `Vec<Frame>` (deterministic) + private `HashMap<BindingId, &FnDecl>` (iteration order doesn't leak into output).
- The unit tests inside `src/event.rs` are picked up by `cargo test --lib`. They run alongside the integration tests.
- T019's `git add` includes `CLAUDE.md` (speckit auto-update from /speckit-plan) — same lesson as M02.
- If T007 reveals a bug in the data-model.md API (e.g. a `MemEvent` variant needs an extra field), update data-model.md AND contracts/m03-api.md AND tests/m03.rs in lock-step. The contract is what M04 will rely on.
- If a runtime-error case turns out to need both `Note` AND a partial-event flag (e.g. M04 wants to know "the stream ended due to error" vs "the stream ended normally"), add a wrapper struct to the public API rather than changing the `MemEvent` enum shape.
- Avoid: putting M06+ (borrow tracking) or M07+ (heap alloc) work into M03. The roadmap is the contract.
