# Specification Quality Checklist: M07.5 — Generics

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
- **First milestone introducing type parameters** — opens the polymorphism story that M07.6 (traits) completes.
- **Three user stories** (P1 P1 P2): generic identity fn with monomorphization (the headline pedagogy), generic struct (extends M07.4 to "container holding any T"), turbofish call (explicit annotation ergonomics).
- **Headline pedagogical points**:
  1. **Monomorphization is visible**: `id(5)` and `id(true)` produce two distinct frames — `id::<i32>` and `id::<bool>` — encoded into `FrameEnter.fn_name`. Drives the "zero-cost-via-duplication" cost model.
  2. **Generic structs render with substituted types**: `Wrapper<i32>` in the slot's type label, not the source `Wrapper<T>`.
  3. **Inference vs turbofish**: arg-driven inference (`id(5)` → `<i32>`) is the common case; turbofish (`id::<bool>(false)`) is the explicit fallback.
- **Sized XL** — explicitly chosen scope. Smaller than M07.4 (no UI rendering surface beyond type-label substitution; substitution machinery mostly typeck-side). Estimated ~800-1100 LOC net change.
- **9th invocation of the closed-enum-with-revisions rule** — additive `Ty::Param(String)` for substitution.
- **No new MemEvent variants**: existing `FrameEnter` carries the mangled fn name (`id::<i32>`) — no event-shape changes.
- **No new UI surface for headline scenarios**: the type-label substitution reuses the existing struct-view (slot.struct_view) and slot type rendering; no new SlotRowView fields.
- **No new Pointee variants**, no new Value variants.
- **Tight restrictions** keeping the milestone tractable: single type param per fn/struct, no bounds (those land in M07.6), no const generics, no lifetime generics, no nested generic calls, no generic methods (method-level type params), no specific-instantiation impls.
- **Three plan-phase deferrals** (no NEEDS CLARIFICATION because reasonable defaults exist):
  1. `Ty::Param(String)` vs `Ty::Var(TyVarId)` — recommendation: `Ty::Param(String)` for simplicity.
  2. Mangled name format — recommendation: `id::<i32>` (Rust standard).
  3. Substitution-failure error message phrasing — recommendation: cite turbofish escape hatch.
- **WASM bundle target ≤ +20%** vs M07.4 baseline (310,880 B → ≤ ~373 KB raw). Smaller than M07.4 because no new UI rendering surface.
- **Foundation for M07.6**: M07.5 ships generics-without-bounds. M07.6 then adds trait bounds so `fn print<T: Show>(x: T) { x.show(); }` becomes expressible — the headline trait payoff.
