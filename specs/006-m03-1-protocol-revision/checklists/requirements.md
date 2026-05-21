# Specification Quality Checklist: M03.1 — Protocol Revision

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-05-21
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

- Validation pass 1 (2026-05-21): all items pass.
- **First revision milestone in the project**: M03.1 patches M03's event protocol after M03 shipped. The user-story framing is hybrid — the *learner* benefits visibly (US1 + US2), the *maintainer* validates non-regression (US3). Future revision milestones (if any) can borrow this dual framing.
- **"Closed enum" rule relaxation**: M03's contract said the `MemEvent` enum was closed from M03 onward. M03.1 needs to relax that to permit additive variants in revision milestones with maintainer consent. The relaxation is itself in scope (FR-003) and the implementation will update `specs/004-m03-event-eval/contracts/m03-api.md` in lock-step.
- **`FrameEnter.params` removal**: in scope per MILESTONES.md M03.1. The field was always redundant with subsequent `SlotAlloc`+`SlotWrite` events; M04's renderer ignored it. Removing it cleans up the contract.
- **No new user stories beyond pedagogical fixes + non-regression**: M03.1 is a focused revision, not a feature expansion. The 3 user stories cover exactly the two visible improvements and the safety guarantee.
- **`Ty::is_copy()` design**: borderline implementation detail in FR-001 / Key Entities. Kept because the design choice (method on `Ty`, not a free function or magic constant) affects how M07 extends the classification when non-Copy types arrive. The choice is plan-phase to refine; spec just signals "must be a method/extensible".
- **Trace count math in SC-004** (`13 − 3 + 2 = 12`): explicit so the audit can verify the event-stream diff exactly. The 3 removed `SlotDrop`s are for `a`, `b`, `r` (all Copy `i32`s in `m03_fn_call`); the 2 added `ReturnValue`s are one per non-erroring frame (`add`, `main`).

## Post-implementation audit (2026-05-21)

Following `/speckit-implement` execution of M03.1 (19 tasks T001–T019).

### Success-criteria results

| ID    | Description | Result |
|-------|-------------|--------|
| SC-001 | Re-baselined M03 snapshots: 0 `SlotDrop`s for L1; ≥ 1 `ReturnValue` per non-halted frame; deterministic | PASS — direct snapshot inspection confirms `SlotDrops=0` everywhere; `ReturnValues>=1` except `div_by_zero` (halts before return) |
| SC-002 | M01 / M02 byte-identical | PASS — `cargo test --test m01` (8) and `cargo test --test m02` (16) clean, no `.snap.new` files |
| SC-003 | New Cursor unit test for `ReturnValue` handling | PASS — `return_value_populates_pending_return` passes; lib unit tests now 10 (was 9 in M04) |
| SC-004 | Predicted event-count diff per sample | PASS — `gen_traces` output: arithmetic 5, fn_call 12, shadow 7, div_by_zero 2 — matches R-008 prediction |
| SC-005 | WASM bundle growth ≤ 5% vs M04 baseline (79,915 B gzipped) | PASS — **79,674 B gzipped** (235 KB raw); actually *decreased* by 241 B (−0.3%) because `FrameEnter.params: Vec<(SlotId, String, Value)>` removal outweighed the new `ReturnValue` variant addition |
| SC-006 | M04 manual QA: Copy slots persist; return-value visible | **DEFERRED to maintainer** per the UI QA-split convention |
| SC-007 | Zero warnings under `-D warnings` | PASS — host build + full test suite (48 tests across 6 suites) clean |
| SC-008 | Closed-enum rule relaxation documented; M03.1 cited as first invocation | PASS — `specs/004-m03-event-eval/contracts/m03-api.md` amended in T002 |

### Implementation findings

- **Two exhaustive matches needed updating** when `MemEvent::ReturnValue` landed: `src/ui.rs::event_span` (caught during T008 build) and `tests/m03.rs::event_span` (caught during T012 retry — the test driver has its own `event_span` for span assertions). The compiler's exhaustiveness check did exactly its job — flagged each site immediately.
- **`FrameEnter.params` removal forced the in-source test helper to update**: `src/ui.rs::tests::frame_enter` had a stale `params: Vec::new()` field that broke compilation. Updated as part of T009. M04's web/index.js `apply_event` already ignored the field via `..` pattern, so no JS changes needed for the removal.
- **`gen_traces` event counts match R-008 prediction exactly** — `m03_fn_call` 13 → 12, `m03_shadow` 8 → 7, `m03_arithmetic` 5 → 5 (−1 SlotDrop + 1 ReturnValue), `m03_div_by_zero` 2 → 2 (unchanged, halts before drops/returns). The full predicted table:

  | Sample | Pre | Post | Δ |
  |---|---:|---:|---:|
  | arithmetic | 5 | 5 | 0 |
  | fn_call | 13 | 12 | −1 |
  | shadow | 8 | 7 | −1 |
  | div_by_zero | 2 | 2 | 0 |

