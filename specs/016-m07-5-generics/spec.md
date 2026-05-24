# Feature Specification: M07.5 — Generics (`fn foo<T>(...)`, `struct Wrapper<T>`, monomorphization-visible frames)

**Feature Branch**: `016-m07-5-generics`
**Created**: 2026-05-24
**Status**: Draft
**Input**: User description: "M07.5 — generics: type parameters on fns + structs"

**Authoritative scope source**: [`MILESTONES.md` › M07.5 — Generics](../../MILESTONES.md). The In scope / Out of scope / Entry criteria / Exit criteria / Demo lines in that block are the contract for this feature. This spec elaborates user stories, requirements, and success criteria; it does not redefine scope.

## User Scenarios & Testing *(mandatory)*

M07.5 introduces **type parameters** on user-defined functions and structs. The first milestone where a learner can write `fn id<T>(x: T) -> T { x }` or `struct Wrapper<T> { v: T }` and call it with any type. The pedagogical headline is **monomorphization visibility**: each concrete substitution produces a distinct frame name in the trace — `id::<i32>(5)` opens a frame labeled `id::<i32>`, `id::<bool>(true)` opens a frame labeled `id::<bool>`. The learner sees "the same source fn produces two distinct frames at runtime" and the zero-cost-via-duplication cost model becomes concrete.

M07.5 is the foundation that M07.6 (traits) builds on. Without M07.5, `fn print<T: Show>(x: T)` (the headline trait-bound payoff) is unreachable. M07.5 ships generics WITHOUT bounds — the surface is "T can be anything"; constraints land in M07.6.

### User Story 1 - Generic identity fn with monomorphization (Priority: P1)

A learner types `fn id<T>(x: T) -> T { x } fn main() { let a = id(5); let b = id(true); }`. The trace shows two distinct frames: one labeled `id::<i32>` opening when `id(5)` is called (param `x : i32 = 5_i32`), and a second labeled `id::<bool>` when `id(true)` is called (param `x : bool = true`). Both frames return their argument unchanged. `a = 5_i32`, `b = true`.

**Why this priority**: this IS the foundational pedagogy. Generics' value proposition — "one source fn, many concrete types, zero runtime cost via duplication" — is invisible without distinct frame names per substitution. P1.

**Independent Test**: load `m07_5_id_fn.rs`, step through both calls, observe two frames with distinct mangled names (`id::<i32>` and `id::<bool>`) AND distinct param types.

**Acceptance Scenarios**:

1. **Given** `fn id<T>(x: T) -> T { x } let a = id(5);`, **When** the pipeline runs, **Then** typeck succeeds with `a : i32 = 5_i32`; the trace contains a `FrameEnter { fn_name: "id::<i32>", .. }`; the `x` param's SlotWrite carries `Value::Int { I32, 5 }`.
2. **Given** the same fn called as `id(true)`, **When** the pipeline runs, **Then** the trace contains a separate `FrameEnter { fn_name: "id::<bool>", .. }` with `x : bool = true`.
3. **Given** both calls in the same program, **When** the pipeline runs, **Then** the two frames are independently visible in the trace at distinct cursor steps — no shared frame, no shared params.
4. **Given** an inferred call `id(5)`, **When** typeck runs, **Then** `T` is inferred from the arg's type (`i32`) without requiring explicit annotation. Inference fails (clear error) if multiple incompatible call sites prevent a single `T` assignment within one frame.

---

### User Story 2 - Generic struct (Priority: P1)

A learner types `struct Wrapper<T> { v: T } fn main() { let w = Wrapper { v: 5 }; let a = w.v; }`. The stacks panel shows `w : Wrapper<i32>` (the substituted type, not the source `Wrapper<T>`), with the struct view rendering exactly like a non-generic struct's `v: i32` field. `a = 5_i32`.

**Why this priority**: extends M07.4's struct surface to "container holding any T". Without generic structs, learners can't build their own `Wrapper`, `Box`-like, `Option`-like types. P1.

