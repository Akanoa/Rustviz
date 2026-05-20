---

description: "Task list for M02 — Name resolution + lightweight typeck"
---

# Tasks: M02 — Name Resolution + Lightweight Typeck

**Input**: Design documents from `/specs/003-m02-resolve-typeck/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/m02-api.md ✓, quickstart.md ✓

**Tests**: Tests ARE part of M02's deliverable — `MILESTONES.md` › M02 demo + spec SC-001 demand snapshot tests. Test tasks appear inside each user story phase.

**Organization**: tasks are grouped by user story. The MVP is US1 (Phase 3); US2 and US3 add error robustness. US2 and US3 touch different files (`resolve.rs` vs `typeck.rs`) and can run in parallel by separate agents.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no incomplete-task dependencies)
- **[Story]**: user-story tag (US1, US2, US3). Setup/Foundational/Polish phases have no tag.
- Include exact file paths (relative to repo root).

## Path Conventions

Single Rust library crate at the repo root. New M02 code under `src/resolve.rs` and `src/typeck.rs`; new tests under `tests/m02.rs` + `tests/samples/m02_*.rs` + `tests/snapshots/`.

---

## Phase 1: Setup

**Purpose**: register the new `indexmap` dependency and the M02 test target in `Cargo.toml`; create empty M02 source/test files.

- [X] T001 Edit `Cargo.toml`: add `indexmap = "2"` under `[dependencies]` (first regular dep for the crate). Add a new `[[test]]` block with `name = "m02"` and `path = "tests/m02.rs"`. Keep existing `[[test]] m01` entry unchanged. The `indexmap` choice is per `specs/003-m02-resolve-typeck/research.md` R-002; the version policy is documented in the global feedback memory "deps when needed".
- [X] T002 Create empty files: `src/resolve.rs` (one-line `//!` doc naming the module's role per `specs/003-m02-resolve-typeck/plan.md`), `src/typeck.rs` (same), `tests/m02.rs` (one-line `//!` doc). Confirm `cargo build` still succeeds with these placeholders (they'll be empty modules at this point; `lib.rs` doesn't yet declare them).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: implement the public types and function stubs every user story will consume; wire `lib.rs` re-exports per `specs/003-m02-resolve-typeck/contracts/m02-api.md`.

**⚠️ CRITICAL**: no user-story phase begins until Phase 2 closes (`cargo build` succeeds with the full public type surface in place, even if the function bodies are `unimplemented!()`).

- [X] T003 [P] In `src/resolve.rs`, define the resolve-pass public types per `specs/003-m02-resolve-typeck/data-model.md`: `pub struct BindingId(pub u32)` (newtype, derives `Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord`), `pub enum BindingKind { Fn, Let { mutable: bool }, Param }` (derives `Debug, Clone, PartialEq`), `pub struct BindingDecl { name: String, kind: BindingKind, decl_span: Span, name_span: Span }` (derives `Debug, Clone, PartialEq`), `pub struct Resolution { uses: IndexMap<Span, BindingId>, bindings: IndexMap<BindingId, BindingDecl> }` (derives `Debug, Clone, Default`). Add a `pub fn resolve(_program: &ast::Program) -> Result<Resolution, ParseError> { unimplemented!("T008 implements this") }` stub. Use `crate::parse::ast`, `crate::parse::error::ParseError`, `crate::parse::span::Span`.
- [X] T004 [P] In `src/typeck.rs`, define the typeck-pass public types per data-model.md: `pub enum Ty { I32, Bool, Unit }` (derives `Copy, Clone, Debug, PartialEq, Eq, Hash`), `pub struct FnSig { params: Vec<Ty>, ret: Ty }` (derives `Debug, Clone, PartialEq`), `pub enum BindingType { Var(Ty), Fn(FnSig) }` (derives `Debug, Clone, PartialEq`), `pub struct TypeMap { expr_types: IndexMap<Span, Ty>, binding_types: IndexMap<BindingId, BindingType> }` (derives `Debug, Clone, Default`). Add a `pub fn typeck(_program: &ast::Program, _resolution: &Resolution) -> Result<TypeMap, ParseError> { unimplemented!("T009 implements this") }` stub. Imports from `crate::parse::*` and `crate::resolve::{BindingId, Resolution}`.
- [X] T005 Edit `src/lib.rs` to declare the new modules and re-export the public surface per `specs/003-m02-resolve-typeck/contracts/m02-api.md`: add `pub mod resolve;` and `pub mod typeck;`, then `pub use resolve::{resolve, BindingDecl, BindingId, BindingKind, Resolution};` and `pub use typeck::{typeck, BindingType, FnSig, Ty, TypeMap};`.
- [X] T006 Run `cargo build`. MUST succeed with zero errors. Warnings about unused parameters in the stub bodies are acceptable at this point (resolved by Phase 3 implementations).

