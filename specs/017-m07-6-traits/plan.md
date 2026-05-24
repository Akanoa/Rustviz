# Implementation Plan: M07.6 — Traits (declarations, impls, static dispatch via bounds)

**Branch**: `017-m07-6-traits` | **Date**: 2026-05-24 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/017-m07-6-traits/spec.md`

## Summary

Introduce Rust's trait system end to end: `trait Show { fn show(&self) -> i32; }`, `impl Show for Point { ... }`, default methods, and trait-bound generics `fn print<T: Show>(x: T)`. **Headline pedagogy**: the bound is the proof — inside `fn print<T: Show>`, the body can call `x.show()` because the bound proves T implements Show. M07.6 closes Level 4's polymorphism story: M07.4 (structs) + M07.5 (generics) + M07.6 (bounds = polymorphism payoff).

**10th invocation of the closed-enum-with-revisions rule**: additive `Item::Trait` on AST; extends `Item::Impl` with `trait_name: Option<String>` (None = inherent, Some = trait impl); extends `TypeParam` from `bound: Option<String>` (M07.5 placeholder) to `bounds: Vec<String>` (multi-bound). No new `MemEvent` variants, no new `Ty`/`Value`/`Pointee` variants. The third dispatch layer (builtins → inherent → trait) extends M07.4's tie-breaker pattern.

Authority chain: `MILESTONES.md` › M07.6 → `spec.md` (this feature) → this plan.

## Technical Context

**Language/Version**: Rust 2024 edition (same toolchain as M01–M07.5). No new toolchain requirements.
**Primary Dependencies**: existing `indexmap`, `serde`, `serde_json`, `wasm-bindgen`, `js-sys`, `console_error_panic_hook`. **No new Rust deps**. **No JS deps changes**.
**Storage**: in-memory; no new files. Trait + trait-impl registries live in typeck's `Typechecker` (`TraitRegistry { schemas: IndexMap<String, TraitSchema> }` and `TraitImplRegistry { impls: IndexMap<(String, String), TraitImpl> }`). M01/M02/M03 snapshot tests should stay byte-identical for programs that don't use traits (additive Item variant + serde-default-empty on extended fields); M01/M02 may re-baseline once if any existing snapshot's debug output picks up the new fields even when empty.
**Testing**: existing `cargo test --test m01 / m02 / m03` should pass byte-identical (or re-baseline once for empty Vec fields per M07.5 precedent). New `cargo test --lib pipeline::tests` covering: trait decl + impl + dispatch, default methods, generic bound, multi-bound, missing required method, extra method, inherent-wins, ambiguous-method, bound-not-satisfied, builtin-impl (impl Show for i32). **≥ 10 new tests**. Manual M07.6 QA per the quickstart procedure.
**Target Platform**: same as M01–M07.5 (host + `wasm32-unknown-unknown`).
**Project Type**: Rust library + companion UI. Touches ~6 source modules (parse/{ast,parser}, resolve [trait scope], typeck, eval, ui [optional frame-label format adjustment]). Sized XL — comparable to M07.4.
**Performance Goals**: same pipeline latency budget. Bound checking is O(B × M) per call site (B = number of bounds, M = number of methods checked); registry lookups are O(1) via IndexMap.
**Constraints**: M03 byte-identical; M01/M02 may re-baseline once for new `bounds: Vec<String>` Vec promotion AND new TraitRegistry/TraitImplRegistry fields on TypeMap if they leak; WASM bundle ≤ +25% vs M07.5 baseline (342,873 B → ≤ ~429 KB raw) per SC-012; zero warnings under `-D warnings` (SC-013); existing M01–M07.5 features preserved.
**Scale/Scope**: ~6 source modules + 4 sample pairs + ≥ 10 new unit tests. **Estimated ~1200–1500 LOC net change**. Sizing: **XL** per the rubric — comparable to M07.4 (trait registries + third-layer dispatch + bound-checking + default-method routing all interact).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

`.specify/memory/constitution.md` is still the unfilled speckit template. Same vacuous PASS as features 001–016.

**Post-design re-check**: still vacuous, still PASS.

## Project Structure

### Documentation (this feature)

```text
specs/017-m07-6-traits/
├── plan.md                          # This file
├── spec.md                          # Feature spec
├── research.md                      # Phase 0: 17 design decisions
├── data-model.md                    # Phase 1: Item::Trait, TraitImpl ext, TraitRegistry, TraitImplRegistry, TypeParam.bounds
├── quickstart.md                    # Phase 1: dev workflow + manual QA procedure
├── contracts/
│   └── m07-6-protocol-delta.md      # Phase 1: 10th closed-enum invocation
└── checklists/
    └── requirements.md              # From /speckit-specify (16/16 PASS)
