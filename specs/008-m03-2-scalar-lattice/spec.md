# Feature Specification: M03.2 — Scalar Lattice Expansion (integer + float types)

**Feature Branch**: `008-m03-2-scalar-lattice`
**Created**: 2026-05-22
**Status**: Draft
**Input**: User description: "M03.2"

**Authoritative scope source**: [`MILESTONES.md` › M03.2 — Scalar lattice expansion (integer + float types)](../../MILESTONES.md). The In scope / Out of scope / Entry criteria / Exit criteria / Demo lines in that block are the contract for this feature. This spec elaborates user stories, requirements, and success criteria; it does not redefine scope.

## User Scenarios & Testing *(mandatory)*

M03.2 is the project's **second revision milestone** (after M03.1). It patches M03's type lattice — the previously closed `Ty ∈ {I32, Bool, Unit}` set under-implements CLAUDE.md's "L1 primitives" wording. After M05's QA, the practical gap surfaced: typing `let count: u32 = 100;` in the live editor fails with `Typeck error: unknown type 'u32'`. M03.2 widens the lattice to the full Rust integer family + the two IEEE 754 float types, with overflow detection for integers and NaN/Inf surfacing for floats.

### User Story 1 — Common integer types work end-to-end (Priority: P1)

A learner types `fn main() { let count: u32 = 100; let byte: u8 = 5; }` (or any of the 12 Rust integer types) in the editor. The pipeline compiles successfully; the stacks panel shows the slots with their type-tagged values (e.g. `count = 100_u32`, `byte = 5_u8`). The learner can perform basic arithmetic within a type (`let next: u32 = count + 1;`) and observe the result.

**Why this priority**: this is the milestone's headline value — the lattice gap is the user-visible bug that triggered M03.2. P1.

**Independent Test**: type `fn main() { let n: u8 = 250; let m: u16 = 1000; }` in the live editor, observe both slots in the stacks panel with the correct type suffixes. Step through; values displayed as `250_u8` and `1000_u16`.

**Acceptance Scenarios**:

1. **Given** any of the 12 integer types as a type annotation (`let x: T = N`), **When** the pipeline runs, **Then** typeck and evaluate succeed and the slot displays `N_T` in the stacks panel.
2. **Given** same-type arithmetic (e.g. `let x: u32 = 5; let y: u32 = x + 3;`), **When** stepped through, **Then** `y` displays as `8_u32`.
3. **Given** cross-type arithmetic (e.g. `let a: u8 = 1; let b: i32 = 2; let c = a + b;`), **When** the pipeline runs, **Then** typeck fails with an error spanning the mismatched operand and a message identifying the expected vs. found type.
4. **Given** an integer that overflows the destination type (e.g. `let x: u8 = 250; let y = x + 10;`), **When** the trace reaches the arithmetic step, **Then** a `Note { kind: RuntimeError, message: "u8 overflow: …" }` event halts the trace.

---

### User Story 2 — Float types work with NaN / Inf surfaced as Info notes (Priority: P1)

A learner types `fn main() { let ratio: f64 = 3.14; let zero: f64 = 0.0; let div: f64 = 1.0 / zero; }`. The pipeline succeeds. The stacks panel shows `ratio = 3.14_f64`, `zero = 0_f64` (or `0.0_f64`), and `div = +Inf_f64`. An `Info` note announces "produced +Inf" at the moment `div` is bound. The trace does NOT halt — `Inf` is valid Rust.

**Why this priority**: floats have distinct semantics from integers (overflow → ±Inf, 0.0/0.0 → NaN, neither halts), and the project is pedagogical — these special values are worth surfacing rather than hiding. P1.

**Independent Test**: type a snippet producing `NaN` (e.g. `let x: f64 = 0.0; let y: f64 = x / x;`); observe `y = NaN_f64` in the stacks panel and an info note "produced NaN" or similar. Trace continues to completion.

**Acceptance Scenarios**:

