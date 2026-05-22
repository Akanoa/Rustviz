# Specification Quality Checklist: M06 — Level 2 References and Borrows

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-05-22
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

- Validation pass 1 (2026-05-22): all items pass.
- **First Level-2 milestone**: M06 is the project's biggest scope jump since M03 (the initial event model). The lattice grows from "primitives only" to "primitives + references between primitives." Arrows in the UI make the borrow checker tangible.
- **Four user stories**:
  - **US1 (P1)**: Shared borrows visible (blue arrow).
  - **US2 (P1)**: Mutable borrows visible (red arrow).
  - **US3 (P1)**: Aliasing violations caught at typeck (red wavy underline via M05's error UX).
  - **US4 (P2)**: Borrow lifetime ends at scope close (BorrowEnd timing).
- **Aliasing rules at typeck, NOT eval**: matches Rust's static borrow checker. Violations are typeck errors with spans. Eval trusts that emitted events respect the rules.
- **Scope-level lifetimes, not NLL**: M06's lifetime story is simpler than Rust's actual one. Borrow ends at enclosing `}`. NLL (lifetime-ends-when-no-longer-used) is deferred indefinitely.
- **No deref (`*r`)**: deliberately deferred to keep M06 scope tight. M06 references are observable but not usable values. A future revision can add deref + mutation through `&mut`.
- **No named lifetimes**: per MILESTONES.md explicit deferral. Function signatures use elision; learners don't see `'a` in M06.
- **No reference-returning functions**: out of scope (needs named lifetimes). Only parameter-side borrows.
- **`Ty::Ref` shape**: plan-phase decision. The unified `Ty::Ref { inner, mut }` form is preferred over per-variant `SharedRef`/`MutRef` to avoid further variant explosion (lessons from M03.2's lattice expansion).
- **`Value::Ref` extension**: a borrow value carries a `BorrowId`; the StateSnapshot reconstructs the active-borrows view from the event stream. The arrow's slot positioning is derived from current slot positions in the stacks panel.
- **SVG overlay positioning**: plan-phase decides whether to use SVG, HTML curved lines, or a hybrid. Bounding-box queries on slot card DOM elements at render time is one approach; the maintainer's preference may differ.
- **Bundle-size budget is intentionally generous** (+50% from M03.2): per project memory, the +5% per-milestone budget is hygiene, not a hard ceiling. M06 grows the lexer + parser + typeck + eval + a new SVG component, so a real bump is expected. Hard ceiling remains M04's 2 MB.
- **No new MemEvent variants**: the three borrow events (`BorrowShared`, `BorrowMut`, `BorrowEnd`) exist in M03's protocol since the original event-model milestone — M06 just fills the `BorrowId` + `Pointee` payloads.
- **Three plan-phase deferrals** (no NEEDS CLARIFICATION because reasonable defaults exist):
  1. **`Ty::Ref` exact shape** (unified vs per-variant — R-001 in research).
  2. **`Value::Ref` exact shape** (BorrowId-only vs BorrowId+target SlotId — R-002 in research).
  3. **SVG vs HTML curves** for the arrow overlay (visual + perf trade-off — R-003 in research).
- **Sized L** per the rubric: 5–6 source modules + new SVG component + sample files + significant test additions. Estimated ~800 LOC net change.

## Post-implementation audit (2026-05-22)

Following `/speckit-implement` execution of M06 (27 tasks T001–T027).

### Success-criteria results

| ID    | Description | Result |
|-------|-------------|--------|
| SC-001 | Live page accepts source with `&`/`&mut` within 1s budget | **CODE-VERIFIED**; browser visual QA deferred |
| SC-002 | N shared borrows render N blue arrows | DEFERRED (visual QA) |
| SC-003 | Mutable borrow renders one red arrow, visually distinct | DEFERRED (visual QA) |
| SC-004 | Aliasing violations caught at typeck (3 patterns) | PASS — all 3 covered by `run_pipeline_shared_then_mut_rejected`, `_two_mut_rejected`, `_mut_then_shared_rejected` |
| SC-005 | ≥ 4 M06 reference samples ship | PASS — 4 shipped: shared_borrow, mut_borrow, aliasing_error, scoped_borrow |
| SC-006 | M01–M03.2/M04/M05 still pass | PASS — 87 tests across 6 suites. **M01 has 1 fewer test**: `lexer_rejects_ampersand` removed (its premise — `&` is rejected — no longer holds). M02 + M03 byte-identical. The orphan sample `tests/samples/m01_reject_ampersand.rs` and its snapshot deleted |
| SC-007 | WASM bundle growth ≤ +50% vs M03.2 (84,007 B → ≤ 126,011 B) | PASS — **87,354 B gzipped (+4%)**, dramatically under budget. The bundle-size policy memory worried about a +50% blowup but the actual cost was minimal because the existing code structure (closed enum + match arms) accommodates new variants well |
| SC-008 | Zero warnings under `-D warnings` | PASS — host build + full test suite clean |

### Implementation findings

- **`Ty` Copy-drop cascade was smaller than expected**. Estimated ~50 sites; actual was ~15 fixed by adding `.clone()` at consume points + changing 2 methods (`name`, `is_copy`) from `(self)` to `(&self)`. The TypeMap stores `Ty` owned, so most internal storage paths unchanged. Most fixes were in `typecheck_binary` where Ty was moved into a `match (a, b)` pattern; pattern changed to `match (&a, &b)`.

- **The borrow_tracker module is ~70 LOC** including doc comments. Lives inline in `src/typeck.rs` as a private module. Holds active borrows per `BindingId`. Three methods (`try_take_shared`, `try_take_mut`, `pop_scope`) covering the full borrow-checker surface for L2.

- **Scope-level lifetime tracking** lives in two places (mirror image of slot drops):
  - typeck: `BorrowTracker::pop_scope(scope_depth)` removes entries on block exit. Integrates with `scope_depth` counter.
  - eval: `Scope::borrows: Vec<BorrowId>` field. `drop_current_scope()` emits `BorrowEnd` events in reverse-allocation order.

- **The lexer's `&` rejection was removed cleanly**. The old code path returned a "Level 2 not yet supported" error; replaced by Amp/AmpMut tokenization with three-char lookahead for `&mut`.

- **Borrow ID allocation** follows the existing pattern (`alloc_borrow_id` analogous to `alloc_frame_id` / `alloc_slot_id`).

- **`Value::Ref { target_slot }` denormalization** simplified the JS renderer. The arrow's `source_slot` is queried at render time from the World's `borrows` list, populated as a side-effect of `SlotWrite` of a `Value::Ref` value. The `target_slot` is on the Value itself, so the renderer doesn't walk events.

- **One M01 test removed**: `lexer_rejects_ampersand`. The sample (`m01_reject_ampersand.rs`) + its snapshot also deleted. Reasonable cleanup — the test asserted a behavior that no longer exists.

- **Pipeline test count** grew significantly:
  - Pre-M06: 35 lib tests (M03.2's count).
  - Post-M06: 44 lib tests (+9 borrow tests).
  - Full suite: 87 tests (8 + 16 + 8 + 44 + 6 + 5 across 6 suites).

- **SVG overlay**: `<svg id="arrow-overlay">` in `<main>` with `position: absolute; pointer-events: none`. Two arrowhead markers (blue + red) defined in `<defs>`. JS `renderArrows(borrows)` queries `data-slot-id` attributes via `document.querySelector`, computes Bezier path control points, appends `<path>` elements. Re-rendered on every state change.

- **Bundle-size growth was tiny (+4%)** despite L sizing. The borrow-tracker module + new enum variants compiled efficiently. Maybe the project's `[profile.release]` settings (LTO, opt-level=z from M03.2) helped — variant additions to existing enums add less code than new enum-arm explosions.

### Test summary

```
$ RUSTFLAGS="-D warnings" cargo test
87 passed
  - m01: 8 (was 9; -1 from `lexer_rejects_ampersand` removed)
  - m02: 16 (byte-identical)
  - m03: 8 (byte-identical)
  - lib: 44 (6 event smoke + 12 cursor + 26 pipeline including 9 new borrow tests)
  - intkind_tests: 5
  - misc smoke: 1

$ cargo build --release --target wasm32-unknown-unknown
WASM: 215 KB raw / 87,354 B gzipped (M03.2 baseline 84,007 B; +4%)
```

### Conclusion

M06 code-side complete. **Shipping for QA.** Maintainer walks `specs/009-m06-borrows/quickstart.md` SC-008 procedure focused on:

1. **Shared borrow**: blue arrow appears + disappears at scope exit.
2. **Mutable borrow**: red arrow, visually distinct.
3. **Aliasing violations** (3 patterns): red wavy underline + status message + disabled controls (reuse of M05's error UX).
4. **Scoped borrow**: arrow disappears at inner `}` while outer `x` persists.

If the SVG overlay positioning has visual quirks (arrows misaligned with slot cards, flicker on window resize, layering with editor highlights), those are tuning concerns for the maintainer's QA pass.