```

### Source Code (repository root) — files M07.6 touches

```text
src/
├── parse/
│   ├── token.rs                # Unchanged — `Plus` token already exists (binary add op); parser context disambiguates the `T: A + B` bound case from arithmetic.
│   ├── lexer.rs                # MODIFIED — extend KEYWORDS with `"trait"` → `TokenKind::Trait`. `"for"` MAY need adding too (currently not a keyword since pre-M07.6 has no loops); used in `impl Trait for Type`. Verify and add.
│   ├── ast.rs                  # MODIFIED — add `Item::Trait(TraitDecl)`. Add `TraitDecl { name, items: Vec<TraitItem>, span }`. Add `TraitItem` enum: `Required { sig: FnSig-like AST, span }` or `Default { decl: FnDecl }`. Extend `ImplBlock` with `trait_name: Option<String>` (None = inherent, Some = trait impl). Promote `TypeParam.bound: Option<String>` to `bounds: Vec<String>` (M07.5 stored at most 1; M07.6 supports `+`-separated multi-bound).
│   └── parser.rs               # MODIFIED — `parse_item` dispatches `Trait` → new `parse_trait_decl`. `parse_struct_decl` / `parse_fn_decl` already call `parse_type_params` (M07.5); extend the bound-parsing inside to consume `+`-separated bounds. `parse_impl_block` extended: after `impl`, look for `<Trait>` for trait_name `for <Type>` shape — peek for `for` keyword to distinguish trait impl from inherent. `parse_trait_decl`: consume `trait`, expect ident (name), expect `{`, parse trait items (each a `fn` decl — body optional → distinguishes required vs default).
├── resolve.rs                  # MODIFIED — `resolve_fn` already pushes type-param scope (M07.5); no change. `Item::Trait` arm: traits don't introduce value-level bindings. `Item::Impl { trait_name: Some(_), .. }` arm: similar — no new bindings. Trait-item method bodies (default methods) walk through `resolve_fn` so their bodies resolve normally — `self` is in scope as before.
├── typeck.rs                   # MODIFIED — **major surface**. Add `TraitRegistry { schemas: IndexMap<String, TraitSchema> }` where `TraitSchema { required_methods: IndexMap<String, FnSig>, default_methods: IndexMap<String, &'a ast::FnDecl> }`. Add `TraitImplRegistry { impls: IndexMap<(String, String), TraitImpl> }` where `TraitImpl { overrides: IndexMap<String, &'a ast::FnDecl> }`. Phase 1: collect trait declarations + trait impls alongside structs/inherent-impls; verify required methods are implemented (or default-bodied); reject extra methods in impls; reject duplicates. Phase 2: extend `typecheck_method_call` with third-layer dispatch (after builtin + inherent fall-through, search trait impls — order: get receiver's type name; for each trait in `current_bounds` of the receiver's type-param if it's a `Ty::Param`, OR for any trait that has an impl for the receiver's concrete type if not a Param, search for the method). Extend `typecheck_generic_free_call` to verify bounds: after substitution, for each `<T: TraitX>` in the fn's type-params, check that the substituted concrete type has a `TraitImpl` for TraitX. Bound failure → typeck error citing both types. Extend `Typechecker.current_type_params` to also carry bounds (`Vec<Vec<(String, Vec<String>)>>` where each tuple is `(param_name, bound_trait_names)`) — used inside generic-fn body typecheck to allow method calls on Ty::Param when proven by bounds. Ambiguous method (one name on multiple bound traits) → typeck error suggesting UFCS.
├── event.rs                    # Unchanged — no new MemEvent variants, no new Ty/Value/Pointee variants.
└── eval.rs                     # MODIFIED — extend method dispatch in `eval_method_call` to check the trait-impl registry (third layer). When the dispatched method is a default method (not overridden by the impl), execute the trait declaration's FnDecl body instead of the impl's. When dispatched through a bound (inside a generic fn body's `x.show()`), look up the trait's impl for the substituted concrete type and dispatch to that. **Trait dispatch frame name**: `<Point as Show>::show` (Rust-standard, distinct from inherent `Point::show`) — drives the mangled `FrameEnter.fn_name`. Plan-phase confirms format.

src/ui.rs                       # MAYBE-MODIFIED — if `<Point as Show>::show` frame names need any special rendering (e.g. parsing the `<as>` syntax for display tweaks). Likely unchanged — the existing frame-card renderer treats `fn_name` as opaque text.

tests/
├── m01.rs / m02.rs / m03.rs        # M01/M02 may re-baseline once for `TypeParam.bound` → `bounds` promotion (the Debug output changes from `bound: None` to `bounds: []`). M03 should stay byte-identical.
└── samples/
    ├── (existing)                  # Unchanged.
    └── m07_6_*.rs                  # NEW (4 files): m07_6_trait_basic, m07_6_default_method, m07_6_generic_bound, m07_6_multi_bound.