1. **Given** an `f32` or `f64` annotation (`let x: f64 = 2.5`), **When** the pipeline runs, **Then** the slot displays as `2.5_f64` in the stacks panel.
2. **Given** float division by zero (`let a: f64 = 0.0; let b: f64 = 1.0 / a;`), **When** the trace reaches that step, **Then** `b` is bound to `+Inf_f64` AND an info note describes the special value. The trace continues normally (no `RuntimeError`).
3. **Given** `NaN`-producing arithmetic (`let n: f64 = 0.0 / 0.0;`), **When** the trace reaches that step, **Then** `n` is `NaN_f64` AND an info note announces the NaN. Trace continues.
4. **Given** float overflow (e.g. multiplying very large `f32`s), **When** the result exceeds `f32::MAX`, **Then** the slot displays `+Inf_f32` or `-Inf_f32` with an info note. Trace continues.
5. **Given** float arithmetic between matching types (`let a: f64 = 1.5; let b: f64 = 2.5; let c: f64 = a + b;`), **When** stepped, **Then** `c = 4_f64` (or `4.0_f64`) — no notes, no halt.

---

### User Story 3 — Existing M01–M05 behavior is preserved (Priority: P1)

The maintainer runs the full test suite + manual QA after M03.2 lands. M01, M02, and existing M03 sample snapshots are byte-identical (they used only `i32`/`bool`/`()` which are untouched). M04 visualization works for all new types (the stacks panel renders the type tag in addition to the value). M05's live pipeline accepts source using new types without code changes beyond the lattice extension itself.

**Why this priority**: 5 shipped milestones rely on the existing type lattice. A revision must not regress them. P1.

**Independent Test**: `cargo test` passes the full suite. M05 page: select each of the existing M03/M04/M05 samples in the dropdown and confirm they render identically to pre-M03.2 behavior.

**Acceptance Scenarios**:

1. **Given** M03.2 lands on `main`, **When** `cargo test --test m01 --test m02 --test m03` runs, **Then** all three exit 0 with byte-identical snapshots (no `.snap.new` files appearing).
2. **Given** M03.2 lands, **When** the M05 page is loaded and existing samples (e.g. `m03_fn_call`, `m05_minimal`) are stepped through, **Then** their stacks-panel rendering is unchanged. (The new type-tag suffix applies uniformly, so `5_i32` instead of just `5`; this is the only intentional visual difference, see SC-006.)
3. **Given** M03.2 lands, **When** the lib unit tests run, **Then** the existing Cursor + pipeline tests all pass; new tests cover the new types.

---

### Edge Cases

