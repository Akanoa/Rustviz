# Feature Specification: M07.6 — Traits (declarations, impls, static dispatch via bounds)

**Feature Branch**: `017-m07-6-traits`
**Created**: 2026-05-24
**Status**: Draft
**Input**: User description: "M07.6 — traits: declarations, impls, static dispatch via bounds"

**Authoritative scope source**: [`MILESTONES.md` › M07.6 — Traits](../../MILESTONES.md). The In scope / Out of scope / Entry criteria / Exit criteria / Demo lines in that block are the contract for this feature. This spec elaborates user stories, requirements, and success criteria; it does not redefine scope.

## User Scenarios & Testing *(mandatory)*

M07.6 introduces **traits** — Rust's polymorphism mechanism — plus **trait bounds on generics**. The headline scenario is `fn print<T: Show>(x: T) { x.show(); }` — a generic fn whose body calls a method on `T` because the bound `T: Show` proves the method exists. This completes the polymorphism story M07.5 (generics) began: M07.5 ships generics-without-constraints (the body can only do generic-safe ops); M07.6 ships the constraints (the body can call methods proven by the bound). Method dispatch extends M07.4's two-layer fall-through (builtins → inherent impls) with a third layer: **trait impls**.

Static dispatch only — `&dyn Trait` and vtables deferred. Associated types, supertraits, and derive macros also deferred (out of scope for the first cut). The headline pedagogy is "the bound is the proof" — visible in the trace as: typeck rejects `print(5)` if `i32` doesn't impl `Show`, but accepts `print(p)` where `Point: Show`.

### User Story 1 - Trait declaration + impl + method dispatch (Priority: P1)