- **Bundle size *decreased*** because the removed `FrameEnter.params: Vec<(SlotId, String, Value)>` field was heavier in the serialized + in-memory enum representation than the new `ReturnValue { frame_id, value, span }` payload. Net win.
- **No JS changes needed for the `pending_return` JSON wire format**: serde derives Serialize/Deserialize on `PendingReturnView` automatically. JS reads `state.pending_return` exactly as the schema specifies.
- **M03 contract amended in lock-step (T002)**: the relaxed closed-enum rule lives in `specs/004-m03-event-eval/contracts/m03-api.md` and cites M03.1 as the first invocation. Future revision milestones (e.g. an eventual M07.1 if heap-drop semantics need refinement) inherit the policy.

### Test summary

```
$ RUSTFLAGS="-D warnings" cargo test
running 48 tests
- m01: 8 passed (byte-identical snapshots)
- m02: 16 passed (byte-identical snapshots)
- m03: 8 passed (re-baselined snapshots)
- lib: 16 passed (6 event smoke + 10 cursor with new ReturnValue test)
total: 48 passed; 0 failed; 0 ignored

$ cargo build --release --target wasm32-unknown-unknown
WASM: 235 KB raw / 79,674 B gzipped (was 79,915 B; −0.3%)
```

### Conclusion (initial)

M03.1 code-side complete. **Shipping for QA.** Maintainer walks `specs/005-m04-ui-shell/quickstart.md` SC-008 procedure with focus on US1 (Copy slots persist until FrameLeave) and US2 (return-value annotation appears for one cursor step).

### QA-driven enhancements (2026-05-21, after first code-side pass)

Maintainer QA found three pedagogical gaps in the initial implementation. Each was addressed in-branch before commit:

1. **Frame disappeared too fast on `FrameLeave`** — the value the just-returned function computed had no visible home between `FrameLeave` and the caller's `SlotWrite`. **Fix**: `apply_event` now marks the leaving frame as inactive (`FrameInProgress.active = false`) instead of popping it. The frame card lingers (grayed) so the return value stays observable while it lands in the caller. Same pedagogy as M03.1's Copy-drop change, lifted from slot granularity to frame granularity.

2. **`→ <value>` annotation disappeared at `FrameLeave`** — the transient `pending_return` field was scoped to "last event was ReturnValue", so the FrameLeave step cleared it. The grayed frame had no return-value indicator. **Fix**: moved the annotation to live on the frame itself (`FrameCardView.return_value: Option<String>`). When `ReturnValue` fires, `apply_event` records the rendered value on the matching frame; the annotation then persists through `FrameLeave` and beyond as long as the grayed frame is visible. `StateSnapshot.pending_return` stays in the snapshot but the renderer no longer reads it (kept additive for backward compat).

3. **Sequential calls accumulated grayed frames forever** — calling `add()` twice in `main()` left both `add` frames grayed, but real machine semantics says the second call overwrites the first call's stack region. **Fix**: `apply_event` for `FrameEnter` now pops any grayed frames sitting above the current top-active frame before pushing the new one. Models "bytes get overwritten when the stack pointer comes back around."

Plus a sample added to exercise the multi-call case: **`web/samples/m03_fn_call_twice.rs`** (`fn add(...) { ... } fn main() { let r1 = add(2,3); let r2 = add(4,5); }`). Registered in `src/bin/gen_traces.rs` and `web/index.html`'s sample selector. Trace event count: **21**.

The CSS for the grayed state was also iterated: slots area at 45% opacity but the header (frame name + `→ <value>` annotation) at full opacity, so the return value reads clearly on a "dead" frame.

### Maintainer QA: PASS

All four enhanced behaviors verified visually in the browser:

- **Function Call** sample: `a` and `b` stay visible across body completion; `→ 5` annotation appears and persists through the grayed `add` frame; `r` then receives `5`; both `add` and `main` end grayed with their return values.
- **Function Call (Twice)** sample: first `add` frame grays after returning `5`; **second `add` frame opens, overwriting the first grayed frame** (only one `add` ever visible at a time); both grays merge as `main` exits.
- **Other samples**: no regressions, no Copy-type slot disappearances mid-frame.

### Final test summary

```
$ cargo test
49 passed
  - m01: 8
  - m02: 16
  - m03: 8 (re-baselined)
  - lib: 17 (6 event smoke + 11 cursor including:
            return_value_persists_on_grayed_frame,
            frame_enter_overwrites_grayed_frames,
            frame_leave_grays_frame)

$ cargo build --release --target wasm32-unknown-unknown
WASM: 236 KB raw / 79,973 B gzipped (M04 baseline 79,915 B; +0.07%, well under +5% budget)
```
