# Specification Quality Checklist: M07.6 — Traits

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-05-24
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Notes

- Validation pass 1 (2026-05-24): all items pass.
- **Closes Level 4's polymorphism story**: M07.4 = "model data" (structs); M07.5 = "abstract over types" (generics); M07.6 = "constrain those abstractions to behavior" (bounds). After M07.6 ships, the project has every "you can model your domain AND make it polymorphic" tool a learner needs.
- **Four user stories** — US1+US2+US3 P1 (trait decl+impl+dispatch, default methods, generic bound), US4 P2 (multi-bound).
- **Headline pedagogy**: `fn print<T: Show>(x: T) { x.show(); }` — "the bound is the proof". Visible in the trace as: typeck rejects `print(5)` if `i32: Show` is unimplemented; accepts `print(p)` where `Point: Show`. The bound is what unlocks method calls on `T` inside the body.
- **Sized XL** — explicitly chosen scope. Adds trait registries, bound-checking machinery, third-layer dispatch (builtins → inherent → trait), multi-bound parsing. ~1200–1500 LOC.
- **10th invocation of the closed-enum-with-revisions rule** — additive `Item::Trait` (AST); extends `Item::Impl` with `trait_name: Option<String>`; extends `TypeParam.bound: Option<String>` → `bounds: Vec<String>`. No new MemEvent variants; no new Ty/Value/Pointee variants.
- **No new UI surface for headline scenarios**: trait-method dispatch reuses the existing frame-card renderer; the mangled name (`<Point as Show>::show`) flows through `FrameEnter.fn_name` automatically. No JS / CSS changes for the core pedagogy.
- **Tight restrictions** keeping the milestone tractable: static dispatch only (no `&dyn`), no associated types, no supertraits, no blanket impls, no derive macros, no where clauses, no generic trait methods, no `Self` return type, no UFCS.
- **Inherent-wins-over-trait** dispatch tie-breaker extends M07.4's pattern. Pedagogically clean.
- **Default-method dispatch routes through the trait's body** when no override; `self.other_method()` calls inside default bodies dispatch normally (the impl can override either method, both, or neither).
- **Method-name ambiguity error suggests UFCS** even though UFCS is out of scope — the learner needs a path forward marker.
- **Trait impls on builtin types** (`impl Show for i32`) IN scope — pedagogically required (Rust-standard; the alternative is an arbitrary "out of scope" error).
- **Three plan-phase deferrals** (no NEEDS CLARIFICATION because reasonable defaults exist):
  1. Frame-name mangling for trait methods — recommendation `<Point as Show>::show` for visibility.
  2. Trait impls on builtin types — recommendation: accept.
  3. Where-clause syntax — recommendation: defer; bounds in `<T: Trait>` only.
- **WASM bundle target ≤ +25%** vs M07.5 baseline (342,873 B → ≤ ~429 KB raw). Larger than M07.5 because the dispatch + bound-checking surface expands meaningfully.
- **Foundation for future work**: M07.6 completes the polymorphism story. Future milestones could add trait objects (`&dyn Trait` + vtable viz), associated types, derive macros, supertraits — all layer on top.