**Checkpoint**: full M02 public type surface compiles. `resolve()` and `typeck()` are stubs. Ready to fill in logic.

---

## Phase 3: User Story 1 — Resolve identifiers and propagate types (Priority: P1) 🎯 MVP

**Goal**: a valid L1 program goes through `parse() → resolve() → typeck()` and produces complete `Resolution` + `TypeMap` outputs. Snapshot tests pin three representative programs (shadowing across scopes, function with parameters, if-as-expression) plus one degenerate happy-path.

**Independent Test**: `cargo test --test m02` passes with snapshots showing every Ident-use mapped to a `BindingId` and every value-expression assigned a `Ty`.

### Implementation

- [X] T008 [US1] Implement the resolver in `src/resolve.rs` per `specs/003-m02-resolve-typeck/research.md` R-004 / R-005 / R-006. (a) Internal `struct Resolver { scopes: Vec<HashMap<String, BindingId>>, next_id: u32, resolution: Resolution }` with methods `push_scope`, `pop_scope`, `declare(name, kind, decl_span, name_span) -> BindingId`, `lookup(name) -> Option<BindingId>`, `record_use(span, id)`. (b) `resolve()` algorithm: phase 1 — walk `program.items`, for each `Item::Fn(decl)` allocate a `BindingId` and record it in the outermost scope (forward declaration). Phase 2 — for each fn item, push a new scope, declare each param in source order, recursively walk the body. Block traversal pushes/pops scopes. `LetStmt`s declare AFTER their initializer is resolved (so the RHS sees the outer binding, not the new one — spec FR-005 / edge-case "shadowing chain"). `Expr::Ident` lookups call `record_use` on success or return the "use of undeclared variable" error (R-007) on failure. Param-name collisions within one fn check the immediate scope and return the "duplicate parameter" error. Skip error paths for this task — return `unimplemented!()` for resolver errors that aren't on the happy path of US1 samples (the happy-path samples don't trigger errors, so this should be unreachable in practice). T013 (US2) will wire the resolver-error paths.
- [X] T009 [US1] Implement the typechecker in `src/typeck.rs` per research.md R-008 / R-009 / R-010. Internal `struct Typechecker<'a> { program: &'a Program, resolution: &'a Resolution, types: TypeMap, current_fn_ret: Option<Ty> }`. Algorithm: (a) seed `types.binding_types` for each fn binding by computing `FnSig` from the AST (param types from annotations, ret type from return-type annotation or `Unit`). For let-stmts and params, the type is filled in as bodies are checked. (b) For each fn item, set `current_fn_ret`, type-check the body block, assert the block's type matches the expected return type. (c) Block type-checking: walk stmts in order. `LetStmt`: infer init's type, validate annotation if present (else use init's type), seed `binding_types[binding_id] = Var(ty)` for the let's binding. `ExprStmt`: type-check expr; its type may be anything (discarded). After stmts, the block's type is its tail expression's type or `Unit`. (d) Expr type-checking is bottom-up per FR-007: literals → fixed types; idents → `binding_types` lookup (Var case; Fn case is unreachable outside Call callees by typeck); unary `-` → operand must be `I32`, result `I32`; unary `!` → operand `Bool`, result `Bool`; binary arithmetic `+ - * / %` → both operands `I32`, result `I32`; binary comparison `< <= > >=` → both operands `I32`, result `Bool`; binary equality `== !=` → operands match and are `I32` or `Bool`, result `Bool`; binary logical `&& ||` → both operands `Bool`, result `Bool`; paren → inner type; block → its tail-or-unit type; if-expr → check cond is `Bool`; if no else, then-block must be `Unit` and result is `Unit`; if else present, both branch blocks must have the same type and that's the result; call → callee must be a direct `Expr::Ident` (else error), look up its binding, must be `BindingType::Fn(sig)`, check arg count matches sig.params length, check each arg type against sig.params[i], result is sig.ret. Every expression node visited gets recorded in `types.expr_types[expr.span()] = ty` EXCEPT the callee Ident of a Call (R-010 / VR-11). Skip error paths in this task — return `unimplemented!()` for error cases not exercised by US1's happy-path samples. T016 (US3) will fill in typeck-error paths.
- [X] T010 [P] [US1] Create happy-path samples under `tests/samples/`:
  - `m02_shadow.rs`: `fn main() { let x = 5; let y = x + 1; let x = true; let z = x; }` — shadowing across statements; two distinct `BindingId`s for `x`; later `z` should resolve to the second `x` and have type `Bool`.
  - `m02_fn_params.rs`: `fn add(a: i32, b: i32) -> i32 { a + b } fn main() { let r = add(2, 3); }` — forward-declared `add`, params resolved, return-type validated.
  - `m02_if_expr.rs`: `fn main() { let c = true; let v = if c { 1 } else { 2 }; let w = if c { foo(); } else { bar(); }; } fn foo() {} fn bar() {}` — if-as-expression (both branches `I32`) and if-as-statement-with-else (both branches `Unit`). Includes forward-references to `foo`/`bar`.
  - `m02_simple.rs`: `fn main() { let x = 2 + 3; }` — minimal happy-path probe for sanity.
- [X] T011 [US1] Implement `tests/m02.rs`: follow `tests/m01.rs` structure. Define `fn analyze_sample(name: &str) -> AnalyzeResult` that parses the sample, runs `resolve()`, then `typeck()`, returning a struct/tuple holding both outputs (or an error). Use a `sample_test!` macro identical in shape to M01's. Snapshot via `insta::assert_debug_snapshot!` with `snapshot_path => "snapshots"` and `prepend_module_to_snapshot => false`. Add four `#[test]` functions: `resolves_and_types_shadow`, `resolves_and_types_fn_params`, `resolves_and_types_if_expr`, `resolves_and_types_simple`.
- [X] T012 [US1] Run `INSTA_UPDATE=always cargo test --test m02`. First run will create unreviewed snapshots at `tests/snapshots/resolves_and_types_*.snap.new`, which `INSTA_UPDATE=always` immediately accepts. Visually read each `.snap` and confirm: (a) every `Expr::Ident` use site appears in `Resolution.uses`; (b) shadowing in `m02_shadow.snap` produces two distinct `BindingId`s for `x`; (c) `m02_fn_params.snap` shows `add` with `FnSig { params: [I32, I32], ret: I32 }`; (d) `m02_if_expr.snap` shows the if-expr's type as `I32` and the if-stmt's type as `Unit`. Re-run `cargo test --test m02` without the env var to confirm passing.

**Checkpoint**: 4 happy-path snapshots pass. The MVP ships: M02 produces complete `Resolution` + `TypeMap` for valid L1 programs.

---

## Phase 4: User Story 2 — Undeclared variable errors (Priority: P1)

**Goal**: invalid programs that reference undeclared identifiers produce a single `ParseError` whose message names the identifier and whose span points at the use site.

**Independent Test**: `cargo test --test m02` adds 2 more passing tests (`m02_undeclared`, `m02_dup_param`). Each snapshot shows `Err(ParseError { ... })` with the expected message and span.

### Implementation

- [X] T013 [US2] Replace the resolver's happy-path `unimplemented!()` stubs in `src/resolve.rs` with real error returns: (a) when `Expr::Ident::name` has no in-scope `BindingId`, return `ParseError { message: format!("use of undeclared variable `{name}`"), span: <ident span> }`. (b) when declaring a parameter and the immediate scope already contains a binding with that name, return `ParseError { message: format!("duplicate parameter `{name}`"), span: <second param's name span> }`. Confirm stop-at-first-error: in a function with two undeclared idents, only the first is reported. Touch only `src/resolve.rs`.
- [X] T014 [P] [US2] Create error samples under `tests/samples/`:
  - `m02_undeclared.rs`: `fn main() { let y = z + 1; }` — `z` not in scope.
  - `m02_undeclared_first.rs`: `fn main() { let y = z + w; }` — two undeclared idents; only the first (`z`) should be reported.
  - `m02_dup_param.rs`: `fn f(x: i32, x: i32) -> i32 { x }` — duplicate parameter name; error at the second `x`.
- [X] T015 [US2] Extend `tests/m02.rs` with three test functions: `errors_on_undeclared`, `errors_on_first_undeclared`, `errors_on_duplicate_param`. Each calls `analyze_sample` and snapshots the result. Run `INSTA_UPDATE=always cargo test --test m02`; verify each snapshot is `Err(ParseError { ... })` with the expected message substring (`"use of undeclared variable"` or `"duplicate parameter"`) and a non-empty span pointing at the right token.

**Checkpoint**: 7 tests pass total (4 happy + 3 resolver-error).

---

## Phase 5: User Story 3 — Type mismatch errors (Priority: P2)

**Goal**: invalid programs trigger typeck errors per the R-011 catalog. Snapshots pin at least the four spec acceptance scenarios (annotation mismatch, operator mismatch, return-type mismatch, non-bool condition) plus the L1-callee restriction.

**Independent Test**: `cargo test --test m02` adds 5+ more passing tests, each snapshotting a distinct error case.

### Implementation

- [X] T016 [US3] Replace the typechecker's happy-path `unimplemented!()` stubs in `src/typeck.rs` with real error returns per `specs/003-m02-resolve-typeck/research.md` R-011. Each error returns a `ParseError` with a message naming the expected and found types (or operator and operand types) and a span. The catalog:
  - **Annotation mismatch** (`let x: T = init;` with `type_of(init) != T`): span at init's expression. Message: `"expected `T`, found `U`"`.
  - **Operator-arity mismatch** for `+ - * / %`: message `"binary operator `op` requires both operands to be `i32`, found `T` and `U`"`; span at the binary expr.
  - **Comparison-operand mismatch** (`< <= > >=`): similar message, requires `i32`.
  - **Equality-operand mismatch** (`== !=`): operands must match (both `i32` or both `bool`); message describes the mismatch.
  - **Logical-operand mismatch** (`&& ||`): message `"binary operator `op` requires both operands to be `bool`, found `T` and `U`"`.
  - **Unary-operand mismatch**: `-` requires `i32`, `!` requires `bool`.
  - **`if` condition type**: message `"`if` condition must be `bool`, found `T`"`; span at cond.
  - **`if` branch mismatch**: message `"branches of `if` have different types: `T` vs `U`"`; span at the if expr.
  - **`if` without else used as value**: message `"`if` without `else` has type `()`; cannot use as a value of type `T`"`; span at the if expr.
  - **Function return-type mismatch**: span at body's tail expression (or fn body span if empty); message `"function returns `T`, but body has type `U`"`.
  - **Non-Ident callee**: span at the callee expression; message `"L1 only supports direct function calls (callee must be a function name)"`.
  - **Call to non-function**: when callee binding is `BindingType::Var(_)`; span at callee; message `"`name` is not a function"`.
  - **Call arity mismatch**: span at the call expression; message `"function `name` expects N argument(s), found M"`.
  - **Argument type mismatch**: span at the offending argument; message `"argument N: expected `T`, found `U`"`. Touch only `src/typeck.rs`.
- [X] T017 [P] [US3] Create typeck-error samples under `tests/samples/`:
  - `m02_type_mismatch.rs`: `fn main() { let x: bool = 5; }`
  - `m02_op_mismatch.rs`: `fn main() { let x = 5 + true; }`
  - `m02_if_cond.rs`: `fn main() { let v = if 5 { 1 } else { 2 }; }`
  - `m02_if_branch.rs`: `fn main() { let v = if true { 1 } else { false }; }`
  - `m02_ret_mismatch.rs`: `fn f() -> i32 { true } fn main() { let _ = f(); }`
  - `m02_call_arity.rs`: `fn add(a: i32, b: i32) -> i32 { a + b } fn main() { let _ = add(1); }`
  - `m02_arg_mismatch.rs`: `fn negate(b: bool) -> bool { !b } fn main() { let _ = negate(5); }`
  - `m02_non_fn_call.rs`: `fn main() { let x = 5; let _ = x(); }`
  - `m02_non_ident_callee.rs`: `fn main() { let _ = (5)(); }`
- [X] T018 [US3] Extend `tests/m02.rs` with one test function per sample above. Each calls `analyze_sample` and snapshots the result. Run `INSTA_UPDATE=always cargo test --test m02`; visually verify each snapshot is `Err(ParseError { ... })` whose message matches the catalog and whose span is the right token.

**Checkpoint**: 16 tests pass total (4 happy + 3 resolver-error + 9 typeck-error). M02 exit criteria fully met.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: verify cross-cutting success criteria (SC-002, SC-003, SC-005, SC-006, SC-007, SC-008) and close out the audit log.

- [X] T019 [P] Verify SC-007: run `RUSTFLAGS="-D warnings" cargo build --release` and `RUSTFLAGS="-D warnings" cargo test --test m02`. Both MUST exit clean. Fix any warnings at the root cause.
- [X] T020 [P] Verify SC-006 (LOC cap): run `find src/resolve.rs src/typeck.rs -name '*.rs' -print0 | xargs -0 wc -l` and confirm the total is ≤ 1500. If exceeded, identify the bloat — likely typeck (R-011 catalog has many error sites). If the work is legitimately L1-scope and the cap was wrong, bump it with rationale appended to the audit log.
- [X] T021 [P] Verify SC-002 + SC-003 + SC-005 (snapshot quality + determinism): re-read every snapshot under `tests/snapshots/resolves_and_types_*.snap` and `tests/snapshots/errors_on_*.snap` and `tests/snapshots/<typeck-error-tests>.snap`. Confirm: (a) every successful-parse snapshot has `Resolution.uses` entries for every Ident, (b) every successful-parse snapshot has `TypeMap.expr_types` entries for every value-producing Expr, (c) BindingType entries exist for every BindingId in Resolution.bindings. Then re-run `cargo test --test m02` twice; verify no `.snap.new` files appear (determinism).
- [X] T022 [P] Verify SC-008 (M01 regression): run `cargo test --test m01`. MUST pass with all 8 M01 tests green (no snapshot drift). If any M01 snapshot changed, that's a regression — M02 must not modify M01 outputs.
- [X] T023 Append post-implementation audit log to `specs/003-m02-resolve-typeck/checklists/requirements.md` (mirroring M01's pattern): a table of SC-001 through SC-008 with PASS/FAIL + notes, any deviations from research/data-model/contract, and the test summary (output of `cargo test --test m02`).
- [X] T024 Run final clean verification: `cargo clean && RUSTFLAGS="-D warnings" cargo build --release && RUSTFLAGS="-D warnings" cargo test`. The full test suite (m01 + m02) MUST pass.
- [X] T025 Stage the changed files: `git add Cargo.toml Cargo.lock src/resolve.rs src/typeck.rs src/lib.rs tests/m02.rs tests/samples/m02_*.rs tests/snapshots/ specs/003-m02-resolve-typeck/ CLAUDE.md`. Run `git status` and report. **Do not commit** — that's the maintainer's call. **Note for this milestone**: the `git add` line includes `CLAUDE.md` to avoid the M01 oversight where the speckit auto-update was missed in the staging list and required a follow-up commit.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: no dependencies. T001 (Cargo.toml) before T002 (file scaffolding).
- **Phase 2 (Foundational)**: depends on Phase 1. T003 + T004 parallel (different files); T005 depends on both; T006 depends on T005.
- **Phase 3 (US1)**: depends on Phase 2 closing clean. T008 (resolve impl) and T009 (typeck impl) are conceptually independent but the test driver (T011) requires both. T010 (samples) parallel with T008/T009. T011 depends on T008+T009+T010. T012 depends on T011.
- **Phase 4 (US2)**: depends on Phase 3. T013 (resolver errors) and T014 (samples) parallel. T015 depends on both.
- **Phase 5 (US3)**: depends on Phase 3 (not Phase 4 — touches a different file). Can run concurrently with Phase 4. T016 (typeck errors) and T017 (samples) parallel. T018 depends on both.
- **Phase 6 (Polish)**: depends on Phases 4 and 5 closing. T019–T022 parallel (all read-only); T023–T025 sequential.

### Story-Level Dependencies

- US1 strictly first (the rest extend US1's resolver / typechecker with error paths).
- US2 touches `resolve.rs`; US3 touches `typeck.rs`. They can run in parallel after US1.

### Parallel Opportunities

- **T003 vs T004**: different files, independent. [P] ✓
- **T008 vs T009**: different files. The bodies are conceptually sequential (typeck consumes Resolution), but the *code paths* are independent — the typechecker just reads Resolution as input, it doesn't need to know how resolver was implemented. Mark sequential for safety; in practice an agent can develop them in parallel by mocking Resolution.
- **T010 vs T008/T009**: samples are inert text files. [P] ✓
- **Phase 4 vs Phase 5**: different files. [P] for parallel agents.
- **T019–T022**: read-only audits. [P] ✓

---

## Parallel Example: Phase 2 Foundational

```bash
# Different files, no overlap:
Task T003: "Define BindingId/BindingKind/BindingDecl/Resolution + resolve() stub in src/resolve.rs"
Task T004: "Define Ty/FnSig/BindingType/TypeMap + typeck() stub in src/typeck.rs"

