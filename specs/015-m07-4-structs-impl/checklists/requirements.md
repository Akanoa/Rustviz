# Specification Quality Checklist: M07.4 — Structs + `impl` blocks

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
- **First milestone introducing user-defined types** — structs are the primary tool a learner uses to model data. After M07.4 the project ships every "model your domain" tool a learner needs (generics + traits + enums come later as Level 5 expansions).
- **Three P1 user stories** (struct decl + literal + field access / field borrow / method dispatch) all coequal. US4 (associated functions) at P2 — important but subset of dispatch machinery.
- **Headline pedagogical points**:
  1. Struct's byte layout is visible in the slot (per-field cell strips with labels).
  2. Field borrows view a sub-region of a composite value (per-field hover highlight).
  3. Methods extend the dispatch story from M07's hardcoded built-ins to user-defined behavior.
- **Sized XL** — explicitly chosen scope. Considered splitting into M07.4 (basic structs) + M07.5 (impl/methods); bundled for cohesive pedagogy. Plan-phase will likely identify a 4-US split (struct decl, field borrow, method dispatch, associated fn) but ship them as one milestone unless implementation slips to XXL.
- **8th invocation of the closed-enum-with-revisions rule** — additive `Ty::Struct` + `Value::Struct` + possibly `Value::Ref` field metadata extension.
- **No new MemEvent variants**: existing `SlotAlloc`/`SlotWrite`/`FrameEnter`/`FrameLeave` carry struct values and method-call frames.
- **No new Pointee variants**: `Pointee::Slot(_)` (M03/M07.3) carries field borrows just like array slice targets.
- **Reuses M07.3's inline-cell pattern**: same `.stack-inline-cells` + `.stack-elem-labels` machinery — extended per-field instead of per-element.
- **Reuses M07.3's slot-target borrow lazy materialization**: field borrows skip BorrowShared events (slot disappears with frame; lifecycle is invisible).
- **Two-pass typeck** required for forward references — collect struct + impl signatures in pass 1, typecheck function bodies in pass 2. Non-trivial change to typeck flow.
- **Three plan-phase deferrals** (no NEEDS CLARIFICATION because reasonable defaults exist):
  1. `Value::Ref` field metadata shape (extension vs new variant) — recommendation: extension.
  2. Field-assignment scope (`p.x = 5;`) — partial; include if it falls out cleanly from extending M06.1.
  3. Auto-deref for `self.x` (parser sugar vs typeck rule) — recommendation: typeck rule.
- **Tight out-of-scope list**: generics, traits, derives, update syntax, tuple/unit structs, pattern matching, recursive structs, non-Copy field types, multiple impl blocks. Keeps the milestone tractable.
- **WASM bundle target ≤ +25%** vs M07.3 baseline (294,655 B). Larger budget than recent milestones reflecting the XL scope.
- **Foundation for future work**: M07.4 unlocks user-defined types. Generics (M07.5?), traits (M07.6?), enums (M07.7?), derives all layer on top.
