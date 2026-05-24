---

description: "Task list for M07.6 — Traits (declarations, impls, static dispatch via bounds)"
---

# Tasks: M07.6 — Traits (declarations, impls, static dispatch via bounds)

**Input**: Design documents from `/specs/017-m07-6-traits/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/m07-6-protocol-delta.md ✓, quickstart.md ✓

**Tests**: M03 snapshots stay byte-identical (no event-shape changes, no Ty/Value shape changes). M01/M02 may re-baseline once for `TypeParam.bound` → `bounds` Vec promotion (Debug output changes). New `cargo test --lib pipeline::tests` covering: trait decl + impl + dispatch, default method, generic bound (the headline), multi-bound, missing required method, extra method, inherent-wins-over-trait, bound-not-satisfied, ambiguous-method-in-multi-bound, impl-for-builtin. **≥ 10 new tests**. Manual M07.6 QA per the quickstart procedure.

**Organization**: 4 user stories (US1+US2+US3 P1; US4 P2). Sized XL — comparable to M07.4. ~6 source modules modified + 4 sample pairs.

**No UX checkpoint needed**: M07.6's headline pedagogy (trait dispatch via `<Point as Show>::show` frame names; bound-driven method calls inside generic bodies) flows entirely through existing renderers. The `<as>` mangled name format is a string convention on `FrameEnter.fn_name`; the existing frame-card renderer treats it as opaque text. No new visualization surface.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1/US2/US3/US4 tag, mandatory on user-story phases only
- File paths relative to repo root

## Path Conventions

~6 existing source files modified + 4 sample pairs. See `specs/017-m07-6-traits/plan.md` Project Structure.

---

## Phase 1: Setup

**Purpose**: pre-flight — confirm starting state.

- [X] T001 Verify pre-conditions: branch `017-m07-6-traits` checked out; `cargo test` from `main` passes (baseline confirmed: 152 tests).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Lexer keywords + AST surface + parser changes + TypeParam.bounds promotion + TraitRegistry/TraitImplRegistry + bound-checking + three-layer dispatch + eval-side trait-method dispatch + mangled frame name. Required by all four user stories. Single-pass cohesive scaffolding since the trait surface cuts across parser/typeck/eval.

- [X] T002 Amend M03's contract in `specs/004-m03-event-eval/contracts/m03-api.md` — append an entry under the closed-enum-with-revisions section noting M07.6 as the 10th invocation (AST-side additions only: `Item::Trait`, `ImplBlock.trait_name`, `TypeParam.bounds` promotion; no event-protocol changes). Reference `specs/017-m07-6-traits/contracts/m07-6-protocol-delta.md`.

- [X] T003 [P] In `src/parse/lexer.rs`, extend the KEYWORDS table with two new entries: `"trait"` → `TokenKind::Trait`, `"for"` → `TokenKind::For`. The `for` keyword is needed for `impl Trait for Type` syntax (not used by any prior milestone — verify it's not already present).

- [X] T004 [P] In `src/parse/token.rs`, add two new `TokenKind` variants: `Trait`, `For`. Update `TokenKind::describe()` to return `"`trait`"`, `"`for`"`.

- [X] T005 In `src/parse/ast.rs`, add new AST surface:
  - Add `pub enum Item { ..., Trait(TraitDecl) }`.
  - Add `pub struct TraitDecl { pub name: String, pub items: Vec<TraitItem>, pub span: Span }`.
  - Add `pub enum TraitItem { Required { name: String, params: Vec<Param>, return_ty: Option<Type>, span: Span }, Default { decl: FnDecl } }`.
  - Extend `ImplBlock` with `pub trait_name: Option<String>` field (None = inherent impl; Some = trait impl).
  - **Promote** `TypeParam.bound: Option<String>` → `pub bounds: Vec<String>`. Update all existing construction sites (M07.5's parse_type_params) to set `bounds: Vec::new()` (no bound) or `bounds: vec![name]` (single bound).
  - Update `item_span` in parser.rs to handle `Item::Trait` shape.

- [X] T006 In `src/parse/parser.rs`, extend parsing:
  - **`parse_item`** extension: dispatch `Trait` keyword → new `parse_trait_decl`.
  - **`parse_trait_decl`**: consume `trait`, expect ident (name), `{`, parse trait items, `}`. Items: parse like a fn but distinguish body presence by peeking `Semi` vs `LBrace` after the return type. `Semi` → `TraitItem::Required { name, params, return_ty, span }`. `LBrace` → `TraitItem::Default { decl: FnDecl }` (use existing `parse_block`).
  - **`parse_impl_block`** extension: after `impl`, peek for two-ident-with-`for`-between shape: `Ident for Ident { ... }`. If present → trait impl (`trait_name: Some(first)`, `ty_name: second`). Else inherent (`trait_name: None`, `ty_name: first`) — M07.4 behavior.
  - **`parse_type_params`** extension: after the first bound, while `Plus` follows, consume `Plus` and the next ident; push to bounds vec. Build `TypeParam { name, bounds: Vec<String>, span }`.

- [X] T007 [P] In `src/resolve.rs`, traverse the new AST surface:
  - `Item::Trait { items, .. }` arm: for each `TraitItem::Default { decl }`, call `resolve_fn(decl)` so the default body's idents resolve (treats `self` like any param). For `TraitItem::Required`, no body to walk.
  - `Item::Impl { items, .. }` (M07.4 already walks fn decls) needs no change — works for both inherent and trait impls.

- [X] T008 In `src/typeck.rs`, add Registries + phase-1 population:
  - Add `pub struct TraitRegistry { pub schemas: IndexMap<String, TraitSchema> }` and `pub struct TraitSchema { pub required_methods: IndexMap<String, FnSig>, pub default_methods: IndexMap<String, FnSig> }`.
  - Add `pub struct TraitImplRegistry { pub impls: IndexMap<(String, String), TraitImpl> }` and `pub struct TraitImpl { pub overrides: IndexMap<String, FnSig> }`.
  - Add fields to `Typechecker`: `traits: TraitRegistry`, `trait_impls: TraitImplRegistry`. Initialize in `Typechecker::new`.
  - Extend the `typeck()` Phase 1 loop: for each `Item::Trait`, register the schema (collect FnSigs for required + default methods; reject duplicates).
  - Extend phase 1 for `Item::Impl { trait_name: Some(name), .. }`: register a `TraitImpl` entry; validate that every method is on the trait (reject "extra method"); validate that every required method is implemented (reject "missing required method"); reject duplicate `(trait, type)` pairs.
  - Extend phase 1's `register_struct` / `register_impl` callsites so trait registrations don't conflict with existing M07.4 logic.

- [X] T009 In `src/typeck.rs`, extend `typecheck_method_call` with third-layer dispatch:
  - After the existing M07 builtins + M07.4 user inherent impl fall-through, search `trait_impls.impls` and `traits.schemas` for the method name on the receiver's concrete type.
  - **Receiver = `Ty::Struct(name, ..)` or builtin like `Ty::Int(_)`**: iterate trait_impls; for each `(trait, type)` matching the receiver's type name, check if the trait declares the method. Return the trait's method sig.
  - **Receiver = `Ty::Param(T)`**: look up T's bounds in `current_type_params` (extended in T011); for each bound trait, check if it declares the method. First-bound match wins. Ambiguity (multiple bounds, same method name) → error suggesting UFCS.
  - **No match in any layer** → existing "no method `<name>`" error (extended to mention traits if applicable).

- [X] T010 In `src/typeck.rs`, add bound-checking at generic call sites:
  - Extend `typecheck_generic_free_call` (M07.5): after computing the substitution, for each type-param `T` in the fn's type-params, for each bound trait in `T.bounds`, verify `trait_impls.impls.contains_key(("<TraitName>", "<T_concrete.name()>"))`. Bound failure → error: `"the trait bound `<T_concrete>: <Trait>` is not satisfied"`.
  - Same plumbing for `typecheck_call` (inferred-substitution path) and for trait-impl-method generic dispatches.

- [X] T011 In `src/typeck.rs`, extend `current_type_params` to carry bounds. Promote from `Vec<Vec<String>>` (just names) to `Vec<Vec<(String, Vec<String>)>>` (each tuple is `(param_name, bound_trait_names)`). Used inside generic-fn body typecheck (T009's Ty::Param dispatch case) to look up bounds for the receiver's type-param.

- [X] T012 In `src/eval.rs`, add trait-method dispatch:
  - Add `Evaluator.trait_default_bodies: HashMap<(String, String), &'a ast::FnDecl>` and `trait_impl_bodies: HashMap<(String, String, String), &'a ast::FnDecl>`. Populate in `Evaluator::new` by walking `Item::Trait` and `Item::Impl { trait_name: Some(_), .. }`.
  - Extend `eval_method_call`'s third-layer dispatch: after builtins + inherent, look up `trait_impl_bodies[(trait, type, method)]` first; fall through to `trait_default_bodies[(trait, method)]` if no override.
  - Build the mangled frame name: `format!("<{type_name} as {trait_name}>::{method_name}")`. Pass as `display_name` to `call_decl` (M07.4/M07.5 machinery reused).
  - Default-method execution: the body is `&trait_default_bodies[(trait, method)]`. Eval enters a new frame with `self` bound to the receiver; body executes; any `self.other()` calls dispatch through the standard third-layer chain.

- [X] T013 [P] In `src/ui.rs`, no changes required — the frame-card renderer treats `fn_name` as opaque text. The `<Point as Show>::show` mangled name renders as-is. Document this as a deliberate non-change.

**Checkpoint**: `cargo build` should compile cleanly. Match-exhaustiveness will flag any `Item::Fn(_) | Item::Struct(_) | Item::Impl(_)` patterns that need `| Item::Trait(_) => { .. }` added. Fix exhaustively. `cargo test` passes: M03 byte-identical (no event-shape changes); M01/M02 possibly re-baseline once for `TypeParam.bound` → `bounds` Vec field promotion.

---

## Phase 3: User Story 1 — Trait declaration + impl + dispatch (Priority: P1) 🎯 MVP

**Goal**: `trait Show { fn show(&self) -> i32; } impl Show for Point { fn show(&self) -> i32 { self.x } } let s = p.show();` typechecks; trace contains a `FrameEnter` labeled `<Point as Show>::show`; impl block's body executes; `s = 1_i32`.

**Independent Test**: load `m07_6_trait_basic.rs`, step past `let s = p.show()`, observe the trait-method frame opens with the `<as>` mangled name, body returns 1, s lands 1_i32.

### Implementation

- [X] T014 [US1] Add 1 sample program pair: `tests/samples/m07_6_trait_basic.rs` and `web/samples/m07_6_trait_basic.rs`. Content:
  ```rust
  struct Point { x: i32, y: i32 }
  trait Show {
      fn show(&self) -> i32;
  }
  impl Show for Point {
      fn show(&self) -> i32 {
          self.x
      }
  }
  fn main() {
      let p = Point { x: 1, y: 2 };
      let s = p.show();
  }
  ```

- [X] T015 [US1] In `src/pipeline.rs` `mod tests`, add unit tests:
  - `run_pipeline_trait_basic`: asserts the trace contains a `FrameEnter { fn_name: "<Point as Show>::show", .. }`; the impl block's body runs; `s`'s SlotWrite carries `Value::Int { I32, 1 }`.
  - `run_pipeline_trait_missing_method`: source where `impl Show for Point {}` (no body) → typeck error containing "missing implementation of trait method `show`".
  - `run_pipeline_trait_extra_method`: source where the impl provides a method not on the trait → typeck error containing "method `<name>` is not on trait `Show`".

- [X] T016 [US1] In `web/index.html`, add a dropdown `<option>` for `m07_6_trait_basic.rs`.

**Checkpoint**: US1 fully functional. Trace shows `<Point as Show>::show` frame.

---

## Phase 4: User Story 2 — Default methods (Priority: P1)

**Goal**: `trait Counter { fn count(&self) -> i32; fn double(&self) -> i32 { self.count() * 2 } } impl Counter for Point { fn count(&self) -> i32 { self.x } } let v = p.double();` typechecks; trace contains TWO nested frames (outer `<Point as Counter>::double` running the trait's default body; inner `<Point as Counter>::count` running the impl override); `v = 2 * p.x`.

**Independent Test**: load `m07_6_default_method.rs`, step past `let v = p.double()`, observe the nested frames + the multiplication result.

### Implementation

- [X] T017 [US2] Add 1 sample program pair: `tests/samples/m07_6_default_method.rs` and `web/samples/m07_6_default_method.rs`. Content:
  ```rust
  struct Point { x: i32, y: i32 }
  trait Counter {
      fn count(&self) -> i32;
      fn double(&self) -> i32 {
          self.count() * 2
      }
  }
  impl Counter for Point {
      fn count(&self) -> i32 {
          self.x
      }
  }
  fn main() {
      let p = Point { x: 1, y: 2 };
      let v = p.double();
  }
  ```

- [X] T018 [US2] In `src/pipeline.rs` `mod tests`, add unit tests:
  - `run_pipeline_default_method`: asserts the trace contains a `FrameEnter` for `<Point as Counter>::double` AND a NESTED `FrameEnter` for `<Point as Counter>::count`; v's SlotWrite carries `Value::Int { I32, 2 }`.

- [X] T019 [US2] In `web/index.html`, add a dropdown `<option>` for `m07_6_default_method.rs`.

**Checkpoint**: US2 fully functional.

---

## Phase 5: User Story 3 — Generic bound (Priority: P1) 🎯 HEADLINE

**Goal**: `fn print<T: Show>(x: T) -> i32 { x.show() } let r = print(p);` typechecks (bound proves x.show() works); trace shows mangled `print::<Point>` frame containing a nested `<Point as Show>::show` frame; `r = p.x`. Call with type that doesn't impl Show → typeck error citing the bound.

**Independent Test**: load `m07_6_generic_bound.rs`, step past `let r = print(p)`, observe the nested-dispatch flow + the result.

### Implementation

- [X] T020 [US3] Add 1 sample program pair: `tests/samples/m07_6_generic_bound.rs` and `web/samples/m07_6_generic_bound.rs`. Content:
  ```rust
  struct Point { x: i32, y: i32 }
  trait Show {
      fn show(&self) -> i32;
  }
  impl Show for Point {
      fn show(&self) -> i32 {
          self.x
      }
  }
  fn print<T: Show>(x: T) -> i32 {
      x.show()
  }
  fn main() {
      let p = Point { x: 1, y: 2 };
      let r = print(p);
  }
  ```

- [X] T021 [US3] In `src/pipeline.rs` `mod tests`, add unit tests:
  - `run_pipeline_generic_bound`: asserts the trace contains a `FrameEnter` for `print::<Point>` (M07.5 monomorphization) AND a NESTED `FrameEnter` for `<Point as Show>::show`; r's SlotWrite carries `Value::Int { I32, 1 }`.
  - `run_pipeline_trait_bound_unsatisfied`: source `let r = print(5);` (with the same trait but no `impl Show for i32`) → typeck error containing "trait bound `i32: Show` is not satisfied".

- [X] T022 [US3] In `web/index.html`, add a dropdown `<option>` for `m07_6_generic_bound.rs`.

**Checkpoint**: US3 (THE HEADLINE) fully functional. The bound-driven dispatch unlocks polymorphism.

---

## Phase 6: User Story 4 — Multi-bound (Priority: P2)

**Goal**: `fn show_n_count<T: Show + Counter>(x: T) -> i32 { x.show() + x.count() } let r = show_n_count(p);` (where Point impls both Show and Counter) typechecks; trace contains nested `<Point as Show>::show` AND `<Point as Counter>::count` frames; `r = p.x + p.x` (or whatever the test setup yields).

**Independent Test**: load `m07_6_multi_bound.rs`, step past the call, observe both trait-method dispatches.

### Implementation

- [X] T023 [US4] Add 1 sample program pair: `tests/samples/m07_6_multi_bound.rs` and `web/samples/m07_6_multi_bound.rs`. Content:
  ```rust
  struct Point { x: i32, y: i32 }
  trait Show { fn show(&self) -> i32; }
  trait Counter { fn count(&self) -> i32; }
  impl Show for Point { fn show(&self) -> i32 { self.x } }
  impl Counter for Point { fn count(&self) -> i32 { self.y } }
  fn show_n_count<T: Show + Counter>(x: T) -> i32 {
      x.show() + x.count()
  }
  fn main() {
      let p = Point { x: 1, y: 2 };
      let r = show_n_count(p);
  }
  ```

- [X] T024 [US4] In `src/pipeline.rs` `mod tests`, add unit tests:
  - `run_pipeline_multi_bound`: asserts the trace contains a `FrameEnter` for `show_n_count::<Point>` AND nested frames for both `<Point as Show>::show` and `<Point as Counter>::count`; r's SlotWrite carries `Value::Int { I32, 3 }` (1 + 2).
  - `run_pipeline_trait_method_ambiguous`: source `trait A { fn name(&self) -> i32; } trait B { fn name(&self) -> i32; } ... fn foo<T: A + B>(x: T) -> i32 { x.name() }` → typeck error containing "ambiguous" and "UFCS".

- [X] T025 [US4] In `web/index.html`, add a dropdown `<option>` for `m07_6_multi_bound.rs`.

**Checkpoint**: US4 fully functional. Multi-bound + ambiguity detection both verified.

---

## Phase 7: Cross-cutting rejection / edge-case tests

**Purpose**: lock in M07.6-specific behavioral guarantees not covered by US1-US4 happy paths.

- [X] T026 In `src/pipeline.rs` `mod tests`, add unit tests for inherent-wins, builtin-impl, missing-method, extra-method, duplicate-impl:
  - `run_pipeline_trait_inherent_wins`: Point has both inherent `impl Point { fn show ... }` AND `impl Show for Point { fn show ... }`. Calling `p.show()` dispatches to the inherent (NOT the trait); frame is `Point::show`, NOT `<Point as Show>::show`. Verifies tie-breaker.
  - `run_pipeline_trait_impl_for_builtin`: `impl Show for i32 { fn show(&self) -> i32 { *self * 10 } } let s = 5.show();` (or equivalent) typechecks; frame labeled `<i32 as Show>::show`; s = 50.
  - `run_pipeline_trait_duplicate_impl`: two `impl Show for Point { ... }` blocks → typeck error citing duplicate.
  - `run_pipeline_trait_duplicate_decl`: two `trait Show {}` declarations → typeck error citing duplicate.

**Checkpoint**: ≥ 10 new M07.6 tests in total (3 US1 + 1 US2 + 2 US3 + 2 US4 + 4 rejection/edge-case = 12). Exceeds the SC floor.

---

## Phase 8: Polish & Cross-Cutting

**Purpose**: snapshot re-baselines, bundle-size check, warnings check, manual QA, doc updates.

- [X] T027 [P] Run `cargo test`. Verify M03 byte-identical. Verify M01/M02 — if `parses_full_l1.snap` etc. re-baseline for `TypeParam.bound` → `bounds` Vec promotion, accept the re-baseline (expected per plan). Re-baseline with `INSTA_UPDATE=always cargo test --test m01 --test m02`.
- [X] T028 [P] Build WASM release and measure bundle size: `cd web && trunk build --release` (wasm-opt may fail per the pre-existing M07.4 issue; measure the staged size at `dist/.stage/*.wasm`). Compare to M07.5 baseline (342,873 B). Acceptable if ≤ +25% (~429 KB). If over: candidate cuts per plan.md.
- [X] T029 [P] Run `RUSTFLAGS="-D warnings" cargo build --release` AND `cargo test`. Both clean. Fix any new warnings introduced by M07.6 changes.
- [X] T030 [P] Run `cargo clippy --all-targets -- -D warnings`. Fix any NEW lints introduced by M07.6.
- [X] T031 Manual M07.6 QA per `specs/017-m07-6-traits/quickstart.md` procedure. ~8 minutes. Step through US1–US4 in the page; verify error UX via live editing for: bound-not-satisfied (`print(5)` where i32: Show absent), missing required method, extra method in impl, duplicate impl, ambiguous method. Cycle through M01–M07.5 samples to confirm no regressions.
- [X] T032 Verify `CLAUDE.md` "Recent Changes" footer includes M07.6.
- [X] T033 Final commit prep. Merge MR note: "10th invocation of the closed-enum-with-revisions rule. AST-only additions (`Item::Trait`, `ImplBlock.trait_name`, `TypeParam.bounds`); no event-protocol changes. Third dispatch layer extends M07.4's builtin → inherent → trait fall-through. Mangled trait-method frame names use the UFCS-style `<Point as Show>::show` format."

---

## Dependencies

```text
Phase 1 (Setup)
  └─ T001 (verify baseline)

Phase 2 (Foundational) — blocks ALL user-story phases
  ├─ T002 (contract amendment, can run anytime)
  ├─ T003 [P] (lexer keywords)
  ├─ T004 [P] (token variants)
  ├─ T005 (AST surface — depends on T004)
  ├─ T006 (parser — depends on T005)
  ├─ T007 [P] (resolve — depends on T005)
  ├─ T008 (TraitRegistry + phase 1 — depends on T005)
  ├─ T009 (typecheck_method_call third-layer dispatch — depends on T008)
  ├─ T010 (bound-checking at call sites — depends on T008+T011)
  ├─ T011 (current_type_params bounds extension — depends on T005)
  ├─ T012 (eval trait dispatch + mangled frame name — depends on T008+T009)
  └─ T013 [P] (ui docstring no-op)

Phase 3 (US1: trait basic) — depends on Phase 2
  ├─ T014 (sample)
  ├─ T015 (3 unit tests)
  └─ T016 (dropdown)

Phase 4 (US2: default method) — depends on Phase 2 (independent of US1)
  ├─ T017 (sample)
  ├─ T018 (1 unit test)
  └─ T019 (dropdown)

Phase 5 (US3: generic bound, HEADLINE) — depends on Phase 2 (independent)
  ├─ T020 (sample)
  ├─ T021 (2 unit tests)
  └─ T022 (dropdown)

Phase 6 (US4: multi-bound) — depends on Phase 2 (independent)
  ├─ T023 (sample)
  ├─ T024 (2 unit tests)
  └─ T025 (dropdown)

Phase 7 (cross-cutting rejection tests) — depends on Phase 2
  └─ T026 (4 rejection tests)

Phase 8 (Polish) — depends on Phases 3-7
  └─ T027–T033 (snapshot/bundle/warnings/QA/docs/commit)
```

---

## Parallel execution opportunities

- **Phase 2**: T003 + T004 + T007 + T013 are file-disjoint [P]. T005 depends on T004; T006/T008 depend on T005; T009 depends on T008; T010/T011 file-disjoint within typeck.rs (but conceptually depend on T008/T009); T012 depends on T008+T009.
- **Phases 3/4/5/6/7**: completely independent of each other after Phase 2. Could be tackled in parallel by different agents/sessions (each touches its own sample files + a sliver of pipeline.rs tests + a one-line index.html addition).
- **Phase 8**: T027/T028/T029/T030 all parallelizable [P].

---

## Implementation strategy

**MVP scope** = **US1 only** (trait decl + impl + dispatch). Lands the foundational machinery in a single sample. ~700 LOC.

**Incremental delivery**:
1. **MVP (US1)**: Phases 1+2+3 (Setup + Foundational + US1). Trait dispatch live.
2. **+US2 (default method)**: Phase 4. Default-method routing.
3. **+US3 (generic bound, HEADLINE)**: Phase 5. The polymorphism payoff.
4. **+US4 (multi-bound)**: Phase 6. Multi-bound + ambiguity.
5. **+Rejection tests**: Phase 7. Lock in error UX.
6. **+Polish**: Phase 8. Snapshot/bundle/QA/docs.

**Recommended landing order**: ship all 4 user stories + rejection tests in one merge. The trait surface is cross-cutting (TraitRegistry + bound-checking + dispatch all interact); splitting at user-story granularity would force three rounds of Phase 2 follow-ups. Single-merge matches M07.4/M07.5 pattern.

**No UX checkpoint**: M07.6's headline pedagogy (trait dispatch + bound-proven method calls) flows through existing renderers. The `<Point as Show>::show` frame label is a string convention; the existing frame-card renderer renders it as-is. No new JS / CSS surface.

**Sequence note**: M07.6 closes Level 4's polymorphism story. After this milestone the project has shipped every "model your domain AND make it polymorphic" tool a learner needs.