# After both:
Task T005: "Re-export public surface in src/lib.rs"
Task T006: "cargo build verification"
```

## Parallel Example: Phases 4 and 5 concurrently (post-US1)

```bash
# Agent A — US2 (resolve errors)
Task T013: "Add resolver error returns to src/resolve.rs"
Task T014: "Create m02_undeclared* + m02_dup_param samples"
Task T015: "Add resolver-error tests + accept snapshots"

# Agent B — US3 (typeck errors), running concurrently
Task T016: "Add typeck error returns to src/typeck.rs"
Task T017: "Create m02_<typeck-error>* samples"
Task T018: "Add typeck-error tests + accept snapshots"

# Coordinate: both agents append tests to tests/m02.rs — serialize file writes
```

---

## Implementation Strategy

### MVP First (US1 only)

1. Complete **Phase 1** (T001–T002): Cargo.toml + skeleton files.
2. Complete **Phase 2** (T003–T006): public type surface compiles.
3. Complete **Phase 3** (T008–T012): resolver + typechecker happy paths + 4 snapshot tests.
4. **STOP and VALIDATE**: `cargo test --test m02` passes 4 tests; M01 tests still pass.
5. MVP ships: M02 produces Resolution + TypeMap for valid L1 programs.

### Incremental Delivery

1. **MVP** = Phases 1–3 (US1 ✓).
2. **Hardening 1** = Phase 4 (US2 ✓). Resolver errors covered.
3. **Hardening 2** = Phase 5 (US3 ✓). Typeck errors covered.
4. **Ready to commit** = Phase 6 polish closed clean (SC-001 through SC-008 verified).

### Single-Agent Strategy (current case)

One AI agent works phases sequentially:
1. Phase 1 → Phase 2. T003 and T004 done either in parallel writes or one after the other; both are short.
2. Phase 3: T008 (resolve) before T009 (typeck) since typeck's tests need a working resolver. T010 (samples) any time. T011 (driver) after both. T012 (verify).
3. Phase 4 → Phase 5 sequentially.
4. Phase 6 audits → audit log → stage.

### Parallel-Agent Strategy (if multiple agents available)

After Phase 2:
- Agent A: T008 (resolver) — owns `src/resolve.rs`.
- Agent B: T009 (typechecker) — owns `src/typeck.rs`; mocks `Resolution` with hand-built data while A is finishing.
- Agent C: T010 (samples) — owns `tests/samples/`.

After T008+T009+T010 close, T011 + T012 sequential. Then Phase 4 (Agent A) and Phase 5 (Agent B) concurrently.

---

## Notes

- [P] tasks = different files, no incomplete-task dependencies.
- [Story] tag is mandatory on user-story-phase tasks only.
- This milestone's `Cargo.toml` adds the project's first regular dependency (`indexmap`). Per the user-confirmed direction "deps when needed", this is fine. The "no parser framework" CLAUDE.md decision remains intact (M01's hand-rolled parser stays).
- Tests are integration tests under `tests/m02.rs` driving samples in `tests/samples/m02_*.rs`. Same pattern as M01.
- T025's staging list explicitly includes `CLAUDE.md` so the speckit `update-agent-context.sh` auto-update from /speckit-plan rides along with the milestone commit (avoiding the M01 follow-up-commit pattern).
- If T008 or T009 reveal that the M02 public API in `data-model.md` is wrong, update `data-model.md` AND `contracts/m02-api.md` in lock-step. The contract is what M03 will rely on.
- Do not commit until T025 reports clean. Committing is the maintainer's explicit action.
- Avoid: putting M03 (event emission) or M06+ (borrow checking) work into M02. The roadmap is the contract.