A learner writes `trait Show { fn show(&self) -> i32; }`, then `impl Show for Point { fn show(&self) -> i32 { self.x } }`, then calls `let s = p.show();`. Method dispatch resolves to the trait impl. New frame opens for `Point::show` (or `<Point as Show>::show` — exact mangling plan-phase's call); self bound to `&p`; body returns `1_i32`; `s = 1_i32`.

**Why this priority**: this IS the foundational pedagogy. Without trait declarations + impl-for + method dispatch, no other story is possible. P1.

**Independent Test**: load `m07_6_trait_basic.rs`, step through `let s = p.show()`, observe the trait-method frame opens, dispatches to the impl-block's body, returns `1_i32`.

**Acceptance Scenarios**:

1. **Given** `trait Show { fn show(&self) -> i32; } impl Show for Point { fn show(&self) -> i32 { self.x } } let s = p.show();`, **When** the pipeline runs, **Then** typeck succeeds with `s : i32`; the trace contains a `FrameEnter` for the trait-method dispatch; the impl block's body executes; `s` lands `1_i32`.
2. **Given** the impl-for is missing (only the trait declaration, no `impl Show for Point`), **When** `p.show()` is called, **Then** typeck error "no method `show` on type `Point`" (Point doesn't implement Show).
3. **Given** Point has both an inherent method `show` AND a trait `Show { fn show }` with `impl Show for Point`, **When** `p.show()` is called, **Then** the INHERENT impl wins per M07.4's dispatch tie-breaker (consistent with the existing builtin-wins-over-trait pattern).
4. **Given** a trait declares a required method without a body and the impl provides one, **When** the call dispatches, **Then** the impl's body runs.

---

### User Story 2 - Default methods (Priority: P1)

A learner writes `trait Counter { fn count(&self) -> i32; fn double(&self) -> i32 { self.count() * 2 } }`, then `impl Counter for Point { fn count(&self) -> i32 { self.x } }`. When `p.double()` is called, the **default body** in the trait declaration runs (the impl didn't override it); that body calls `self.count()` which dispatches back to the impl's override. Result: `p.x * 2`.

**Why this priority**: default methods are how traits add behavior without forcing every impl to re-implement common cases. Without defaults, traits feel mechanical (every method must be re-stated in every impl); with defaults, learners see "the trait provides functionality, not just a list of names". P1.

**Independent Test**: load `m07_6_default_method.rs`, step through `let v = p.double()`, observe two frames: outer `Counter::double` (the default body), then inner `Counter::count` (the impl override). Result `v = 2 * p.x`.

**Acceptance Scenarios**:

1. **Given** `trait Counter { fn count(&self) -> i32; fn double(&self) -> i32 { self.count() * 2 } } impl Counter for Point { fn count(&self) -> i32 { self.x } } let v = p.double();`, **When** the pipeline runs, **Then** typeck succeeds; the trace contains a frame for `double` (default body) AND a nested frame for `count` (impl override); `v` lands `2 * p.x`.
2. **Given** an impl overrides a default method, **When** the call dispatches, **Then** the override wins (the trait's default is unused).
3. **Given** a trait has a required method (no body) and the impl doesn't provide one, **When** typeck runs, **Then** error "missing implementation of trait method `<name>` for type `<Ty>`".

---

### User Story 3 - Generic bound `T: Trait` (Priority: P1)

A learner writes `fn print<T: Show>(x: T) -> i32 { x.show() }`. Inside the body, `x.show()` works because the bound `T: Show` proves `T` implements `Show`. Call sites: `print(p)` works (since `Point: Show`); `print(5)` fails typeck ("the trait bound `i32: Show` is not satisfied").

**Why this priority**: THIS is the polymorphism payoff. M07.5 shipped `fn id<T>(x: T) -> T` where the body can't call ANY method on T. M07.6 unlocks `fn print<T: Show>(x: T)` where the body can call `x.show()`. Without the bound, the body is purely generic-safe-ops; with it, the body uses the trait's methods. P1 — the headline scenario for the whole milestone.

**Independent Test**: load `m07_6_generic_bound.rs`, step through `print(p)`, observe the generic fn enters with `x : Point` (substituted), then dispatches `x.show()` to Point's impl block — same flow as a direct `p.show()` call.

**Acceptance Scenarios**:

1. **Given** `trait Show { fn show(&self) -> i32; } impl Show for Point { fn show(&self) -> i32 { self.x } } fn print<T: Show>(x: T) -> i32 { x.show() } let r = print(p);`, **When** the pipeline runs, **Then** typeck succeeds with `r : i32`; the trace contains a `FrameEnter` for `print::<Point>` (monomorphized) and a nested frame for `Show::show` dispatched on `Point`; `r` lands `p.x`.
2. **Given** `let r = print(5);` where `i32: Show` is NOT implemented, **When** typeck runs, **Then** error "the trait bound `i32: Show` is not satisfied".
3. **Given** the same generic `print` called with two distinct types that both impl Show (`print(p)` AND `print(q)` where Q is another struct with Show), **When** the pipeline runs, **Then** TWO distinct frames `print::<Point>` and `print::<Q>` open per the M07.5 monomorphization rule.
4. **Given** the body of `print<T: Show>` tries `x.unknown_method()` (a method NOT on Show), **When** typeck runs, **Then** error "method `unknown_method` is not on trait `Show` (the only trait `T` is bounded by)".

---

### User Story 4 - Multiple bounds `T: Trait1 + Trait2` (Priority: P2)

A learner writes `fn show_n_count<T: Show + Counter>(x: T) -> i32 { x.show() + x.count() }`. Both bounds active inside the body; both trait methods callable.

**Why this priority**: multi-bound is the natural extension of single-bound; once single-bound works, multi-bound is mostly syntax. Useful but less foundational than US1-US3. P2.

**Independent Test**: load `m07_6_multi_bound.rs`, step through `show_n_count(p)`, observe both method dispatches succeed.

**Acceptance Scenarios**:

1. **Given** `fn show_n_count<T: Show + Counter>(x: T) -> i32 { x.show() + x.count() } let r = show_n_count(p);` where Point impls both Show and Counter, **When** the pipeline runs, **Then** typeck succeeds; the trace contains nested frames for both `Show::show` and `Counter::count`.
2. **Given** Point impls Show but NOT Counter, **When** `show_n_count(p)` is called, **Then** typeck error "the trait bound `Point: Counter` is not satisfied".
3. **Given** the body calls a method that's on Show but not Counter (or vice versa), **When** typeck runs, **Then** the call succeeds (one bound is enough to prove the method exists).
4. **Given** the same method name appears on BOTH bound traits (`Show::name` and `Counter::name`), **When** `x.name()` is called, **Then** typeck error "ambiguous method `name` — candidates: `Show::name`, `Counter::name`; use UFCS (`Show::name(&x)`) to disambiguate" (UFCS itself is OUT of scope; the error suggests it as a hint).

---

### Edge Cases

- **Empty trait `trait Marker {}`** — typeck-accepted; impl blocks for it are empty too. Useful as a tag-trait; no methods to dispatch. Bound `T: Marker` doesn't add any callable methods.
- **Trait with one default method and one required** `trait Foo { fn a(&self) -> i32; fn b(&self) -> i32 { self.a() + 1 } }` — impl provides `a` only; `b` calls through.
- **Method name collision: inherent wins over trait** — if Point has inherent `impl Point { fn show(&self) -> i32 { 42 } }` AND `impl Show for Point { fn show(&self) -> i32 { self.x } }`, then `p.show()` dispatches to the inherent (`42`), not the trait impl. M07.4's tie-breaker pattern extended.
- **Method name collision: two traits both define `show`** — `trait A { fn show(&self) -> i32; } trait B { fn show(&self) -> i32; } impl A for Point {...} impl B for Point {...}`, then `p.show()` is ambiguous. Typeck error per US4 acceptance #4.
- **Empty impl block** `impl Marker for Point {}` — accepts; no methods to verify (Marker is empty).
- **Impl-for on built-in types** `impl Show for i32 { ... }` — M07.6 specific decision. **Recommendation: ACCEPT** (matches Rust; learners writing `impl Show for i32` would be confused if it didn't work). The trait dispatch table uses `(trait_name, ty_name)` as the key; the type lookup just queries by the rendered type name.
- **Required method missing in impl** `trait Show { fn show(&self) -> i32; } impl Show for Point {}` — typeck error "missing implementation of trait method `show` for type `Point`".
- **Extra method in impl** `trait Show { fn show(&self) -> i32; } impl Show for Point { fn show(&self) -> i32 { self.x } fn other(&self) {} }` — typeck error "method `other` is not on trait `Show`".
- **Calling a trait method via UFCS** `Show::show(&p)` — out of scope. M07.6 only supports the method-call dot syntax `p.show()`.
- **`Self` type in trait declarations** `trait Show { fn clone_show(&self) -> Self; }` — out of scope. Methods return concrete types only.
- **Associated types** `trait Iter { type Item; fn next(&mut self) -> Self::Item; }` — out of scope.
- **Supertraits** `trait B: A { fn b_method(&self); }` (B requires A) — out of scope.
- **Blanket impls** `impl<T: Foo> Bar for T { ... }` — out of scope.
- **Trait objects `&dyn Trait`** — out of scope. Static dispatch only via generic bounds.
- **Derive macros** `#[derive(Debug, Clone)]` — out of scope (would need built-in implementations for stdlib traits).
- **Where clauses** `fn foo<T>(x: T) where T: Show { ... }` — out of scope. Bounds expressed only as `T: Trait` on the type-param list.
- **Trait method visibility** (`pub trait`, `pub fn`) — out of scope. Visibility modifiers not yet in the language subset.
- **Generic trait methods** `trait Foo { fn bar<U>(&self, u: U); }` — out of scope (combines M07.5 generic-method restriction with traits).
- **Trait methods with `&mut self` / `self` receivers** — IN scope (mirrors M07.4's self-receiver support). `&self` is the headline case; the others fall out from existing M07.4 dispatch.
- **Trait methods with associated functions** `trait Foo { fn new() -> Self; }` — out of scope (requires Self type).
- **Multi-segment trait paths** `mod::Show` — out of scope. Single-segment trait names only.
- **Auto-traits** (`Send`, `Sync`) — deferred to M08 when threads land.
- **Trait inheritance edge cases** — covered by the no-supertrait restriction (no inheritance).
- **Orphan rules** — irrelevant in M07.6 (single-file scope, no crate visibility).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST parse `trait Name { fn item1; fn item2; ... }` as a new top-level `Item::Trait` declaration. Items are fn signatures (required methods, no body) OR fn declarations (default methods, with body).
- **FR-002**: System MUST parse `impl TraitName for TypeName { fn item1 { ... }; ... }` as an extension of `Item::Impl` (M07.4 already added inherent impls; M07.6 adds an optional `trait_name: Option<String>` field).
- **FR-003**: System MUST parse trait-bound syntax `<T: Trait>` on fn / struct type-param lists (extending M07.5's parse-permissive bound capture; M07.5 stored bounds and rejected at typeck — M07.6 wires them into the type system).
- **FR-004**: System MUST parse `<T: Trait1 + Trait2>` for multi-bound. The `+` token may need lexer support if absent — verify and add if needed.
- **FR-005**: System MUST extend the typeck registries with a `TraitRegistry` (trait name → required + default methods) and a `TraitImplRegistry` ((trait_name, type_name) → method overrides).
- **FR-006**: System MUST collect trait declarations and trait impls in phase 1 (alongside `StructRegistry` and `ImplRegistry`), so phase-2 body typecheck has full visibility regardless of source order.
- **FR-007**: System MUST extend method dispatch with a **third layer**: M07's hardcoded built-ins → M07.4's user-defined inherent impls → **M07.6's trait impls**. The first-match-wins order is the M07.4 tie-breaker pattern.
- **FR-008**: System MUST check trait bounds at call sites: for `fn print<T: Show>(x: T)` called as `print(arg)`, verify that the substituted concrete type `T_concrete` has an impl of `Show`. Bound-not-satisfied → typeck error naming both types.
- **FR-009**: System MUST allow method calls on type-param-typed values inside a generic body when the bound proves the method exists. `fn print<T: Show>(x: T) { x.show(); }` typechecks because `T: Show` and `Show` declares `fn show`.
- **FR-010**: System MUST reject method calls on type-param-typed values that the bounds don't prove exist. `fn print<T: Show>(x: T) { x.unknown(); }` typeck-rejects citing the bound.
- **FR-011**: System MUST handle default methods: when an impl doesn't override a default-bodied method, dispatch resolves to the trait declaration's body.
- **FR-012**: System MUST reject impls that fail to provide a required method ("missing implementation of trait method `<name>` for type `<Ty>`").
- **FR-013**: System MUST reject impls that include methods not declared on the trait ("method `<name>` is not on trait `<Trait>`").
- **FR-014**: System MUST reject duplicate trait declarations (`trait Show {} trait Show {}`) and duplicate trait impls for the same `(trait, type)` pair.
- **FR-015**: System MUST handle method-name ambiguity between multiple bound traits with a clear error suggesting UFCS (even though UFCS itself is out of scope) so the learner has a path forward.
- **FR-016**: System MUST extend `FrameEnter.fn_name` rendering for trait-method calls — recommendation: `<Point as Show>::show` (Rust-standard) OR a simpler `Point::show` if the inherent-vs-trait distinction is too subtle for the headline. Plan-phase decides format.
- **FR-017**: System MUST ship at least 4 new reference programs (`tests/samples/m07_6_*.rs` + `web/samples/`) covering: trait decl + impl + dispatch, default method, generic bound, multi-bound.
- **FR-018**: System MUST preserve all M01–M07.5 existing tests byte-identical for tests that don't exercise traits. Snapshots that include typeck/eval output may re-baseline minimally if the new registries' Debug shape leaks; mitigation via serde-skip-if-empty on the registry types.

### Key Entities

- **Trait declaration** (`Item::Trait`): top-level AST item with `name`, `items: Vec<TraitItem>`, `span`. Items are either required (signature-only) or default (with body).
- **Trait item**: either a required method `fn name(&self) -> Ret;` (no body — fn signature stored as a `FnSig` shape) OR a default method `fn name(&self) -> Ret { ... }` (with body — full `FnDecl`).
- **Trait impl** (extension of `Item::Impl`): the existing `Item::Impl { ty_name, items }` from M07.4 gains an optional `trait_name: Option<String>` field. `Some("Show")` → trait impl; `None` → inherent impl (M07.4's existing behavior).
- **Trait bound** (extension of `TypeParam`): the M07.5 `TypeParam { name, bound: Option<String>, .. }` was parser-permissive and typeck-rejected. M07.6 promotes `bound` to `bounds: Vec<String>` to support multi-bound and wires them into the type system.
- **`TraitRegistry`**: phase-1-collected `IndexMap<String, TraitSchema>` where `TraitSchema { required_methods: IndexMap<String, FnSig>, default_methods: IndexMap<String, FnDecl> }`. Phase 2 consults this when verifying generic bounds + dispatching trait-method calls.
- **`TraitImplRegistry`**: phase-1-collected `IndexMap<(String, String), TraitImpl>` keyed by `(trait_name, type_name)`. `TraitImpl { overrides: IndexMap<String, FnDecl> }` — only methods that the impl actually provides (defaults fall through to the trait's body).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After M07.6 ships, `trait Show { fn show(&self) -> i32; } impl Show for Point { fn show(&self) -> i32 { self.x } } let s = p.show();` typechecks; the trace contains a `FrameEnter` for the trait-method dispatch; `s = 1_i32`.
- **SC-002**: Default methods work: `trait Counter { fn count(&self) -> i32; fn double(&self) -> i32 { self.count() * 2 } } impl Counter for Point { fn count(&self) -> i32 { self.x } } let v = p.double();` — the trace shows TWO nested frames (outer `double` from default; inner `count` from impl override); `v = 2 * p.x`.
- **SC-003**: Generic bound: `fn print<T: Show>(x: T) -> i32 { x.show() } let r = print(p);` typechecks; the trace shows mangled `print::<Point>` frame; `r = p.x`.
- **SC-004**: Bound-not-satisfied: `let r = print(5);` (where `i32: Show` is not implemented) → typeck error containing "trait bound" and naming both `i32` and `Show`.
- **SC-005**: Multi-bound: `fn show_n_count<T: Show + Counter>(x: T)` works when Point impls both; rejects when Point impls only one (with a clear message naming the missing bound).
- **SC-006**: Missing required method in impl → typeck error naming the unimplemented method.
- **SC-007**: Extra method in impl → typeck error naming the extra method.
- **SC-008**: Inherent-wins-over-trait: when both an inherent method and a trait-impl method exist with the same name, the inherent wins. Match the M07.4 tie-breaker.
- **SC-009**: Method-name collision between two bound traits: typeck error suggesting UFCS (even though UFCS is out of scope).
- **SC-010**: ≥ 4 new `m07_6_*.rs` reference programs ship.
- **SC-011**: Existing M01–M07.5 tests pass byte-identical (with possible minimal re-baseline for TypeMap-shape changes from new registries — verify and accept if needed).
- **SC-012**: WASM bundle growth ≤ +25% vs M07.5 baseline (~342,873 B → ≤ ~429 KB raw). Larger than M07.5's +20% because M07.6 adds trait registries, bound-checking machinery, third-layer dispatch, AND the `+`-token bound parsing.
- **SC-013**: Zero warnings under `RUSTFLAGS="-D warnings" cargo build --release` AND `cargo test`. Both host and WASM targets clean.

## Assumptions

- **Single-segment trait names**: `Show` works; `mod::Show` doesn't (matches M07.4 / M07.5 path restrictions).
- **Type-param bound syntax** `<T: Trait>`: M07.5 parser already captures the bound string in `TypeParam.bound: Option<String>`. M07.6 promotes to `bounds: Vec<String>` for multi-bound. Where-clause syntax `where T: Trait` deferred.
- **Multi-bound `+` separator**: needs lexer support if `+` token isn't already used (it's used as the binary-add op `Plus`, so it's already lexed; parser context disambiguates).
- **`Self` type in trait declarations**: out of scope (no return-Self, no `fn clone(&self) -> Self`). Method return types are concrete or use the trait's existing type-params (no associated types either).
- **Methods take `&self`, `&mut self`, or `self`** — supported via M07.4's existing `ParamKind`. No new self-receiver shapes.
- **Trait dispatch on generic-typed receivers**: when typeck sees `x.method()` inside `fn print<T: Show>`, the method lookup consults the bound (Show's methods) for the substituted type. This is the headline pedagogy.
- **Method-name ambiguity** (one name on multiple bound traits): hard error suggesting UFCS even though UFCS is out of scope — the learner needs to know the path forward exists, even if they can't take it yet.
- **Inherent impls win over trait impls** (M07.4 tie-breaker extension): if Point has `impl Point { fn show ... }` AND `impl Show for Point { fn show ... }`, the inherent wins. M07.4 already established this rule; M07.6 inherits it. Pedagogically clean: inherent ≈ "the type's own methods", traits ≈ "extension via shared behavior".
- **Default method dispatch routes through trait's body**: when an impl doesn't override a default-bodied method, dispatch resolves to the trait's body (with self bound to the receiver). The trait's body calls back to `self.other_method()` which dispatches via the impl's overrides.
- **Trait methods do NOT have their own type-params** (`fn bar<U>(&self) -> U`): out of scope (mirrors M07.5's no-method-level-type-params restriction).
- **Impl-on-builtin types** `impl Show for i32`: IN scope. The trait dispatch table uses `(trait_name, type_rendered_name)` as the key — works for any type whose `Ty::name()` is stable. Pedagogically: a learner writing `impl Show for i32 { ... }` shouldn't get an arbitrary "out of scope" error.
- **No trait objects, no `dyn`, no vtable**: M07.6 is static-dispatch only. Trait objects deferred to a future "dynamic dispatch" milestone (M08? M09?).
- **No derive macros**: `#[derive(Debug)]` etc. would need built-in std-trait impls. Deferred.
- **Frame-name format for trait methods** — plan-phase decides between `<Point as Show>::show` (Rust-standard, verbose) vs `Point::show` (M07.4-style, simpler — but conflates with inherent impls). Recommendation: `<Point as Show>::show` for trait dispatch, distinct from `Point::show` for inherent dispatch — makes the dispatch path visible.
- **Bundle target ≤ +25%**: substantial new surface (trait registries, bound-checking, third-layer dispatch, multi-bound parsing). Larger than M07.5's +20% because the dispatch story expands meaningfully.
- **Sized XL** per the rubric: ~6 source modules touched (parse/{ast, parser}, resolve [trait scope], typeck [registries + bound checking + dispatch], eval [trait-method frame entry], ui [trait-method frame label]) + 4 sample pairs + ≥ 10 unit tests. Estimated ~1200–1500 LOC net change. Comparable to M07.4 (struct + impl + methods).
- **Three plan-phase deferrals** (no NEEDS CLARIFICATION because reasonable defaults exist):
  1. **Frame-name mangling for trait methods**: `<Point as Show>::show` vs `Point::show`. Recommendation: the `<as>` form for visibility.
  2. **Trait impls on builtin types**: ship `impl Show for i32` as in-scope. Recommendation: accept (Rust-standard).
  3. **Where-clause syntax**: defer. Recommendation: bounds in `<T: Trait>` only.
- **Foundation completion**: M07.6 closes Level 4's polymorphism story. M07.4 = "model data" (structs); M07.5 = "abstract over types" (generics); M07.6 = "constrain those abstractions to behavior" (bounds). After M07.6 ships, the project has every "you can model your domain AND make it polymorphic" tool a learner needs.
