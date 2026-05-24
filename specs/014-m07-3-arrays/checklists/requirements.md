# Specification Quality Checklist: M07.3 — Arrays (`[T; N]`)

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
- **First milestone where `Value::Slice { target: Pointee::Slot(_) }` is constructed** — M07.1 declared the Slot case; M07 built Heap; M07.2 built Static; M07.3 closes out the slice trilogy.
- **Three P1 user stories** (all coequal — basic array + len / indexing / slicing). Each demonstrates a distinct mechanic. The slicing one is the structural payoff.
- **Headline pedagogy** is the stack-vs-heap contrast with Vec. Same surface (`t[i]`, `&t[1..]`, `t.len()`), different storage (inline in stack slot vs heap-allocated). Zero heap events for arrays — pedagogically striking.
- **No protocol changes** — additive `Ty::Array` variant + AST literal/type nodes. No new MemEvent variants; existing `BorrowShared` (or skipped per M07.2's pattern) carries slot-targeted slice borrows.
- **Slot-targeted slice borrows** likely skip BorrowShared/BorrowEnd events (consistent with M07.2's Static treatment — frames disappear atomically, so scope-exit BorrowEnd would be a silent no-op). Plan-phase confirms.
- **No mutation through index** — `t[0] = 5;` deferred. Arrays in M07.3 are read-after-construction.
- **No repeat syntax** — `[v; N]` deferred to keep parser scope small. Literal-only `[e1, e2, ..., eN]` form is enough for pedagogical samples.
- **Tight out-of-scope list**: repeat syntax, multi-dimensional, non-Copy elements, mutation through index, iterator methods. Smaller than M07.1's scope.
- **Sized M** per rubric: ~4 source modules, ~500-700 LOC. Smaller than M07.1 because heap + slice infrastructure is fully reused.
- **No new Rust deps, no new JS deps**.
- **Three plan-phase deferrals** (no NEEDS CLARIFICATION because reasonable defaults exist):
  1. Inline byte-cell visual styling in the stack slot.
  2. Slot-targeted borrow lifecycle — skip events or emit them (recommended: skip).
  3. Function-signature support for `[T; N]` params.
- **Foundation for future work**: M07.3 completes the slice abstraction. Future milestones (index-position mutation, iterators, multi-dim arrays, repeat syntax) layer on top. M08 (threads) is independent.

## Post-implementation audit (2026-05-24)

| Success criterion | Status | Evidence |
|---|---|---|
| SC-001 (`let t = [1, 2, 3]` typechecks `[i32; 3]`, zero heap events, 12 inline cells) | code ✓ / visual ⏳ | `run_pipeline_array_basic` + `run_pipeline_array_no_heap` ✓; visual deferred to maintainer QA |
| SC-002 (`t.len()` returns U64 N) | ✓ | `run_pipeline_array_basic` |
| SC-003 (`t[i]` returns element / OOB → RuntimeError) | ✓ | `run_pipeline_array_index` + `run_pipeline_array_index_oob` |
| SC-004 (`&t[range]` typechecks `&[T]`, `Pointee::Slot` target, slot-to-slot arrow, `s.len()` returns range len) | code ✓ / visual ⏳ | `run_pipeline_array_slice` |
| SC-005 (OOB array slice → RuntimeError + halt) | ✓ | `run_pipeline_array_slice_oob` |
| SC-006 (≥ 3 new `m07_3_*.rs` samples) | ✓ | `m07_3_array_basic.rs`, `m07_3_array_index.rs`, `m07_3_array_slice.rs` in `tests/samples/` + `web/samples/` |
| SC-007 (existing M01–M07.2 byte-identical) | ✓ | Starting tests 119 → 125 (+6 new M07.3 tests); existing tests pass unchanged |
| SC-008 (WASM bundle ≤ +15% vs M07.2) | ✓ | M07.2 baseline 280,519 B; M07.3 raw WASM 294,655 B = +5.0% (well under +15% ceiling) |
| SC-009 (zero warnings under -D warnings, host + WASM) | ✓ | `cargo clean && RUSTFLAGS="-D warnings" cargo build/test --release && cargo build --release --target wasm32-unknown-unknown`: all clean |

### Implementation notes

- **Cascade fixes for `Ty::Array` / `Value::Array` exhaustive matches** (T002-T008): 7 sites — `ty_size_bytes`, `value_size_bytes`, `Ty::name`, `Ty::is_copy`, `ty_from_ast`, `Value::type_name`, `render_value`, `render_value_for_note`, eval_expr, typecheck_expr_inner. All mechanical.
- **No new `LiveSlot` constructor needed**: only the `SlotAlloc` arm constructs LiveSlot. Threaded `inline_cells: None` at SlotAlloc; populated at SlotWrite for `Value::Array`.
- **Slot-target slice borrow lifecycle skip generalized cleanly** from M07.2's Static-only path to `matches!(target, Pointee::Static(_) | Pointee::Slot(_))`. UI lazy-materialization already covered the Slot case (M06 path) — no UI-side change needed for the lifecycle.
- **No M03 snapshot drift** — confirmed by full test pass after the new variants landed.
- **No new Rust deps, no new JS deps, no `Cargo.toml` changes**.
- **Closes the slice trilogy**: `Value::Slice { target: Pointee::Slot(_) }` first constructed by `eval_slice_borrow` when receiver is `Value::Array`. After M07.3 all three Pointee variants (Slot/Heap/Static) carry the slice abstraction.
- **WASM bundle**: 294,655 B raw / ~+5% vs M07.2 baseline. Comfortably under the +15% ceiling — small additive surface as expected.
