# Specification Quality Checklist: M07.1 — Slices

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-05-23
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

- Validation pass 1 (2026-05-23): all items pass.
- **First slice-related milestone in the project** — Rust's slice primitive (`&[T]`) had no representation in any prior milestone. M07.1 fills this gap and lays the foundation M07.2 (`&str` + static memory) will build on.
- **Three P1 user stories** (all coequal — partial slice, full slice, dangling slice). Each demonstrates a different aspect of the slice mechanic. The dangling case extends M07's existing pedagogy to slice granularity.
- **No new MemEvent variants**: existing `BorrowShared` / `BorrowMut` / `BorrowEnd` carry slice borrows; the slice's length lives in the slot's Value, not in the borrow event.
- **Ty/Value extensions**: a slice type representation is needed. Plan-phase decides exact shape — likely `Ty::Slice(Box<Ty>)` distinct from `Ty::Ref`, or a unified shape. Either way it's an additive variant per the closed-enum-with-revisions rule (5th invocation after M03.1/M03.2/M06/M07).
- **Length annotation visual** on borrow arrows is the headline UI addition. Slices must be visually distinguishable from single-element borrows — `&v[0]` (one element) vs `&v[..]` (many) should look different.
- **Out-of-scope items explicitly listed**: mutable slices, iterator methods, slice methods beyond `len()`, slicing a slice, standalone range expressions, range bounds with non-Int types, slicing non-Vec receivers. Tight scope.
- **Foundational for M07.2** (per MILESTONES.md): `&str` will reuse the slice type, length-annotation visual, and borrow infrastructure introduced here. M07.2 only adds the static-memory region + literal-typing rule on top.
- **Three plan-phase deferrals** (no NEEDS CLARIFICATION because reasonable defaults exist):
  1. **Range AST shape** — single Expr::Range variant vs four explicit forms vs only-inside-Index sugar. Single variant with Option<Box<Expr>> bounds is cleanest.
  2. **Slice type representation** — `Ty::Slice(Box<Ty>)` distinct vs unified with Ty::Ref via flag. Distinct is cleaner; reuses M03.2 "unified variants" lesson.
  3. **Length annotation visual** — text label vs second arrow vs tooltip. Inline text label `[len: N]` near arrowhead is simplest.
- **Sized L** per the rubric: ~4 source modules, ~600 LOC, 3 sample pairs. Smaller than M07 (1500 LOC) because much of the heap + borrow infrastructure is reused.
- **No protocol changes** — existing events carry slices; existing Pointee::Heap still represents the target. Only Ty + Value extensions, both additive.

## Post-implementation audit (2026-05-23)

| Success criterion | Status | Evidence |
|---|---|---|
| SC-001 (`&v[1..3]` typechecks, BorrowShared emits, arrow shows `[len: 2]`) | code ✓ / visual ⏳ | `run_pipeline_slice_range` test ✓; visual deferred to maintainer QA |
| SC-002 (all four range forms parse + typeck) | ✓ | `run_pipeline_slice_all_forms` (`&v[..]`, `&v[1..]`, `&v[..2]`, `&v[0..2]` → lengths 3,2,2,2) |
| SC-003 (dangling slice → RuntimeError at realloc) | ✓ | `run_pipeline_slice_dangling` |
| SC-004 (OOB range → RuntimeError) | ✓ | `run_pipeline_slice_oob_end`, `run_pipeline_slice_oob_start_gt_end` |
| SC-005 (`s.len()` returns slice's len, not Vec's) | ✓ | `run_pipeline_slice_basic` and `run_pipeline_slice_all_forms` |
| SC-006 (≥ 3 new `m07_1_*.rs` samples) | ✓ | `m07_1_slice_basic.rs`, `m07_1_slice_range.rs`, `m07_1_slice_dangling.rs` in `tests/samples/` + `web/samples/` |
| SC-007 (existing M01–M07 byte-identical) | ✓ | Starting tests: 102 passing; after M07.1: 110 passing (+8 new tests; existing 102 unchanged) |
| SC-008 (WASM bundle ≤ +25% vs M07) | ✓ | Raw release `.wasm`: 273,852 B (vs ceiling 1,131,463 B per plan); gzipped: 103,302 B |
| SC-009 (zero warnings under -D warnings, both host + WASM) | ✓ | `RUSTFLAGS="-D warnings" cargo build/test --release`: clean; `cargo build --release --target wasm32-unknown-unknown`: clean; verified after `cargo clean` |

### Implementation notes

- **Dangling-detection extension** (T020): M07's scan inspected only `Value::Ref { target: Pointee::Heap }` on locals. **One arm added** in `realloc_heap` for `Value::Slice { target: Pointee::Heap }` — same error message, same span source (`local.decl_span`). Zero refactor needed.
- **Slice typing via peephole rule** (T012): `Expr::Borrow.inner = Expr::Index { index: Expr::Range }` is detected structurally in the Borrow typeck arm; routed to `typecheck_slice_borrow` which returns `Ty::Slice(T)` directly (NOT wrapped in `Ty::Ref`). Matches Rust's `&[T]` shape.
- **Range parsing scope** (T006): `parse_expr` does not recognize `..`. Only `parse_index_inner` (called between `[` and `]`) accepts the four range forms. Standalone `..` becomes a parse-stage error. The earlier T010 plan-phase note about "typeck-rejects standalone Range" was redundant given the parser-side guard — kept the typeck arm as a defensive panic.
- **`Value::Slice` is a sibling of `Value::Ref`** (T008): not an extension via `Option<len>`. The variant-level discriminator keeps the rendering decision trivial (Slice → annotated arrow; Ref → plain arrow) and avoids the "is this a fat pointer? check the Option" fragility.
- **Length annotation visual** (T015): SVG `<text class="arrow-len-label">[len: N]</text>` element positioned near the arrowhead, with different X/Y offsets for heap-routed vs slot-routed arrows. Small (10px) blue monospace, `pointer-events: none`.
- **No M03 snapshot drift** — additive enum variants don't change existing variants' Debug output. Confirmed: 102 starting tests all still pass.
- **No new Rust deps, no new JS deps, no `Cargo.toml` changes**.
- **Bundle**: 273 KB raw / 103 KB gzipped. Well under the +25% ceiling. Slice infrastructure is additive variants + ~120 LOC of eval/typeck logic + ~30 LOC JS — small footprint.
