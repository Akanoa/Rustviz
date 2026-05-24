# Specification Quality Checklist: M07.7 — Trait objects

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
- **Closes the Level 4 polymorphism trilogy**: M07.5 (generics) + M07.6 (traits, static dispatch) + M07.7 (traits, dynamic dispatch). After M07.7 ships, the project demonstrates every Rust polymorphism mechanism a learner needs.
- **Four user stories** — US1+US2+US3 P1 (basic &dyn, &dyn param, Box<dyn>); US4 P2 (static-vs-dyn side-by-side comparison).
- **Headline pedagogy** — fat pointer + vtable: `&dyn Show` is 16 bytes (data + vtable) vs `&T`'s 8; method dispatch goes through a runtime indirection visible as a two-step arrow at the call site. A new VTABLES panel (analog of M07.2's static-memory region) holds one box per `(type, trait)` pair.
- **Pedagogical contrast with M07.6** — the SHIP-DEFINING moment: US4's side-by-side sample shows `fn s<T: Show>(x: T)` (static, monomorphized `s::<Point>` frame) vs `fn d(x: &dyn Show)` (dynamic, ONE `d` frame + vtable lookup). Same input, different dispatch flavors. Place LAST in the dropdown so learners internalize the foundational forms first.
- **Sized XL** — explicitly chosen scope. Adds AST node + Value variant + MemEvent variant + new UI panel (VTABLES) + fat-pointer slot rendering + two-step dispatch arrows. ~1500-1800 LOC; comparable to or slightly larger than M07.4.
- **11th invocation of the closed-enum-with-revisions rule** — additive `Type::DynTrait`, `Ty::DynRef`, `Value::DynRef`, `VtableAddr`, `MemEvent::VtableAlloc`. First M07.x to add a new MemEvent variant since M07.2 (where StaticAlloc + BytesCopy landed). Pure additive — no field extensions, no variant restructures.
- **Meaty UI surface** — similar to M07.4's struct view. The fat-pointer rendering in the slot + the VTABLES panel + the two-step dispatch arrows are the iterate-on-this pieces. **UX checkpoint after first cut** is appropriate per the M07.4 precedent.
- **Three plan-phase deferrals** (no NEEDS CLARIFICATION because reasonable defaults exist):
  1. `Pointee::Vtable(VtableAddr)` vs separate field on `Value::DynRef` — recommendation: separate field (vtables aren't borrow targets in the M07.4-7 sense).
  2. Vtable interning timing — recommendation: lazy (mirror M07.2's StaticAlloc pattern; only used vtables get emitted).
  3. Dispatch arrow CSS / visual styling — iterative; UX checkpoint. Recommendation: dashed orange or muted blue for the vtable indirection (visually distinct from solid borrow/owning arrows).
- **Tight restrictions** keeping scope tractable: single-trait objects only (no `&dyn A + B`); no bare `dyn Trait` (always behind borrow/Box); no `impl Trait` sugar; no `Vec<Box<dyn Trait>>` heterogeneous collection (explicit out-of-scope); no upcasting; no `?Sized`; no `fn` pointers.
- **Vtable interning** matches Rust's linker behavior: one vtable per (trait, type) pair across the whole binary. Visually: one VTABLES box per pair, never more. Multiple `&dyn Show` borrows of Point share one vtable.
- **Frame-name format**: same `<Point as Show>::show` UFCS-style as M07.6 static dispatch. The dispatch path differs (static vs dynamic) but the resolved-method frame name is the same — makes the contrast visible at the OUTER frame (`s::<Point>` vs `d`) without confusion at the INNER frame.
- **WASM bundle target ≤ +25%** vs M07.6 baseline (378,170 B → ≤ ~473 KB raw). Substantial new surface justifies the larger budget.
- **Foundation for future work**: after M07.7, "polymorphism" is fully shipped. Future Level-4 milestones (auto-traits `Send`/`Sync` for M08, derive macros, associated types, supertraits) layer on top.