web/
├── samples/                    # MODIFIED — add 4 m07_6_*.rs mirrors.
├── index.html                  # MODIFIED — dropdown grows 4 entries.
├── index.js                    # Unchanged — frame-card renderer treats fn_name as opaque text.
├── style.css                   # Unchanged — no new UI elements.
└── Trunk.toml                  # Unchanged.

# M03's contract amended for the 10th closed-enum invocation:
specs/004-m03-event-eval/contracts/m03-api.md   # MODIFIED — note M07.6 as the 10th invocation. AST gains `Item::Trait`; `Item::Impl` gains `trait_name: Option<String>`; `TypeParam` extended `bound: Option<String>` → `bounds: Vec<String>`. No event-side changes; no Ty/Value/Pointee changes. The headline pedagogy (`<Point as Show>::show` distinct frames) is a labeling change on existing `FrameEnter.fn_name`.
```

**Structure Decision**: similar scope to M07.4 (also XL). The substitution machinery from M07.5 is the foundation; M07.6 adds the registries + third-layer dispatch + bound checking on top. No new event variants, no new UI rendering surface — the trait-method dispatch reuses M07.5's mangled-frame-name plumbing, just with a different format (`<Point as Show>::show`).

## Complexity Tracking

> No constitutional violations. Table omitted.

### Notable non-trivial complexity

- **Trait + impl-for + bound parsing**: three new syntactic constructs that share parser machinery. `trait Show { ... }` parses similar to a struct decl with fn items instead of fields. `impl Show for Point { ... }` extends M07.4's `parse_impl_block` with a peek for the `for` keyword (which itself may need lexer support). Bound parsing `T: A + B` extends M07.5's `parse_type_params` to handle `+`-separated bound chains.
- **Three-layer dispatch (M07.6's expansion of M07.4's tie-breaker)**: method-call resolution now has THREE layers:
  1. Hardcoded built-ins (Vec::push, String::push_str, etc.) — first match wins.
  2. User-defined inherent impls (M07.4).
  3. Trait impls (M07.6).
  Inherent wins over trait per the tie-breaker; built-ins win over both. Implementation: extend `typecheck_method_call`'s fall-through chain.
- **Bound-checking at call sites**: for `fn print<T: Show>(x: T)` called as `print(p)`, after inferring `T = Point`, look up `TraitImplRegistry[("Show", "Point")]`. If absent → "the trait bound `Point: Show` is not satisfied". The bound info comes from the fn's type-params (each tagged with their bound traits in `TypeParam.bounds`).
- **Method calls on type-param-typed values inside generic body**: `fn print<T: Show>(x: T) { x.show() }` — inside the body, `x` has type `Ty::Param("T")`. Method dispatch on `Ty::Param("T")` consults the param's BOUNDS (from `current_type_params` extended with bound info). For each bound trait, look up the method in the trait's schema. First-bound match wins; ambiguity error if multiple bounds define the same method name.
- **Default-method dispatch**: when an impl provides only `count` and the trait provides a default `double { self.count() * 2 }`, calling `p.double()` should:
  1. Look up `TraitImplRegistry[("Counter", "Point")].overrides[double]` — absent.
  2. Fall through to the trait's `default_methods[double]` body.
  3. Execute that body with `self` bound to `&p`. The body's `self.count()` calls back through the standard dispatch.
- **Method-name ambiguity in multi-bound**: `fn foo<T: A + B>(x: T)` where both A and B have a method `name`. The error message suggests UFCS (`A::name(&x)`) even though UFCS is out of scope — pedagogical "you have a path forward" signal.
- **Impl-for on builtin types** (`impl Show for i32`): the trait dispatch key is `(trait_name, type_rendered_name)`. For Point that's `("Show", "Point")`; for i32 that's `("Show", "i32")`. Works uniformly because `Ty::name()` gives stable strings for all types.
- **`Self` inside trait/impl method body** — out of scope per the no-`Self`-return restriction. `self` (lowercase) for the receiver is supported (M07.4 machinery).
- **No new Ty/Value/Pointee variants**: M07.6's polymorphism is entirely a static-dispatch + bound-checking concern, not a runtime-shape concern. Trait-method calls produce normal `FrameEnter`/`SlotAlloc`/`SlotWrite`/`ReturnValue` events — just with the mangled name distinguishing trait from inherent.
- **Bundle growth ≤ +25%**: estimated +50–80 KB from trait registries + bound-checking + third-layer dispatch + default-method routing. Verify post-merge.
- **Method dispatch on `Self` type inside a default method's body** — e.g. `trait Foo { fn bar(&self) -> i32 { self.bar() } }` (recursive default) — should work since `self`'s type at dispatch resolution is the concrete impl type (the receiver), not `Self`. The default method body re-dispatches through standard machinery.