- **Untyped integer literal in annotated `let`** (`let x: u8 = 5;`): the literal `5` has no type suffix but the annotation narrows it. Plan-phase confirms the inference path (likely: parse `5` as a generic int literal, typeck binds it to the annotated `u8`, evaluator stores `5_u8`).
- **Untyped float literal in annotated `let`** (`let x: f64 = 2.5;`): the literal `2.5` is recognized as a float by the parser (needs a `.` to distinguish from int). Plan-phase confirms.
- **Integer literal too large for type** (`let x: u8 = 300;`): typeck error at literal site ("literal out of range for u8"). NOT a runtime overflow; the literal itself is invalid for the annotated type.
- **Negative literal for unsigned type** (`let x: u8 = -1;`): typeck error. The `-` operator on an unsigned integer is also a typeck error (unsigned doesn't impl `Neg`).
- **Cross-type arithmetic without annotation** (`let a: u8 = 1; let b: i32 = 2; let c = a + b;`): typeck error on the `+` operator, span on the mismatched operand.
- **Implicit i32 default for un-annotated literals**: if a program writes `let x = 5;` without an annotation, `x` is `i32` per current M03 behavior. M03.2 preserves this default — the new types only kick in via explicit annotations or function signatures.
- **`usize`/`isize` semantics**: width depends on platform. For determinism in tests + browser, treat as `u64`/`i64` respectively (most common 64-bit case). Document this.
- **Integer overflow halts the trace; float Inf/NaN continues**: the asymmetry is pedagogically meaningful. Integer overflow in real Rust panics in debug mode (a bug to fix); float Inf/NaN is valid Rust behavior to observe.
- **Multiple NaN/Inf in one trace**: the Info note fires once per producing-expression (per binding), not on every subsequent use of the NaN/Inf value. So `let a: f64 = 0.0/0.0; let b: f64 = a + 1.0;` emits one Info note for `a`; `b`'s computation doesn't emit a redundant note even though `b` is also `NaN`.
- **`Value` no longer derives `Eq`**: introducing floats means the unified `Value` type drops `Eq` (only `PartialEq`). Any downstream code that relied on `Eq` (HashMap keys, etc.) refactors. Same for any types embedding `Value`.
- **Comparison operators with NaN**: per Rust semantics, `NaN == NaN` is `false`, `NaN < 1.0` is `false`, etc. The visualizer follows Rust's `PartialOrd` semantics; no special pedagogical UX for NaN comparison ordering.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST extend the `Ty` enum to include 12 new integer variants (`I8`, `I16`, `I32`, `I64`, `I128`, `U8`, `U16`, `U32`, `U64`, `U128`, `ISize`, `USize`) and 2 new float variants (`F32`, `F64`). All 14 are Copy (`Ty::is_copy()` returns `true`).
- **FR-002**: System MUST recognize the 14 new type names as valid type annotations in `let` bindings, function parameters, and function return types. Annotation-driven only; no literal type suffixes (`5u8`, `2.5_f32`) in this milestone.
- **FR-003**: System MUST extend the `Value` representation to hold typed integer and typed float values. The wire format (JSON) MUST include enough discriminator to recover the type tag on the consumer side (used by `render_value` to display the suffix).
- **FR-004**: System MUST detect integer overflow on arithmetic operators (`+`, `-`, `*`). On overflow, the evaluator emits a `Note { kind: RuntimeError, message: "<type> overflow: <op> <a> <b> exceeds <type>::MAX" }` and halts the trace. (Same pedagogical pattern as div-by-zero.)
- **FR-005**: System MUST allow float arithmetic to produce `±Inf` and `NaN` without halting the trace. The evaluator emits a `Note { kind: Info, message: "produced <NaN|+Inf|-Inf>" }` once per binding that first produces such a value. Subsequent operations propagating the special value MUST NOT re-emit the note.
- **FR-006**: System MUST treat cross-type arithmetic (e.g. `u8 + i32`, `i32 + f64`) as a typeck error with a span on the mismatched operand and a message identifying the expected vs. found type.
- **FR-007**: System MUST treat integer literals that don't fit the annotated type as a typeck error (e.g. `let x: u8 = 300;` is invalid because `300 > u8::MAX`).
- **FR-008**: System MUST treat the unary `-` operator applied to an unsigned-type expression as a typeck error (`let x: u8 = -1;` is invalid because `u8` doesn't impl `Neg`).
- **FR-009**: System MUST recognize untyped float literals with a decimal point (`2.5`, `0.0`, `3.14`) as float-typed expressions. Specific type (`f32` vs. `f64`) is determined by the annotation or function signature; if no annotation, default to `f64`. Untyped integer literals (`5`, `100`) keep the existing `i32` default per M03.
- **FR-010**: System MUST display the type tag suffix when rendering values in the stacks panel (e.g. `5_u8`, `2.5_f64`, `NaN_f64`, `+Inf_f32`). Existing `i32` values now display as `5_i32` instead of just `5`. Existing `bool` and `()` values are unchanged.
- **FR-011**: System MUST treat `usize` and `isize` as `u64` and `i64` respectively for the visualizer (deterministic platform-independent width).
- **FR-012**: System MUST ship at least 3 new reference programs (`tests/samples/m03_2_*.rs` + `web/samples/m03_2_*.rs`) covering: (a) basic non-`i32` integer arithmetic, (b) integer overflow producing a runtime-error halt, (c) float arithmetic involving NaN or Inf with the Info note surfaced.

### Key Entities

- **Integer Ty variants**: 12 new variants on the closed-then-relaxed `Ty` enum (closed-with-revisions per M03.1's precedent).
- **Float Ty variants**: 2 new variants (`F32`, `F64`).
- **`Value` (modified)**: unified type holding any of the new variants. Plan-phase decides between per-type variants and a single `{ kind, bits }` form. JSON shape grows additively; M03's contract is amended.
- **`Note { kind: Info }` for float specials**: existing `Note` variant + `Info` kind, used for NaN/Inf surfacing without halt. Already part of the M03 protocol; M03.2 just uses it in new situations.
- **Integer literal value**: when un-annotated, defaults to `i32` (current M03 behavior). When annotated with a different integer type, the literal is interpreted in that type's range (or rejected if out of range).
- **Float literal value**: any decimal-containing literal. Defaults to `f64` if no annotation; matches the annotation otherwise.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After M03.2 ships, the M05 page accepts source code using any of the 14 new types as a type annotation and produces a trace within the existing 1-second SC-001 latency budget. No "unknown type" typeck errors for any of the 14 new types.
- **SC-002**: For any integer type, attempting an arithmetic operation that exceeds the type's range produces a `Note { kind: RuntimeError }` halting the trace. Verified for at least 3 integer types (e.g. `u8 + u8`, `i32 - i32`, `i64 * i64`).
- **SC-003**: For `f32` and `f64`, division by zero produces `±Inf` and emits an `Info` note. `0.0/0.0` produces `NaN` and emits an `Info` note. The trace does NOT halt in either case. Verified by automated tests.
- **SC-004**: Cross-type arithmetic (e.g. `u8 + i32`, `i32 + f64`) is a typeck error with a span on the mismatched operand. Verified by automated tests.
- **SC-005**: Existing M01, M02, M03 snapshot tests pass byte-identically. M04 and M05 manual QA show no regressions (the only intentional visual change is the new type-tag suffix on previously-rendered values — `5` becomes `5_i32`).
- **SC-006**: At least 3 new `m03_2_*.rs` reference programs ship — at minimum: (a) integer with non-`i32` arithmetic, (b) integer overflow halt, (c) float Inf or NaN with info note.
- **SC-007**: WASM bundle size growth ≤ 5% vs M05 baseline (63,144 B gzipped). Adding 12 + 2 enum variants and their arithmetic should not significantly bloat WASM, especially if a unified `Value::Int { kind, bits }` representation is chosen (plan-phase).
- **SC-008**: Zero warnings under `RUSTFLAGS="-D warnings" cargo build --release` AND `cargo test`. Both host and WASM targets clean.

## Assumptions

- **Closed-enum relaxation already in place**: M03.1 relaxed the `MemEvent` enum's closed rule for revision milestones. The same relaxation applies to `Ty` and `Value` — they're closed in M03 but additive variants are permitted in revision milestones. M03.2 invokes this exception for `Ty` (+14 variants) and `Value` (shape change).
- **Plan-phase decides `Value` representation**: per-type variants vs. unified `{ kind, bits }`. The unified form is cleaner (one match per operation) but the per-type form is more obvious to read. Either works for the M05-side renderer.
- **`Eq` derive on `Value` is dropped**: floats don't impl `Eq`. Any downstream `Eq`-dependent code (HashMap keys, etc.) gets refactored to `PartialEq`. Plan-phase audits the call sites.
- **Untyped float literal recognition**: a token containing a `.` between digits (`2.5`, `0.0`) is a float literal at the lexer level. The lexer change is small but real; plan-phase confirms.
- **`usize`/`isize` ≡ `u64`/`i64`**: for browser determinism. Documented in FR-011. If pedagogically valuable later, a future revision could make these dynamic (machine-width).
- **Default-`f64` for un-annotated decimal literals**: matches Rust's default. `let x = 2.5;` is `f64`; `let x = 5;` stays `i32`.
- **Integer overflow halts; float Inf/NaN surfaces via Info note**: the asymmetry is intentional and pedagogically meaningful (overflow is a bug to fix; Inf/NaN is valid Rust to observe).
- **Type-tag rendering uses `_T` suffix**: `5_u8`, `2.5_f64`, `NaN_f64`, `+Inf_f32`, `-Inf_f64`. Visual style settles during M03.2's QA (the maintainer can tune the suffix style — `: u8` vs. `_u8` vs. `<u8>` etc. — without affecting the milestone's correctness).
- **No literal suffixes in M03.2**: writing `5u8` is NOT supported in this milestone. Learners must use `let x: u8 = 5;`. Future revision (or M04+ polish) could add suffix parsing.
- **Sized M**: 3 modules (`typeck.rs`, `event.rs`, `ui.rs`) with extension; new arithmetic dispatch; new lexer / parser case for float literals; new tests + 3 samples. ~400 LOC net change estimated.
