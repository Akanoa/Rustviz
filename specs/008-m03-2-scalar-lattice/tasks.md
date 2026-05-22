---

description: "Task list for M03.2 — Scalar lattice expansion (integer + float types)"
---

# Tasks: M03.2 — Scalar Lattice Expansion

**Input**: Design documents from `/specs/008-m03-2-scalar-lattice/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/m03-2-protocol-delta.md ✓, quickstart.md ✓

**Tests**: M01/M02 byte-identical (SC-005). M03 snapshots re-baselined (Value Debug format changes; predictable mechanical diff). New `cargo test --lib` for typeck + eval covering each new type's basics + cross-type errors + overflow + NaN/Inf surfacing. Manual M05 QA per the SC-008 procedure.

**Organization**: 3 user stories (US1+US2+US3 all P1). Big foundational phase because Ty + Value both restructure mechanically. No new modules.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1/US2/US3 tag, mandatory on user-story phases only
- File paths relative to repo root

## Path Conventions

5 source files modified + 3 new sample-pair files. No new modules. See `specs/008-m03-2-scalar-lattice/plan.md` Project Structure.

---

## Phase 1: Setup

**Purpose**: pre-flight — confirm starting state.

- [X] T001 Verify pre-conditions: branch `008-m03-2-scalar-lattice` checked out; `cargo test` from `main` passes (57 tests across m01/m02/m03/lib); the M05 page loads via `cd web && trunk serve` and the editor is writable. No code change in this task.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: the type-system restructure (new helper enums + `Ty` and `Value` shape changes + call-site refactor) is the foundation both user stories build on. Big phase by task count, but most tasks are mechanical.

- [X] T002 [P] Amend the M03 contract in `specs/004-m03-event-eval/contracts/m03-api.md`: extend the M03.1 closed-enum relaxation rule to cover `Ty` and `Value` (not just `MemEvent`). Reference `specs/008-m03-2-scalar-lattice/contracts/m03-2-protocol-delta.md` for the M03.2 delta. Same wording style as the M03.1 amendment.

- [X] T003 In `src/typeck.rs`, add the two helper enums:

  - `pub enum IntKind { I8, I16, I32, I64, I128, U8, U16, U32, U64, U128, ISize, USize }` with derives `Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize`.
  - `pub enum FloatKind { F32, F64 }` with the same derives.

  Methods on `IntKind`: `min_value() -> i128`, `max_value() -> i128`, `contains(self, v: i128) -> bool`, `is_signed(self) -> bool`, `name(self) -> &'static str`. `USize` / `ISize` share min/max with `U64` / `I64` per FR-011. Method on `FloatKind`: `name(self) -> &'static str`. Unit tests in the same file for each method's exhaustiveness — at least one per method, asserting the values for U8 (0..=255), I8 (-128..=127), USize (matches U64), etc.

- [X] T004 Restructure `Ty` in `src/typeck.rs` to the nested form:

  ```rust
  pub enum Ty {
      Int(IntKind),
      Float(FloatKind),
      Bool,
      Unit,
  }
  ```

  Update `Ty::name()` to return `String` (delegating to `IntKind::name()` / `FloatKind::name()` for the kind variants, returning `"bool"` / `"()"` for the others). Update `Ty::is_copy()` to return `true` for all 4 variants (all are Copy in L1). At this point the code WILL NOT compile because existing `Ty::I32` call sites are broken; T006 fixes them.

- [X] T005 Restructure `Value` in `src/event.rs` to the unified form:

  ```rust
  pub enum Value {
      Int { kind: IntKind, bits: i128 },
      Float { kind: FloatKind, value: f64 },
      Bool(bool),
      Unit,
  }
  ```

  Keep the existing derives (`Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize`) — no `Eq` added (verified in audit; floats don't impl `Eq`). Import `IntKind` and `FloatKind` from `crate::typeck`. The Debug format change (`Int(5)` → `Int { kind: I32, bits: 5 }`) is expected and will re-baseline M03 snapshots in T014.

- [X] T006 Mechanically refactor every existing `Ty::I32` / `Ty::Bool` / `Ty::Unit` / `Value::Int(N)` call site to the new form. Search via `git grep -nE 'Ty::I32|Ty::Bool\\b|Ty::Unit|Value::Int\\('` and update each:

  - `Ty::I32` → `Ty::Int(IntKind::I32)`
  - `Ty::Bool` → unchanged (Bool stays a unit variant)
  - `Ty::Unit` → unchanged
  - `Value::Int(N)` → `Value::Int { kind: IntKind::I32, bits: N as i128 }` (existing code only constructs i32 values)
  - In `src/ui.rs::render_value`, switch the match on `Value::Int` to destructure `{ kind, bits }` and render as `format!("{bits}_{}", kind.name())`. Similarly handle `Value::Float { kind, value }` — render normal floats as `format!("{value}_{}", kind.name())`, handle `NaN_<kind>`, `+Inf_<kind>`, `-Inf_<kind>` for special values. Bool / Unit unchanged.

  After this task `cargo build` and `cargo test --test m01 --test m02 --test m03` should compile (M03 will likely have snapshot drift; T014 re-baselines).

**Checkpoint**: code compiles. M01 + M02 byte-identical. M03 snapshots show predictable Debug-format diff. Foundation in place for US1 + US2.

---

## Phase 3: User Story 1 — Integer types end-to-end (Priority: P1)

**Goal**: typeck accepts the 12 new integer types as annotations and as arithmetic operand types; eval performs integer arithmetic with overflow detection halting on a RuntimeError note; cross-type arithmetic is a typeck error.

**Independent Test**: type `fn main() { let n: u8 = 250; let m = n + 10; }` in the live editor; observe trace halts with status "u8 overflow: ..." at the `n + 10` step.

### Implementation

- [X] T007 [US1] In `src/typeck.rs`, extend the type-name lookup to recognize all 12 new integer type names as annotations. Lookup table: `"i8" → IntKind::I8`, `"i16" → IntKind::I16`, …, `"usize" → IntKind::USize`. Apply to `let` bindings, function params, and return types.

  Add typeck rules:
  - **Literal range check**: for `let x: T = <int_literal>;` where `T = Ty::Int(k)`, verify `k.contains(literal_value as i128)`. If false, emit a typeck error with span on the literal: `"literal {N} out of range for {kind.name()}"`.
  - **Unsigned negation**: for `-expr` where `expr: Ty::Int(k)` with `!k.is_signed()`, error: `"cannot apply unary `-` to `{kind.name()}`"`.
  - **Cross-type arithmetic**: for `a {op} b` where `typeof(a) != typeof(b)` and both are numeric (`Ty::Int(_)` or `Ty::Float(_)`), error spanning `b`: `"expected `{a_type}`, found `{b_type}`"`. Apply uniformly across `+`, `-`, `*`, `/`, `%`.

- [X] T008 [US1] In `src/eval.rs`, dispatch integer arithmetic over `IntKind`. For each binary op (`+`, `-`, `*`):

  ```rust
  fn int_add(kind: IntKind, a: i128, b: i128) -> Result<i128, (IntKind, &'static str)> {
      let raw = a.checked_add(b).ok_or((kind, "+"))?;
      if kind.contains(raw) { Ok(raw) } else { Err((kind, "+")) }
  }
  ```

  Pattern: `i128::checked_op` then `kind.contains(raw)` gate. On overflow, emit `Note { kind: NoteKind::RuntimeError, message: format!("{} overflow: {a} {op} {b}", kind.name()), span: <op-span> }` and halt the trace (same pattern as the existing div-by-zero). `/` and `%` keep the existing div-by-zero behavior; when both operands are non-zero, dispatch through the same range-gate path. Unary `-` uses `i128::checked_neg` + range gate.

- [X] T009 [US1] Add unit tests in `src/typeck.rs::tests` (extend the existing block) and `src/eval.rs::tests`. Cover:

  - typeck: each of the 12 integer types as a valid annotation (12 tests, can be table-driven); literal range overflow for `u8` (300 rejected) and `i8` (-200 rejected); negation of `u8` rejected; cross-type `u8 + i32` rejected; `i32 + f64` rejected (depends on US2 landing first if not adjusted — note in test).
  - eval: `u8 + u8 = 100` works; `u8::MAX + 1` halts with RuntimeError; `i32 * 100000 * 100000` halts; `i64` non-overflowing arithmetic; `usize` arithmetic (uses u64 range).

  At least 6 new typeck tests + 6 new eval tests. Use table-driven `for each kind in [I8, I16, I32, ...]` where possible to keep test count manageable.

**Checkpoint**: typing `fn main() { let n: u8 = 100; let m: u8 = n + 5; }` compiles. `let m: u8 = 250 + 10;` halts. `let z = (1u8 + 2i32);` is a typeck error (with the syntactic caveat that bare literal suffixes aren't supported; in practice the user writes `let a: u8 = 1; let b: i32 = 2; let c = a + b;`).

---

## Phase 4: User Story 2 — Float types with NaN/Inf info notes (Priority: P1)

**Goal**: lexer recognizes float literals (`digits.digits`), parser produces `FloatLit`, typeck accepts `f32`/`f64` annotations, eval performs float arithmetic with NaN/Inf detection emitting Info notes. The trace does NOT halt on Inf or NaN.

**Independent Test**: type `fn main() { let a: f64 = 0.0; let b: f64 = a / a; }` in the live editor; observe `b = NaN_f64` AND an Info note "produced NaN". Trace continues.

### Implementation

- [X] T010 [US2] In `src/parse/token.rs`, add `Float(f64)` variant to `TokenKind`. In `src/parse/lexer.rs`, after consuming an integer literal, peek for `.` followed by another digit. If found, consume both and parse the full `digits.digits` substring as `f64`, emit `Token::Float(value)`. If the `.` is followed by a non-digit (e.g. end-of-source or an identifier — not currently in L1), fall back to the existing behavior of emitting `Token::Int` + `Token::Dot`. In `src/parse/ast.rs`, add `FloatLit(f64, Span)` variant to `Expr`. In `src/parse/parser.rs`, when parsing a primary expression and seeing `Token::Float(v)`, produce `Expr::FloatLit(v, span)`. Lexer + parser unit tests for the new path: simple literals like `2.5`, `0.0`, `3.14`.

- [X] T011 [US2] In `src/typeck.rs`, recognize `"f32"` and `"f64"` as float type annotations. Type-check `Expr::FloatLit(v, span)` as `Ty::Float(F64)` by default; if context annotation says `F32`, narrow at typeck (no value, just type assignment). Apply the cross-type arithmetic rule (`i32 + f64` error, `f32 + f64` error) consistently with integer dispatch.

  For untyped int literal vs annotated float: `let x: f64 = 5;` — the integer literal `5` is reinterpreted as a `Float(F64)` value at typeck time (literal value 5 fits any reasonable float). Plan-phase confirms this path.

- [X] T012 [US2] In `src/eval.rs`, dispatch float arithmetic. For each binary op:

  ```rust
  fn float_add(kind: FloatKind, a: f64, b: f64) -> (f64, bool /* newly_special */) {
      let was_special = !a.is_finite() || !b.is_finite();
      let result = match kind {
          FloatKind::F32 => (a as f32 + b as f32) as f64,
          FloatKind::F64 => a + b,
      };
      let now_special = result.is_nan() || result.is_infinite();
      (result, now_special && !was_special)
  }
  ```

  Same pattern for `-`, `*`, `/`. When `newly_special` is `true`, emit a `Note { kind: NoteKind::Info, message: format!("produced {}", classify(result)) }` (where `classify` returns `"NaN"`, `"+Inf"`, or `"-Inf"`) — the trace does NOT halt.

  Update `src/ui.rs::render_value` to handle the float special-case rendering (`NaN_f64`, `+Inf_f32`, `-Inf_f64`). Normal float values render as `format!("{value}_{kind_name}")` — Rust's `Display for f64` produces sensible defaults (`5` for whole numbers, `2.5` for non-whole).

- [X] T013 [US2] Add float unit tests in `src/typeck.rs::tests` and `src/eval.rs::tests`. Cover:

  - typeck: `let x: f64 = 2.5;` works; `let y: f32 = 1.0;` works (narrowed); `let z: f64 = 5;` (untyped int via annotation) works.
  - eval: `1.0 + 2.5 = 3.5_f64`; `0.0 / 0.0` produces NaN AND emits one Info note; `1.0 / 0.0` produces `+Inf` AND emits one Info note; `(-1.0) / 0.0` produces `-Inf` + Info note; propagation case: `let a = 0.0/0.0; let b = a + 1.0;` — `b` is NaN but only ONE Info note fires (from the `a` computation, not from propagation).

  For NaN equality in assertions, use the pattern documented in `quickstart.md` — `format!("{:?}", v)` or destructure-and-`is_nan()`. Don't use `assert_eq!` on NaN.

**Checkpoint**: typing `fn main() { let r: f64 = 3.14; }` compiles. `let n: f64 = 0.0/0.0;` runs to completion with NaN info note. Float arithmetic doesn't halt the trace.

---

## Phase 5: User Story 3 — M01–M05 byte-identical preservation (Priority: P1)

**Goal**: M01 + M02 snapshot tests pass byte-identically (they don't reference `Value`). M03 snapshots re-baseline mechanically (Value's Debug format changes). Lib tests (cursor + pipeline) pass with the updated Value form.

**Independent Test**: `cargo test --test m01 --test m02` exits 0 with zero snapshot drift. `cargo test --test m03` passes against re-baselined snapshots whose diff is a uniform `Int(N)` → `Int { kind: I32, bits: N }` rewrite.

### Implementation

- [X] T014 [US3] Re-baseline the M03 snapshot tests: run `INSTA_UPDATE=always cargo test --test m03`. Then visually inspect every `tests/snapshots/emits_*.snap` and verify the diff matches the R-014 prediction: every `Int(N)` becomes `Int { kind: I32, bits: N }`. ReturnValue events, FrameLeave events, SlotWrite events — every site referencing a `Value::Int(N)`. Anything else changing is a bug; investigate before continuing.

- [X] T015 [US3] Run the full test suite under `-D warnings`: `RUSTFLAGS="-D warnings" cargo test`. Verify all suites pass: m01 (8 byte-identical), m02 (16 byte-identical), m03 (8 re-baselined), lib (~30 with new typeck + eval tests for US1 + US2). Then run `cargo test --test m01 && cargo test --test m02` separately to verify byte-identical (no `.snap.new` files). M03 should have no `.snap.new` after T014's accept.

**Checkpoint**: full test suite passes; M01/M02 byte-identical; M03 re-baselined with predictable diff.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: ship the 3 reference samples + dropdown entries; verify SC-007 (bundle size) + SC-008 (zero warnings); audit log; stage.

- [X] T016 [P] Create the 3 M03.2 reference sample files in both `tests/samples/` and `web/samples/` (6 files total, identical content per pair):

  - `m03_2_basic_u8.rs`:
    ```rust
    fn main() {
        let a: u8 = 5;
        let b: u8 = 3;
        let c: u8 = a + b;
    }
    ```
  - `m03_2_u8_overflow.rs`:
    ```rust
    fn main() {
        let a: u8 = 250;
        let b: u8 = a + 10;
    }
    ```
  - `m03_2_float_nan.rs`:
    ```rust
    fn main() {
        let a: f64 = 0.0;
        let b: f64 = a / a;
    }
    ```

  Match the trailing-newline + 4-space-indent formatting of the existing `m03_*.rs` files.

- [X] T017 [P] In `web/index.html`, add 3 new dropdown options after the existing M03/M05 entries:

  ```html
  <option value="m03_2_basic_u8">u8 arithmetic (M03.2)</option>
  <option value="m03_2_u8_overflow">u8 overflow (M03.2)</option>
  <option value="m03_2_float_nan">f64 NaN (M03.2)</option>
  ```

  Order is "happy path → overflow → float" within the M03.2 group.

- [X] T018 [P] Verify SC-007 (bundle size ≤ +5% vs M05 baseline 63,144 B gzipped) AND SC-008 (zero warnings). Commands:
  - `RUSTFLAGS="-D warnings" cargo build --release` — clean host build.
  - `RUSTFLAGS="-D warnings" cargo test` — full test suite clean.
  - `cargo build --release --target wasm32-unknown-unknown` — WASM clean.
  - `gzip -kc target/wasm32-unknown-unknown/release/rustviz.wasm | wc -c` — must be ≤ 66,302 B (63,144 × 1.05). Adding 14 enum variants + arithmetic dispatch should add < 3 KB gzipped given the unified Value form keeps dispatch compact. If exceeded, investigate.

- [X] T019 Run final clean verification: `cargo clean && RUSTFLAGS="-D warnings" cargo build --release && RUSTFLAGS="-D warnings" cargo test && cargo build --release --target wasm32-unknown-unknown`. Full pipeline must pass clean from scratch.

- [X] T020 Append post-implementation audit log to `specs/008-m03-2-scalar-lattice/checklists/requirements.md` (mirror the M01–M05 + M03.1 pattern). Table covering SC-001 through SC-008. Mark SC-001 / SC-002 / SC-005 (browser verification) as **DEFERRED to maintainer** per the UI QA-split convention. Document the per-snapshot diff (every `Int(N)` → `Int { kind: I32, bits: N }`); document any QA-driven follow-ups discovered during the maintainer's pass.

- [X] T021 Stage all changed files:

  ```bash
  git add Cargo.toml Cargo.lock \
          src/parse/token.rs src/parse/lexer.rs src/parse/ast.rs src/parse/parser.rs \
          src/typeck.rs src/event.rs src/eval.rs src/ui.rs src/lib.rs \
          tests/snapshots/emits_*.snap tests/samples/m03_2_*.rs web/samples/m03_2_*.rs \
          web/index.html \
          specs/004-m03-event-eval/contracts/m03-api.md specs/008-m03-2-scalar-lattice/ \
          CLAUDE.md
  ```

  Cargo.toml/Cargo.lock likely unchanged; include defensively. Run `git status` and report. **Do not commit** — maintainer's QA pass happens between stage and commit per the UI QA-split convention.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: no dependencies.
- **Phase 2 (Foundational)**: T002 parallel to T003 (different files). T004 depends on T003 (Ty embeds IntKind/FloatKind). T005 depends on T003 (Value embeds them). T006 depends on T004 + T005 (refactors call sites for both). All foundational tasks blocking US1 + US2.
- **Phase 3 (US1)**: depends on Phase 2 complete. T007 → T008 → T009 sequential.
- **Phase 4 (US2)**: depends on Phase 2 complete. T010 → T011 → T012 → T013 sequential.
- **Phase 5 (US3)**: depends on Phase 3 + 4 (US3 verifies the result of both stories). T014 → T015 sequential.
- **Phase 6 (Polish)**: depends on Phases 3–5 closing. T016 / T017 / T018 parallel (different files / read-only). T019 → T020 → T021 sequential.

### Story-Level Dependencies

- US1 and US2 share the foundational restructure (Phase 2) but are otherwise independent — different parts of the pipeline (typeck/eval vs. lexer/parser/typeck/eval).
- US3 is the regression guarantee; depends on US1 + US2 landing first.

### Parallel Opportunities

- **T002 + T003**: M04 contract amendment vs. helper-enum addition. Different files. [P] ✓
- **T016 + T017 + T018**: sample files vs. dropdown HTML vs. read-only audits. Different files. [P] ✓
- **US1 + US2**: theoretically parallel (different parts of the codebase) but the integer + float typeck/eval changes interleave in the same files (`typeck.rs`, `eval.rs`) — sequential in practice for a single agent.

---

## Parallel Example: Phase 6 polish

```bash
# T016 + T017 + T018 are independent read-only or new-file work:
Task T016: "Create 3 m03_2_*.rs sample files (tests/ + web/ mirrors)"
Task T017: "Add 3 dropdown entries in web/index.html"
Task T018: "Run warnings + bundle size audits (read-only)"
```

---

## Implementation Strategy

### MVP First (US1 alone, no floats)

1. **Phase 1** (T001): pre-flight.
2. **Phase 2** (T002–T006): foundational restructure (no behavior change yet, just shape).
3. **Phase 3** (T007–T009): integer typeck + eval + tests.
4. **Phase 5 partial** (T014, T015 subset): re-baseline M03 snapshots; M01/M02 pass.
5. **STOP and VALIDATE**: `cargo test` passes; integer types work end-to-end. **At this point M03.2's integer half is shippable as a smaller milestone if desired.**

US2 (floats) can ship in the same milestone or be deferred to "M03.3 — float types" if the integer pass is enough for now. Per the maintainer's earlier decision: keep both in M03.2.

### Single-Agent Strategy

One AI agent:

1. T001 (no-op pre-flight) → T002 (contract amend) + T003 (helpers, parallel-able with T002 in practice if sequenced) → T004 (Ty) → T005 (Value) → T006 (mechanical refactor — biggest task, lots of files but pattern is uniform).
2. T007 → T008 → T009 (US1 sequence).
3. T010 → T011 → T012 → T013 (US2 sequence).
4. T014 → T015 (re-baseline + verify).
5. Phase 6: T016 + T017 + T018 (read-only or new file), T019 (final clean), T020 (audit), T021 (stage).

---

## Notes

- [P] tasks = different files, no incomplete-task dependencies.
- [Story] tag mandatory on user-story phases.
- **No new Rust deps**. No JS dep changes.
- **M03 snapshot re-baseline is expected and uniform**: every `Int(N)` becomes `Int { kind: I32, bits: N }`. M01/M02 snapshots are untouched.
- **`Value::Eq` is NOT added in M03.2** (verified: existing `Value` already only derives `PartialEq`).
- **NaN equality in tests**: don't use `assert_eq!` on NaN-containing values — use `format!("{:?}", v)` comparison or destructure-and-`is_nan()`. Documented in `quickstart.md`.
- **Lexer change is small**: only the `digits.digits` case for floats. Two-char lookahead at the `.`. No conflict with L1 syntax (L1 has no method calls).
- **`usize` / `isize` ≡ `u64` / `i64`** per FR-011 for browser determinism.
- **No literal suffixes** (`5u8`, `2.5_f32`) — annotations only.
- **No `as` casts**.
- **Type-tag rendering style** (`5_u8` vs alternatives): plan-phase chose `_T` suffix per Rust's literal-suffix convention; maintainer tunes during QA.
- **`CLAUDE.md`** may get an auto-update from `/speckit-plan` (it did in prior milestones). Include in the T021 stage list.
- **Maintainer QA between stage and commit** — same pattern as M01–M05 + M03.1.
- Avoid: implementing M06 (borrows) work in M03.2. M03.2 is strictly scalar lattice.