**Independent Test**: load `m07_5_generic_struct.rs`, step past `let w = Wrapper { v: 5 }`, observe `w`'s slot rendering as `Wrapper<i32>` with a single `v: i32` field row.

**Acceptance Scenarios**:

1. **Given** `struct Wrapper<T> { v: T } let w = Wrapper { v: 5 };`, **When** typeck runs, **Then** typeck succeeds with `w : Wrapper<i32>`; the SlotWrite for `w` carries `Value::Struct { name: "Wrapper", fields: [("v", Int{I32, 5})] }`; the rendered type label is `"Wrapper<i32>"` (with the substitution applied).
2. **Given** `let w2 = Wrapper { v: true };` in a separate statement, **When** typeck runs, **Then** `w2 : Wrapper<bool>` — a separate, structurally-identical struct with the bool substitution.
3. **Given** `let a = w.v;`, **When** the pipeline runs, **Then** `a : i32 = 5_i32`; field access works exactly like the non-generic case.
4. **Given** mismatched-element field with explicit annotation (`let w: Wrapper<i32> = Wrapper { v: true };`), **When** typeck runs, **Then** error "expected `i32`, found `bool`".

---

### User Story 3 - Turbofish call (Priority: P2)

A learner types `let v = id::<bool>(false);` — the **turbofish** `::<bool>` explicitly pins `T` to `bool`. Useful when inference is ambiguous or when the learner wants to be explicit. The trace shows `id::<bool>` frame just like the inferred case.

**Why this priority**: explicit type annotation is standard Rust ergonomics; useful but optional since inference handles the common case. P2.

**Independent Test**: load `m07_5_turbofish.rs`, step past `let v = id::<bool>(false);`, observe `v : bool = false` and the `id::<bool>` frame.

**Acceptance Scenarios**:

1. **Given** `let v = id::<bool>(false);`, **When** the pipeline runs, **Then** typeck succeeds with `v : bool`; the frame is labeled `id::<bool>`.
2. **Given** mismatched turbofish `let v = id::<bool>(5);` (explicit bool, but arg is i32), **When** typeck runs, **Then** error "argument 1: expected `bool`, found `i32`".
3. **Given** turbofish on a generic struct literal `let w = Wrapper::<i32> { v: 5 };`, **When** the pipeline runs, **Then** typeck succeeds; `w : Wrapper<i32>`.
4. **Given** turbofish matching the inferred type (`id::<bool>(true)` — the arg's bool agrees with the explicit `<bool>`), **When** the pipeline runs, **Then** typeck succeeds (no conflict).

---

### Edge Cases

- **Unused type param** `fn foo<T>(x: i32) -> i32 { x }` — typechecks (T is declared but unused; standard Rust allows it). No warning emitted.
- **Same fn called twice with same type** `id(5); id(5);` — produces TWO frames both labeled `id::<i32>`, distinct frame_ids. Each call entry is a fresh frame (cost model stays consistent with non-generic fns).
- **Generic call inside a generic fn** `fn outer<T>(x: T) -> T { id::<T>(x) }` — out of scope for M07.5. Single-level generics only; calling a generic fn from inside another generic fn (substituting `T` through) requires substitution-during-substitution machinery. Reject with a clear "M07.5 supports generic calls only from concrete-type contexts" error.
- **Mismatched arg types inside one inferred call** `fn pair<T>(a: T, b: T) -> T { a }` called as `pair(5, true)` — typeck error: "cannot infer T from conflicting args: i32 vs bool". (Note: `pair<T>` is still single-T-param, so this is in scope; "cannot infer" is the expected error path when args conflict.)
- **Generic struct used as field type** `struct Outer { w: Wrapper<i32> }` — out of scope. Field-of-generic-struct deferred; struct fields stay restricted to primitives in M07.5 (matches M07.4).
- **Recursive generic struct** `struct Node<T> { v: T, next: Box<Node<T>> }` — out of scope. No recursive structs in M07.5.
- **Generic methods inside an `impl` block** `impl Wrapper<T> { fn get<U>(&self) -> U { ... } }` — out of scope. Methods on generic structs work (US2 makes that possible) but adding ANOTHER generic param to a method is too much for M07.5; defer.
- **Impl on a specific instantiation** `impl Wrapper<i32> { fn double(&self) -> i32 { self.v * 2 } }` — out of scope. Only `impl<T> Wrapper<T>` syntax considered; specific-instantiation impls deferred.
- **Type parameter shadowing** `fn id<T>(x: T) -> T { fn inner<T>(y: T) -> T { y } inner(x) }` — out of scope. Nested fns aren't part of L1; if they were, the inner T would shadow.
- **Underscore type arg** `id::<_>(5)` — out of scope (no inference placeholders in turbofish).
- **`const N: usize` const generics** `struct Buf<const N: usize> { ... }` — out of scope per the no-const-generics restriction.
- **Higher-ranked types** `fn foo(f: fn<T>(T)) { ... }` — out of scope; not in any prior milestone.
- **Lifetime params** `fn first<'a>(x: &'a i32) -> &'a i32` — out of scope per the explicit-lifetimes-as-generics restriction (scope-level lifetime handling stays as-is).
- **Default type params** `fn id<T = i32>(x: T) -> T { x }` — out of scope.
- **Where clauses** `fn id<T>(x: T) -> T where T: Sized { x }` — out of scope.
- **Empty type-param list** `fn foo<>() {}` — parser rejects with "expected type parameter, found `>`".

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST parse `fn name<T>(...) { ... }` and `struct Name<T> { ... }` as type-parameter-bearing AST nodes. `FnDecl` and `StructDecl` gain a `type_params: Vec<TypeParam>` field.
- **FR-002**: `TypeParam` is `{ name: String, span: Span }` — single-letter convention (T, U, V) but the parser accepts any identifier.
- **FR-003**: System MUST parse turbofish call syntax `id::<bool>(false)` and `Wrapper::<i32> { v: 5 }` as path-with-type-args expressions.
- **FR-004**: System MUST parse generic-type-parameter use sites inside fn/struct bodies (`fn id<T>(x: T) -> T` — `T` appears as a `Type::Path { segments: ["T"], .. }`).
- **FR-005**: System MUST extend the type lattice with a `Param(String)` variant representing an unresolved type parameter during typeck.
- **FR-006**: System MUST treat type-parameter names in scope (collected from the enclosing fn's `type_params`) as valid types in the body. `T` outside of any generic context resolves as "unknown type".
- **FR-007**: System MUST typecheck generic-fn calls via simple substitution: at the call site, infer `T` from the arg types (single-arg case: `T = arg_ty`; multi-arg single-T case: all args must agree). Mismatch → typeck error naming both types.
- **FR-008**: System MUST typecheck turbofish-call sites by binding `T` to the explicit type arg, then checking each arg against the substituted param type. Mismatch → typeck error.
- **FR-009**: System MUST typecheck generic-struct literals via substitution from field types: `Wrapper { v: 5 }` infers `T = i32`. Explicit annotation mismatch (`let w: Wrapper<i32> = Wrapper { v: true };`) → typeck error.
- **FR-010**: System MUST render substituted types in slot labels: `Wrapper<i32>` (with the substitution applied), NOT the source `Wrapper<T>`.
- **FR-011**: System MUST render substituted fn names in `FrameEnter.fn_name`: `id::<i32>`, not `id`. The mangled name encodes the substitution so the trace shows distinct frames per call site's instantiation.
- **FR-012**: System MUST extend the eval-side registries to dispatch generic calls. The registry lookup key is the source name (`id`, `Wrapper`, `new`); substitution happens at the call site.
- **FR-013**: System MUST emit one FrameEnter per call (no event-level memoization of identical-substitution frames); the cost model "each concrete substitution opens a fresh frame" is the headline pedagogy.
- **FR-014**: System MUST reject "generic call inside generic fn" (`fn outer<T>(x: T) -> T { id::<T>(x) }`) at typeck — substitution-during-substitution out of scope. Clear error: "generic-fn calls inside another generic fn's body are out of scope in M07.5".
- **FR-015**: System MUST reject multiple type params per fn/struct (`fn pair<T, U>(...)`) at typeck. Clear error: "M07.5 supports a single type parameter; multiple type parameters are out of scope".
- **FR-016**: System MUST reject type-param bounds (`fn id<T: Foo>(...)`) at typeck. Clear error: "trait bounds on generics are deferred to M07.6".
- **FR-017**: System MUST reject const generics (`<const N: usize>`) and lifetime generics (`<'a>`) at parse-time. Clear errors.
- **FR-018**: System MUST ship at least 3 new reference programs (`tests/samples/m07_5_*.rs` + `web/samples/`) covering: identity fn with two distinct frames, generic wrapper struct, turbofish call.
- **FR-019**: System MUST preserve all M01–M07.4 existing tests byte-identical (the new `type_params` field is empty for non-generic fns/structs; with serde-default-empty this keeps existing snapshots unchanged).

### Key Entities

- **Type parameter** (`TypeParam`): named placeholder on a fn or struct decl. `{ name: String, span: Span }`. Order significant (positional substitution at call sites).
- **Type parameter type** (`Ty::Param(String)`): the type-system representation. Carries the param's name (e.g. `"T"`) so error messages can reference it. Substituted at call sites with concrete types.
- **Substitution map** (typeck-side, scoped per call/frame): `{ "T" → Ty::Int(I32), .. }`. Built at the call site from inference or turbofish; used to lower the fn's params + return type to concrete `Ty`s.
- **Mangled fn name** (eval-side): `id::<i32>` — the source name + `::<>` + substitution. Drives `FrameEnter.fn_name` so the trace shows distinct frames per substitution.
- **Generic struct schema**: `Wrapper<T>`'s schema in `StructRegistry` carries `type_params: Vec<String>` so type lookups can substitute. The `Ty::Struct { name, fields }` for an instantiation is the schema with `T` substituted.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After M07.5 ships, `fn id<T>(x: T) -> T { x } let a = id(5); let b = id(true);` typechecks; the trace contains two distinct `FrameEnter` events with `fn_name` `"id::<i32>"` and `"id::<bool>"`.
- **SC-002**: `struct Wrapper<T> { v: T } let w = Wrapper { v: 5 };` typechecks; the SlotWrite for `w` carries `Value::Struct { name: "Wrapper", fields: [("v", Int{I32,5})] }`; the slot's rendered type label is `"Wrapper<i32>"`.
- **SC-003**: Turbofish `let v = id::<bool>(false);` typechecks and frames as `id::<bool>`.
- **SC-004**: Inferred call with type mismatch (`pair(5, true)` on `fn pair<T>(a: T, b: T) -> T`) → typeck error naming both inferred types.
- **SC-005**: Turbofish with arg-type mismatch (`id::<bool>(5)`) → typeck error naming the expected and found types.
- **SC-006**: Generic-fn-call inside another generic fn → typeck error citing the M07.5 out-of-scope rule.
- **SC-007**: Multi-type-param decl (`fn pair<T, U>(...)`) → typeck error citing the single-param restriction.
- **SC-008**: Type-bound on generic (`fn id<T: Foo>(...)`) → typeck error pointing at M07.6.
- **SC-009**: ≥ 3 new `m07_5_*.rs` reference programs ship.
- **SC-010**: Existing M01–M07.4 tests pass byte-identical (additive `type_params: Vec<TypeParam>` field with serde-default-empty on AST nodes shouldn't affect existing snapshots).
- **SC-011**: WASM bundle growth ≤ +20% vs M07.4 baseline (smaller than M07.4's +25% — substitution machinery + name mangling, no new MemEvent variants, no new UI surface for the headline scenarios).
- **SC-012**: Zero warnings under `RUSTFLAGS="-D warnings" cargo build --release` AND `cargo test`. Both host and WASM targets clean.

## Assumptions

- **Single type param per fn/struct**: only `<T>` shapes; `<T, U>` deferred. Matches MILESTONES.md restriction.
- **Type parameter names accept any identifier**: parser doesn't enforce single-letter; convention is T/U/V but `fn id<MyType>(x: MyType)` works. Stay permissive at parse, strict at typeck only on the no-bounds / no-multiple-params rules.
- **No bounds at this milestone**: trait bounds (`T: Foo`) are M07.6's payoff. M07.5's `T` accepts any concrete type at the call site; the fn body can only do operations that work for any `T` (move, return, store in a struct) — NOT field access on `T`, NOT method calls on `T` (those would require bounds).
- **Simple direct-match inference**: at a call site, infer `T = arg_ty` from the first generic-typed param's type; check all subsequent generic-typed params agree. No full HM unification, no constraint solving, no most-general-type computation. If inference fails (no args, or args conflict), typeck error with a clear "add an explicit turbofish annotation" hint.
- **Monomorphization at frame-entry granularity**: each call site that requires a fresh substitution produces a fresh FrameEnter with the mangled `fn_name`. No event-level memoization of identical substitutions (two `id(5)` calls produce two distinct frames with the same `id::<i32>` name); the cost model "each call entry is a fresh frame" stays consistent with non-generic fns.
- **Generic struct field types** restricted to primitives + the struct's own type params. `Wrapper<T> { v: T }` works. `Wrapper<T> { v: Vec<T> }` and `Wrapper<T> { v: OtherStruct<T> }` deferred — fields stay primitive-flavored per M07.4.
- **No generic methods**: `impl Wrapper<T>` blocks supported (the impl substitutes T from the receiver); but method-level type params (`impl Wrapper<T> { fn get<U>(&self) -> U }`) deferred.
- **Inferred call without type info**: if a fn call has zero generic-typed args (e.g. a user-defined `fn new<T>() -> Wrapper<T>` style) and no turbofish, typeck error pointing at the missing turbofish.
- **Frame-name format**: `id::<i32>` (Rust standard). For multi-param substitutions (if ever supported in a future milestone), would be `pair::<i32, bool>` — established here as the format but only exercised with single-param subs in M07.5.
- **Bundle target ≤ +20%**: smaller than M07.4's +25% since the headline scenarios (identity fn, wrapper struct, turbofish) don't add new UI rendering surface beyond the type-label substitution; the substitution machinery is mostly typeck-side.
- **Sized XL** per the rubric: ~5 source modules touched (parse/{ast, parser}, typeck, eval, ui-for-type-label-substitution) + 3 sample pairs + ≥ 8 unit tests. Estimated ~800-1100 LOC net change. Comparable to M07.4 but smaller — protocol footprint is just the additive Ty variant.
- **Three plan-phase deferrals** (no NEEDS CLARIFICATION because reasonable defaults exist):
  1. **`Ty::Param` vs `Ty::Var`** — recommendation: `Ty::Param(String)` for simplicity; substitution map is `HashMap<String, Ty>`.
  2. **Mangled name format** — recommendation: `id::<i32>` (Rust standard); use the substituted type's `name()` for the rendering.
  3. **Substitution-failure error message** — recommendation: "cannot infer type parameter `T` from arguments; add an explicit type annotation like `id::<i32>(...)`" with the source span pointing at the call site.
- **Foundation for M07.6**: M07.5 ships generics as the foundation; M07.6 adds trait bounds (`T: Foo`) so the body can call trait methods on `T`. Without M07.6, M07.5's `T` is "any concrete type, but you can only do generic-safe ops on it" (move, return, store-in-a-struct).
