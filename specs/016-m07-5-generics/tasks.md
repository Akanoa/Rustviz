---

description: "Task list for M07.5 — Generics (`fn foo<T>(...)`, `struct Wrapper<T>`, monomorphization-visible frames)"
---

# Tasks: M07.5 — Generics (`fn foo<T>(...)`, `struct Wrapper<T>`, monomorphization-visible frames)

**Input**: Design documents from `/specs/016-m07-5-generics/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/m07-5-protocol-delta.md ✓, quickstart.md ✓

**Tests**: M02 + M03 snapshots stay byte-identical (additive AST fields with serde-default-empty). M01's `parses_full_l1.snap` may re-baseline once (depends on serializer format). New `cargo test --lib pipeline::tests` covering: generic id fn with two substitutions, generic struct, turbofish, mismatched-arg-inference error, turbofish type mismatch, multi-type-param rejection, bound-on-generic rejection, generic-call-inside-generic-fn rejection. **≥ 8 new tests**. Manual M07.5 QA per the quickstart procedure.

**Organization**: 3 user stories (US1 + US2 P1; US3 P2). Sized XL but smaller than M07.4 — no new UI rendering surface beyond `Ty::name()` substitution-aware rendering. ~5 source files modified + 3 sample pairs.

**No UX checkpoint needed**: M07.5's headline pedagogy (distinct frame names per substitution) is fully driven by `FrameEnter.fn_name` rendering — already shipped in M07.4's frame-card renderer. M07.5 just feeds different strings into it. The generic-struct type label (`Wrapper<i32>`) comes from `Ty::name()` rendering automatically. No JS / CSS / new UI surface.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1/US2/US3 tag, mandatory on user-story phases only
- File paths relative to repo root

## Path Conventions

~5 existing source files modified + 3 sample pairs. See `specs/016-m07-5-generics/plan.md` Project Structure.

---

## Phase 1: Setup

**Purpose**: pre-flight — confirm starting state.

- [X] T001 Verify pre-conditions: branch `016-m07-5-generics` checked out; `cargo test` from `main` passes (baseline confirmed: 143 tests). Note current test count for post-merge delta.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: AST extensions + parser changes + `Ty::Param` + `Ty::Struct.type_args` extension + typeck substitution machinery + eval mangled-name plumbing. Required by all three user stories. Single-pass cohesive scaffolding since the substitution surface cuts across parser/typeck/eval/ui.

- [X] T002 Amend M03's contract in `specs/004-m03-event-eval/contracts/m03-api.md` — append an entry under the closed-enum-with-revisions section noting M07.5 as the 9th invocation (additive `Ty::Param(String)` variant + additive `Ty::Struct.type_args: Vec<Ty>` field via serde-default-empty). Reference `specs/016-m07-5-generics/contracts/m07-5-protocol-delta.md`.

- [X] T003 [P] In `src/parse/ast.rs`, add new AST surface:
  - Add `pub struct TypeParam { pub name: String, pub span: Span }`.
  - Extend `FnDecl` with `pub type_params: Vec<TypeParam>` field (serde-default-empty).
  - Extend `StructDecl` with `pub type_params: Vec<TypeParam>` field (serde-default-empty).
  - Extend `Type::Path` with `pub type_args: Vec<Type>` field (serde-default-empty).
  - Extend `Expr::Path` with `pub type_args: Vec<Type>` field (serde-default-empty).
  - Extend `Expr::StructLit` with `pub type_args: Vec<Type>` field (serde-default-empty).
  - All new fields default to `Vec::new()` at construction sites in parser.rs; serde annotation: `#[serde(default, skip_serializing_if = "Vec::is_empty")]`.
  - Update Debug derives — all new fields are Debug-clean via Vec/String.

