# Specification Quality Checklist: M07 — Level 3: Heap

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
- **Largest milestone since M04** — sized L per MILESTONES.md, **bordering on XL**. The spec flags this explicitly and authorizes plan-phase to consider an in-place split (M07a Box / M07b Vec+String+realloc) if mid-implementation sizing exceeds L on any axis. **Important constraint**: per MILESTONES.md, "do not ship the milestone without realloc animation" — M07a alone (Box only) would not close the milestone. Splitting is a delivery-pacing decision, not a milestone-restructure.
- **Three P1+P2 user stories**:
  - **US1 (P1)**: Box owning arrow. Foundational. MVP candidate.
  - **US2 (P1)**: Vec realloc + dangling-borrow detection. **Headline pedagogy.**
  - **US3 (P2)**: String allocation + push_str. Could ship in M07.1 if needed.
- **Significant new language features**:
  - **String literals** (`"..."`) — first time in the project. Lexer addition.
  - **Method-call syntax** (`v.push(x)`) — first time. Parser + typeck addition.
  - **Path expressions** for static calls (`Vec::new()`, `Box::new(v)`, `String::from(s)`) — first time. Parser addition.
  - **Indexing** (`v[0]`) — first time. Parser + typeck + eval addition.
- **`SlotWrite` / `BorrowShared` / `Note` reused; HeapAlloc / HeapRealloc / HeapFree filled in** — same protocol-reuse pattern as M06 (which filled the borrow events). No M03 protocol amendment for the heap events; `Ty` and `Value` get additive variants (4th invocation of the closed-enum-with-revisions rule).
- **Vec growth policy** assumed as doubling (initial 0 → 1 → 2 → 4 → 8 → ...). Plan-phase confirms. Affects which pushes trigger realloc and therefore the demo's reproducibility.
- **Indexing assignment** (`v[0] = 5;`) deferred — would require extending M06.1's place-expression set. Out of scope.
- **Method-call dispatch is structural**, not trait-based: hardcoded `(Ty, method_name) → signature` table. No user-defined methods, no traits, no `impl` blocks. Avoids massive scope creep.
- **Vec<T> only for primitive T** in M07 — `Vec<Box<i32>>` (nested heap) is out of scope. Plan-phase confirms.
- **Dangling-borrow detection scope**: borrows into Vec elements only. Borrows through Box (`&*b`) deferred (re-borrow-through-deref isn't in M06.1 either). String content borrows out of scope (no character access).
- **Heap panel rendering**: free-form per MILESTONES.md. Plan-phase decides flexbox vs JS-positioned absolute. Realloc animation should use CSS transitions on `transform` or `top`/`left` (~300ms target).
- **Bundle-size budget +60%** from M06.1 baseline. The bundle-size policy memory authorizes generous budgets for variant-growth milestones. Hard ceiling is M04's 2 MB.
- **Plan-phase deferrals** (no NEEDS CLARIFICATION; reasonable defaults exist):
  1. Method-call AST shape (separate variants vs unified Call).
  2. Vec growth policy.
  3. Heap panel layout strategy.
- **First Level-3 milestone** — third panel (heap) finally renders content. Establishes infrastructure that M08 (threads with Arc/Mutex) will lean on (Arc is a heap-allocated reference-counted owning type — same panel + same owning-arrow pattern).
- **Sized L** per the rubric: ~8-10 source modules touched + significant new UI work + new lexer/parser/typeck/eval features + new heap panel rendering. Estimated **~1200-1500 LOC net change**. By far the largest milestone since M03/M04.

## Post-implementation audit (2026-05-23)

Following `/speckit-implement` execution of M07 (29 tasks T001–T029).

### Success-criteria results

| ID    | Description | Result |
|-------|-------------|--------|
| SC-001 | Live page accepts new heap syntax within budget | **CODE-VERIFIED**; browser visual QA deferred |
| SC-002 | Box renders heap box + black owning arrow; disappears at scope close | DEFERRED (visual QA) |
| SC-003 | Vec realloc animation + dangling-borrow RuntimeError underline | DEFERRED (visual QA); code-verified via `run_pipeline_vec_dangling_borrow` |
| SC-004 | String shows contents; push_str triggers realloc | DEFERRED (visual QA); code-verified via `run_pipeline_string_push_str_realloc` |
| SC-005 | Vec OOB → RuntimeError | PASS — `run_pipeline_vec_index_oob` |
| SC-006 | M01–M06.1 tests stay passing | PASS — m01: 8 byte-identical, m02: 16 (one snapshot re-baselined for the typeck callee-error message change), m03: 8 byte-identical |
| SC-007 | WASM bundle ≤ +60% vs M06.1 (88,841 B → ≤ 142,146 B) | PASS — **99,455 B gzipped (+12%)**, well under budget despite the heap surface + new lexer/parser/typeck features. The closed-enum + match-arm structure absorbs new variants efficiently |
| SC-008 | Zero warnings under `-D warnings` | PASS — host build + full test suite clean, WASM target clean |

### Implementation findings

- **Largest single milestone since M03/M04** — 29 tasks, ~1500 LOC, all four new language features landed (string literals, method calls, path expressions, indexing). Tight cycle.

- **`Value::Ref` restructure was the trickiest task** (T008) — `target_slot: SlotId` → `target: Pointee`. Cascade affected ~10 sites across eval and ui. M01/M02/M03 stayed byte-identical because no existing samples constructed `Value::Ref` in a way that the Debug-format diff broke; only one M02 snapshot drifted from a typeck error-message change (callee path support).

- **`Ty::Box(_)` and `Ty::Vec(_)` are unified-variant style** matching the M03.2 `Ty::Ref` precedent. `Box<Ty>` recursion keeps Ty non-Copy (already non-Copy since M06).

- **`Box::new`, `Vec::new`, `String::from` dispatched via hardcoded table** in `typecheck_path_call`. Likewise `Vec::push`, `Vec::len`, `String::push_str` in `typecheck_method_call`. No traits, no `impl` blocks. Structural dispatch is sufficient for M07's scope and keeps the surface tight.

- **Vec::new eager-allocates with capacity 0** so a HeapAlloc event fires immediately. Plan-phase R-014 noted this as a simplification — pedagogically helpful (the heap panel shows the empty Vec). First push triggers realloc to capacity 1.

- **Vec growth = doubling** (0 → 1 → 2 → 4 → 8 → ...). For the headline `m07_vec_realloc.rs` demo with 3 pushes, the realloc count is 3 (push 1 → 0→1, push 2 → 1→2, push 3 → 2→4). The `&v[0]` borrow taken between push 2 and push 3 is invalidated at push 3.

- **Dangling-borrow detection** at HeapRealloc scans all active LocalSlot values for `Value::Ref { target: Pointee::Heap(from), .. }`. Each match produces a `Note { RuntimeError }` with the borrow's decl_span. Implemented; verified by `run_pipeline_vec_dangling_borrow`.

- **Heap-element borrow simplification**: M07 borrows `&v[0]` at the WHOLE-ALLOCATION level. The borrow's `Pointee::Heap(addr)` targets the Vec's buffer, not the specific element. Documented in research R-018.

- **`BorrowView` renamed to `ArrowView`** with `kind: ArrowKind` (Shared/Mut/Owning) and `target: ArrowTarget` (Slot(u32)/Heap(u32)). The JS-side `state.borrows` becomes `state.arrows`. Plan R-027.

- **Heap display includes contents** — ty_name field hijacked to carry the value/contents (`Box<i32> = 5_i32`, `Vec[1, 2] (cap=2)`, `String "hi" (cap=2)`). Simplification noted.

- **Realloc Info notes**: in addition to dangling-borrow RuntimeError notes, a generic Info Note fires at each HeapRealloc describing the new contents (`Vec[1, 2, 3] (cap=4)`). Helps the learner correlate the event stream with the heap state.

### Test summary

```
$ RUSTFLAGS="-D warnings" cargo test
102 passed
  - m01: 8 (byte-identical)
  - m02: 16 (one snapshot re-baselined for typeck callee-error msg)
  - m03: 8 (byte-identical)
  - lib: 58 (+8 new M07 tests: 2 Box + 4 Vec + 2 String)
  - intkind_tests: 5
  - misc: 7

$ cargo build --release --target wasm32-unknown-unknown
WASM: 247 KB raw / 99,455 B gzipped (M06.1 baseline 88,841 B; +12%)
```

### Conclusion

M07 code-side complete. **Shipping for QA.** Maintainer walks `specs/011-m07-heap/quickstart.md` SC-008 procedure focused on:

1. **Box (US1)**: load `Box (M07)`, observe heap box appear with black owning arrow from `b` to heap. Disappears at scope close.
2. **Vec realloc (US2, headline)**: load `Vec realloc (M07)`, observe heap box update + dangling-reference RuntimeError underline at `&v[0]` when the third push reallocates.
3. **String (US3)**: load `String (M07)`, observe heap box with `"hi"` content; updates to `"hi!"` after `push_str("!")` (with HeapRealloc if capacity grew).

Known simplifications:
- Heap-element borrows tracked at whole-allocation granularity (not per-element).
- Vec aliasing rules NOT enforced for heap borrows (would need per-element tracking).
- Box/Vec/String contents displayed via ty_name hijack — could be cleaner with a dedicated event variant in a future revision.
- Realloc animation relies on flex reflow + CSS transition; for single-allocation demos there's no visible position shift (the Note text carries the pedagogy instead).