- [X] T004 In `src/parse/parser.rs`, extend parsing:
  - **`parse_fn_decl`** extension: after `expect_ident("function name")`, if `Lt` follows, parse a comma-separated `TypeParam` list until `Gt`. Each `TypeParam` is `Ident { name, span }`. Multi-element lists permitted at parse (typeck rejects); bound syntax `Ident :` permitted at parse (typeck rejects).
  - **`parse_struct_decl`** extension: same after the struct name.
  - **`parse_type`** extension: after the path identifier, if `Lt` follows, parse comma-separated `Type` args until `Gt`. Build `Type::Path { segments, type_args, span }`. The existing `Type::Generic` arm for `Box<T>` / `Vec<T>` either MIGRATES into `Type::Path` (recommended) or stays separate (legacy) — plan-phase decision; recommendation is migrate.
  - **`parse_atom`** Path extension: after a multi-segment ident (`Ident :: Ident`), if `ColonColon` then `Lt` follows, parse comma-separated `Type` args until `Gt`. Build `Expr::Path { segments, type_args, span }`. Disambiguates from `<` comparison via the `ColonColon` prefix.
  - **`parse_struct_lit`** extension: extends T012 (parse_atom's struct-lit detection) — when the path has `type_args`, the resulting `Expr::StructLit` carries them.
  - All four parse extensions are tolerant at parse-time; typeck-side rejection messages cite the M07.5-specific scope rules.

- [X] T005 [P] In `src/resolve.rs`, add traversal for the new AST surface:
  - `resolve_fn`: push a fresh type-param scope (storing just names, no BindingIds) before walking params + body. `T` inside the body resolves as a type-name via this scope; type-name resolution itself happens at typeck.
  - `resolve_expr` arms for `Expr::Path { type_args }` and `Expr::StructLit { type_args }`: don't need recursion in M07.5 (type-args restricted to concrete primitives — no nested type-args).
  - `Item::Struct { type_params, .. }`: type-params don't need top-level binding registration.

- [X] T006 In `src/typeck.rs` + `src/event.rs`, add `Ty::Param(String)` variant + extend `Ty::Struct` with `type_args`. Update:
  - `Ty::name()` → `Param(name) => name.clone()`. For `Struct { name, type_args: [] }` keep `"Point"`; for non-empty `type_args` render `format!("{}<{}>", name, type_args.iter().map(|t| t.name()).collect::<Vec<_>>().join(", "))`.
  - `Ty::is_copy()` → `Param(_) => false` (no bounds in M07.5). For `Struct { fields, .. }` keep "all fields Copy"; type_args don't affect Copy-ness (the schema's `fields` already carries the substituted concrete types when needed).
  - Add a `Ty::Struct` arm extension: every existing `Ty::Struct { name, fields }` pattern needs to add `type_args: _` (or `type_args: ..`) for exhaustiveness — fix exhaustively when the compiler flags.
  - The shared `Ty` enum lives in `typeck.rs`; `event.rs` re-uses it. The change at typeck.rs propagates to event.rs automatically.

- [X] T007 In `src/typeck.rs`, add substitution machinery:
  - Add `pub struct Typechecker.subst: Vec<HashMap<String, Ty>>` — the substitution stack. Initialize empty.
  - Add `fn apply_subst(&self, ty: &Ty) -> Ty` helper. Recursively walks `ty`:
    - `Ty::Param(name)` → look up in `self.subst.last()` (innermost binding). If found, return the concrete type. If not (no active substitution OR T not in the map), return `ty.clone()` (defensive — typeck-pass invariant should prevent this).
    - `Ty::Struct { name, fields, type_args }` → recurse on `type_args` (substitute T placeholders in instantiations like `Wrapper<T>` inside a generic fn body).
    - `Ty::Ref { inner, mutable }` → recurse on `inner`.
    - `Ty::Box(inner)`, `Ty::Vec(inner)`, `Ty::Slice(inner)`, `Ty::Array(inner, n)` → recurse on `inner`.
    - All other variants → return `ty.clone()` (Int, Float, Bool, Unit, String, Str — primitives have no substitution structure).
  - Add `TypeMap.call_substs: IndexMap<Span, Vec<(String, Ty)>>` — keyed by call-site span. Populated at typeck call sites that resolve to generic fns/methods/assoc_fns.
  - Extend `register_struct` to read `decl.type_params` into the schema. Store as `Vec<String>` of param names alongside the existing fields. The schema knows which fields' types contain `Ty::Param(_)` so call-site instantiation can substitute them.
  - Extend `register_impl_fn` and `build_fn_sig` to read `decl.type_params` similarly. The resulting `FnSig.params` and `FnSig.ret` may contain `Ty::Param(_)`; substitution applies at the call site.

- [X] T008 In `src/typeck.rs`, plumb substitution through call typecking. Three sites:
  - **`typecheck_call`**: when the resolved `FnSig` contains `Ty::Param(_)` in params or ret, infer substitution from args (direct match — first generic-typed param's `Ty::Param(name)` → `T = arg_ty`; subsequent occurrences must agree). Apply `apply_subst` to each param type before the arg-type comparison. Apply to return type before returning. Record the substitution in `call_substs[call_span]`.
  - **`typecheck_method_call`**: same plumbing for user-defined methods. Self-receiver substitution comes from the receiver's struct type's `type_args` (no inference needed — receiver's instantiation already concrete). Then args drive any additional param-T inference.
  - **`typecheck_path_call`**: same plumbing for assoc fns. Turbofish (`Point::new::<i32>(...)`) provides explicit substitution; otherwise inference from args.
  - **Turbofish path**: when `Expr::Path { type_args }` has non-empty `type_args`, BIND those to the fn's type-params positionally (skipping inference). Verify args match the substituted param types.
  - **Multi-type-param decl rejection**: when `FnSig` came from a `FnDecl` with `decl.type_params.len() > 1`, reject at the call site with "M07.5 supports a single type parameter; multi-type-param fns deferred".
  - **Bound rejection**: in `parse_param` / `parse_fn_decl` extension at T004 — actually, bounds parse with `T: Foo` shape. typeck rejects at the FnDecl registration phase with "trait bounds on generics are deferred to M07.6".
  - **Generic-call-inside-generic-fn rejection**: detect when a generic call is made inside a fn whose own `type_params` is non-empty. Reject with "nested generic calls are out of scope in M07.5".

- [X] T009 In `src/typeck.rs`, extend struct-literal typecking for generics:
  - `typecheck_struct_lit`: when the resolved struct has `type_params`, build the substitution. **Inferred case**: walk schema fields; for each field whose type contains `Ty::Param(name)`, take the substitution from the corresponding lit field's value type. Conflicts (T appears in multiple fields with mismatched lit values) → error. **Turbofish case** (`Wrapper::<i32> { v: 5 }`): `type_args` on the struct-lit drives substitution; field value types just verified against substituted field types.
  - The resulting `Ty::Struct { name, fields, type_args }` carries `type_args` populated from the substitution.

- [X] T010 In `src/eval.rs`, plumb mangled fn names through `call_decl`:
  - At the call site for generic fns (`Expr::Call`, `Expr::MethodCall`, `Expr::Path`-callee): look up `types.call_substs.get(call_span)`. If present, build the mangled name via `format!("{}::<{}>", source_name, sub_tys.iter().map(|(_, t)| t.name()).collect::<Vec<_>>().join(", "))`. Pass as `display_name` to `call_decl` (already in M07.4).
  - `Ty::Param` at eval-time defensive: if `lookup_local_value`/`SlotAlloc.ty` would ever carry `Ty::Param`, panic with "M07.5 invariant: Ty::Param escaped typeck substitution at <span>". Tests should not exercise this (typeck substitutes before).

- [X] T011 [P] In `src/ui.rs`, exhaustive-match arms for `Ty::Param(_)`:
  - `render_ty(Ty::Param(name))` → render `format!("<{}>", name)` (e.g. `"<T>"`) as a defensive fallback. The match needs to compile (exhaustive); the rendering is "visibly wrong" if ever shown.
  - `value_size_bytes_ui(Value)` is by-Value not by-Ty, so unaffected.
  - `ty_size_bytes_ui(Ty::Param(_))` → return 0 (defensive; shouldn't be called for a Ty::Param).
  - No new SlotRowView fields, no JS changes — the substituted type rendering flows through the existing `slot.ty` field (which is `render_ty(concrete_ty)` after typeck substitution).

**Checkpoint**: `cargo build` should compile cleanly. Match-exhaustiveness will flag any `Ty::Struct { name, fields }` patterns that need `type_args: _` (or `..`) added. Fix exhaustively. `cargo test` passes: M02/M03 byte-identical (no existing sample constructs generic types); M01 possibly re-baselines once for `type_params: Vec::new()` AST field (depends on snapshot serializer behavior — verify and either accept the re-baseline or refine the serde annotation).

---

## Phase 3: User Story 1 — Generic identity fn with monomorphization (Priority: P1) 🎯 MVP

**Goal**: `fn id<T>(x: T) -> T { x } fn main() { let a = id(5); let b = id(true); }` typechecks; trace contains two distinct `FrameEnter` events with `fn_name` `"id::<i32>"` and `"id::<bool>"`; param `x`'s `SlotWrite` carries the substituted concrete type per call; `a = 5_i32`, `b = true`.

**Independent Test**: load `m07_5_id_fn.rs`, step through both calls, observe two frames with distinct mangled names (`id::<i32>` and `id::<bool>`) AND distinct param types.

### Implementation

- [X] T012 [US1] Add 1 sample program pair: `tests/samples/m07_5_id_fn.rs` and `web/samples/m07_5_id_fn.rs`. Content:
  ```rust
  fn id<T>(x: T) -> T {
      x
  }

  fn main() {
      let a = id(5);
      let b = id(true);
  }
  ```

- [X] T013 [US1] In `src/pipeline.rs` `mod tests`, add unit tests:
  - `run_pipeline_generic_id_fn`: asserts the trace contains exactly TWO `FrameEnter` events with `fn_name == "id::<i32>"` and `fn_name == "id::<bool>"`; both frames have a `SlotAlloc { name: "x", ty: <correct concrete type>, .. }` followed by `SlotWrite` carrying the corresponding `Value::Int { I32, 5 }` / `Value::Bool(true)`. Asserts `a` and `b` SlotWrites land with the correct values.
  - `run_pipeline_generic_inference_mismatch`: source `fn pair<T>(a: T, b: T) -> T { a } fn main() { let _ = pair(5, true); }` — asserts pipeline returns `CompileError { stage: Typeck, .. }` with message containing "cannot infer" and naming both `i32` and `bool`.

- [X] T014 [US1] In `web/index.html`, add a dropdown `<option>` for `m07_5_id_fn.rs`.

**Checkpoint**: at this point US1 is fully functional. `cargo test` passes including the 2 new tests. The page can load `Generic id fn` and step through observing two distinct frames per call. **MVP deliverable**: monomorphization-visible pedagogy at the trace level.

---

## Phase 4: User Story 2 — Generic struct (Priority: P1)

**Goal**: `struct Wrapper<T> { v: T } let w = Wrapper { v: 5 }; let a = w.v;` typechecks; w's slot type label shows `Wrapper<i32>` (substituted), the struct view renders the `v: i32` field; `a = 5_i32`.

**Independent Test**: load `m07_5_generic_struct.rs`, step past `let w = Wrapper { v: 5 }`, observe `w`'s slot rendering as `Wrapper<i32>` with a single `v: i32` field row.

### Implementation

- [X] T015 [US2] Add 1 sample program pair: `tests/samples/m07_5_generic_struct.rs` and `web/samples/m07_5_generic_struct.rs`. Content:
  ```rust
  struct Wrapper<T> {
      v: T,
  }

  fn main() {
      let w = Wrapper { v: 5 };
      let a = w.v;
  }
  ```

- [X] T016 [US2] In `src/pipeline.rs` `mod tests`, add unit tests:
  - `run_pipeline_generic_struct`: asserts the trace contains a `SlotAlloc { name: "w", ty: Ty::Struct { name: "Wrapper", fields: [("v", Int(I32))], type_args: [Int(I32)] }, .. }` and a corresponding `SlotWrite` carrying `Value::Struct { name: "Wrapper", fields: [("v", Int{I32, 5})] }`. Asserts `a = 5_i32` via field access.
  - `run_pipeline_generic_struct_two_instantiations`: source with `let w1 = Wrapper { v: 5 }; let w2 = Wrapper { v: true };` — asserts BOTH SlotAllocs have distinct substituted types (`Wrapper<i32>` for w1, `Wrapper<bool>` for w2) per the nominal-with-instantiation equality rule.

- [X] T017 [US2] In `web/index.html`, add a dropdown `<option>` for `m07_5_generic_struct.rs`.

**Checkpoint**: US2 fully functional. `cargo test` passes including the 2 new tests. Page renders `Wrapper<i32>` in the slot's type label automatically via the extended `Ty::name()`.

---

## Phase 5: User Story 3 — Turbofish call (Priority: P2)

**Goal**: `let v = id::<bool>(false);` typechecks with `v : bool`; the frame is labeled `id::<bool>` (just like the inferred case). Turbofish on struct literal `Wrapper::<i32> { v: 5 }` also works.

**Independent Test**: load `m07_5_turbofish.rs`, step past `let v = id::<bool>(false);`, observe `v : bool = false` and the `id::<bool>` frame.

### Implementation

- [X] T018 [US3] Add 1 sample program pair: `tests/samples/m07_5_turbofish.rs` and `web/samples/m07_5_turbofish.rs`. Content:
  ```rust
  fn id<T>(x: T) -> T {
      x
  }

  fn main() {
      let v = id::<bool>(false);
  }
  ```

- [X] T019 [US3] In `src/pipeline.rs` `mod tests`, add unit tests:
  - `run_pipeline_turbofish`: asserts the trace contains a `FrameEnter { fn_name: "id::<bool>", .. }` and `v`'s SlotWrite carries `Value::Bool(false)`.
  - `run_pipeline_turbofish_type_mismatch`: source `let v = id::<bool>(5);` — asserts pipeline returns `CompileError { stage: Typeck, .. }` with message containing "expected `bool`" and "found `i32`".

- [X] T020 [US3] In `web/index.html`, add a dropdown `<option>` for `m07_5_turbofish.rs`.

**Checkpoint**: US3 fully functional. `cargo test` passes including the 2 new tests.

---

## Phase 6: Out-of-scope rejection tests (cross-cutting; NOT a user story)

**Purpose**: lock in the M07.5-specific rejection messages for out-of-scope shapes so they don't accidentally regress.

- [X] T021 In `src/pipeline.rs` `mod tests`, add unit tests for the scope-rejection paths:
  - `run_pipeline_generic_multi_param_rejected`: source `fn pair<T, U>(a: T, b: U) -> T { a } fn main() { let _ = pair(5, true); }` — asserts CompileError with message citing the single-param restriction.
  - `run_pipeline_generic_bound_rejected`: source `fn id<T: Foo>(x: T) -> T { x } fn main() { let _ = id(5); }` — asserts CompileError with message mentioning "M07.6" or "trait bound".
  - `run_pipeline_generic_nested_call_rejected`: source `fn outer<T>(x: T) -> T { id::<T>(x) } fn id<T>(x: T) -> T { x } fn main() { let _ = outer(5); }` — asserts CompileError with message mentioning "nested generic calls" or "out of scope in M07.5".

**Checkpoint**: 3 new tests pass; combined with US1/US2/US3 tests, ≥ 8 new M07.5 tests total per SC.

---

## Phase 7: Polish & Cross-Cutting

**Purpose**: snapshot re-baselines, bundle-size check, sample integration verification, warnings check, doc updates.

- [X] T022 [P] Run `cargo insta test`. Verify M02 + M03 snapshots are byte-identical. Verify M01's `parses_full_l1.snap` — if it re-baselines for `type_params: []` AST field, accept the re-baseline and note in commit message; if it stays byte-identical, even better.
- [X] T023 [P] Build WASM release and measure bundle size: `cd web && trunk build --release && ls -la dist/.stage/*.wasm` (or `target/wasm32-unknown-unknown/release/rustviz.wasm` if wasm-opt fails — note: trunk's wasm-opt has a pre-existing memory-copy compatibility issue; the staged pre-wasm-opt size is the meaningful number). Compare to M07.4 baseline (310,880 B). Acceptable if ≤ +20% (~373 KB). If over: candidate cuts per plan.md.
- [X] T024 [P] Run `RUSTFLAGS="-D warnings" cargo build --release` AND `cargo test`. Both should be clean. Fix any new warnings introduced by M07.5 changes (pre-existing warnings out of scope).
- [X] T025 [P] Run `cargo clippy --all-targets -- -D warnings`. Fix any NEW lints introduced by M07.5 changes; pre-existing lints (already flagged in M07.4 polish phase) out of scope.
- [X] T026 Manual M07.5 QA per `specs/016-m07-5-generics/quickstart.md` procedure. ~6 minutes. Step through US1–US3 in the page; verify error UX via live editing for the four rejection paths (inference mismatch, turbofish mismatch, multi-T, bound). Cycle through M01–M07.4 samples to confirm no regressions.
- [X] T027 Verify `CLAUDE.md` "Recent Changes" footer includes M07.5 (the speckit update-agent-context script handles this; verify post-hoc).
- [X] T028 Final commit messages + tag. Note in the eventual merge MR: "9th invocation of the closed-enum-with-revisions rule. Additive `Ty::Param(String)` + serde-default-empty `Ty::Struct.type_args` extension. Substitution via per-call-site `TypeMap.call_substs` side table. Mangled fn names (`id::<i32>`) drive monomorphization-visible frames — no UI changes needed."

---

## Dependencies

```text
Phase 1 (Setup)
  └─ T001 (verify baseline)

Phase 2 (Foundational) — blocks ALL user-story phases
  ├─ T002 (contract amendment, can run anytime)
  ├─ T003 [P] (AST extensions)
  ├─ T004 (parser — depends on T003 AST)
  ├─ T005 [P] (resolve — depends on T003 AST)
  ├─ T006 (Ty::Param + Ty::Struct.type_args — depends on T003)
  ├─ T007 (typeck substitution machinery — depends on T006)
  ├─ T008 (call-site plumbing — depends on T007)
  ├─ T009 (struct-lit plumbing — depends on T007)
  ├─ T010 (eval mangled names — depends on T008)
  └─ T011 [P] (ui exhaustive arms — depends on T006)

Phase 3 (US1: generic id fn) — depends on Phase 2
  ├─ T012 (sample)
  ├─ T013 (2 unit tests)
  └─ T014 (dropdown)

Phase 4 (US2: generic struct) — depends on Phase 2 (independent of US1)
  ├─ T015 (sample)
  ├─ T016 (2 unit tests)
  └─ T017 (dropdown)

Phase 5 (US3: turbofish) — depends on Phase 2 (independent of US1/US2)
  ├─ T018 (sample)
  ├─ T019 (2 unit tests)
  └─ T020 (dropdown)

Phase 6 (rejection tests) — depends on Phase 2
  └─ T021 (3 rejection tests)

Phase 7 (Polish) — depends on Phases 3-6
  └─ T022–T028 (snapshot/bundle/warnings/QA/docs/commit)
```

---

## Parallel execution opportunities

- **Phase 2**: T003 + T005 + T011 are file-disjoint [P]. T004 depends on T003; T006 depends on T003; T007 depends on T006; T008/T009 depend on T007; T010 depends on T008.
- **Phases 3/4/5/6**: completely independent of each other. After Phase 2 lands, all four can be done in parallel by different agents/sessions (each touches its own sample files + a sliver of pipeline.rs tests + a one-line index.html addition).
- **Phase 7**: T022, T023, T024, T025 all parallelizable [P].

---

## Implementation strategy

**MVP scope** = **US1 only** (generic id fn with monomorphization). Lands the headline pedagogy — distinct frames per substitution — in a single sample. ~600 LOC.

**Incremental delivery**:
1. **MVP (US1)**: Phases 1+2+3 (Setup + Foundational + US1). Headline pedagogy live.
2. **+US2 (generic struct)**: Phase 4. Extends struct surface to "container holding any T".
3. **+US3 (turbofish)**: Phase 5. Explicit-annotation ergonomics.
4. **+Rejection tests**: Phase 6. Locks in error messages.
5. **+Polish**: Phase 7. Snapshot/bundle/QA/docs.

**Recommended landing order**: ship all 3 user stories + rejection tests in one merge. The AST/typeck/eval surface is cross-cutting (substitution touches all three layers); splitting at user-story granularity would force three rounds of "land Phase 2 scaffolding, then one US, then Phase 2 again". The single-merge approach matches the M07.4 pattern.

**No UX checkpoint needed**: unlike M07.4 (which had the struct-viz Proposal A iteration), M07.5's UI surface is purely a `Ty::name()` extension that flows automatically through the existing renderers. No new visual decisions.

**Sequence note**: M07.5 closed cleanly enables M07.6 (traits). The substitution machinery from this milestone is the foundation; M07.6 adds trait-bound checking + trait-dispatch fall-through on top.
