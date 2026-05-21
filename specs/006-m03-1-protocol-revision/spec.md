# Feature Specification: M03.1 — Protocol Revision: Copy-drop + Return-value Bridge

**Feature Branch**: `006-m03-1-protocol-revision`
**Created**: 2026-05-21
**Status**: Draft
**Input**: User description: "M03.1"

**Authoritative scope source**: [`MILESTONES.md` › M03.1 — Protocol revision: Copy-drop + return-value bridge](../../MILESTONES.md). The In scope / Out of scope / Entry criteria / Exit criteria / Demo lines in that block are the contract for this feature. This spec elaborates user stories, requirements, and success criteria; it does not redefine scope.

## User Scenarios & Testing *(mandatory)*

M03.1 is the project's first **revision milestone** — it patches M03's event protocol after M03 shipped, based on pedagogical issues uncovered during M04's manual QA. The audience is dual: (a) the Rust learner running the rustviz page, who sees a more honest visualization of Copy semantics and function return values after the revision lands; and (b) the maintainer + future-milestone implementer, who relies on the revised event protocol as the foundation M05–M08 extend.

### User Story 1 — Copy-typed slots persist visually until the frame leaves (Priority: P1)

A Rust learner steps through `m03_fn_call` in the rustviz page. When the cursor advances past the function body's tail expression and into the scope-teardown phase, the `a` and `b` slots in the `add()` frame **do not disappear one by one** — they stay visible until the whole `add()` frame card vanishes at `FrameLeave`. This matches the machine-level reality: for Copy types (`i32`, `bool`) no destructor runs, the bytes physically persist on the stack, and only at frame exit does the storage become reusable.

**Why this priority**: this is the visible win the M04 QA pass identified. A learner currently sees slots vanishing one at a time at scope exit for primitives — that builds a wrong mental model about ownership vs. memory. P1 because it directly addresses the pedagogical critique that triggered creating M03.1.

**Independent Test**: open the rustviz page, select `Function Call`, Play through to the end. Observe: `a` and `b` are alive in `add()`'s frame card from their `SlotWrite` through the moment `add()` leaves. No mid-frame slot disappearance. Confirmed by the maintainer per the SC-008-style procedure.

**Acceptance Scenarios**:

1. **Given** a sample with a function call whose body uses only Copy-typed locals (e.g. `m03_fn_call`), **When** the cursor steps through the function body, **Then** every Copy-typed slot persists in the frame card until the `FrameLeave` event for that frame fires.
2. **Given** a sample with shadowing of Copy-typed bindings (e.g. `m03_shadow`), **When** the cursor steps through the shadowing statements, **Then** earlier shadowed slots stay visible until the outer block ends (rather than disappearing at the inner block's drop step).
3. **Given** the M03 event stream for any sample using Copy-only types, **When** the trace is inspected, **Then** there are zero `SlotDrop` events in the stream. Slot lifetimes are bounded by `SlotAlloc` and `FrameLeave` only.

---

### User Story 2 — The function's return value is visible between body completion and frame exit (Priority: P1)

A Rust learner steps through `m03_fn_call`. After the cursor reaches the body's tail expression, before the `FrameLeave` event closes the `add()` frame, a **return-value indicator** appears on `add()`'s frame card — making the value (`5`) that's about to flow back to the caller visible for one step. Then `FrameLeave` closes the frame and the next step shows the value landing in `r` via `SlotWrite` in the caller frame.

**Why this priority**: same P1 as US1 — both are the pedagogical fixes the M04 QA identified. Without this, the value appears in the caller from nowhere; the ABI return-value mechanic (return register / caller-provided slot) is invisible. P1 because closing this visualization gap is the second half of M03.1's scope.

**Independent Test**: open the rustviz page, select `Function Call`, Play through. Observe: between the last expression-evaluation step of `add()`'s body and `add()`'s `FrameLeave`, a `→ 5` (or equivalent) annotation appears on `add()`'s frame card. After `FrameLeave`, `r` in `main()` receives `5` via `SlotWrite`. The chain of custody for the return value is visible end-to-end.

**Acceptance Scenarios**:

1. **Given** a function call returning a value (e.g. `m03_fn_call`'s `add(2, 3)` returning `5`), **When** the cursor reaches the step just after the body's tail expression evaluates, **Then** a return-value indicator appears on the called frame's card showing the value about to be returned.
2. **Given** a function with implicit unit return (e.g. `fn main() {}`'s outer call), **When** the cursor reaches the frame's exit, **Then** the return-value indicator shows `()` (unit) — not omitted entirely.
3. **Given** an unsuccessful (runtime-error-halting) execution, **When** the cursor reaches the `Note { kind: RuntimeError }` event, **Then** no return-value indicator appears for the halted frame — the trace simply stops as before.

---

### User Story 3 — Existing M01/M02/M04 behavior is preserved (Priority: P1)

The maintainer runs the full test suite + manual QA of M04 after M03.1 lands. M01 and M02's snapshot tests pass byte-identically (they don't touch event emission). M03's snapshot tests are updated to reflect the new event stream (fewer `SlotDrop` events, new `ReturnValue` events) but every prior sample still parses, resolves, type-checks, and evaluates without errors. M04's page loads, plays back all 4 samples, and the only visible behavioral difference is the two improvements US1 + US2 introduce.

**Why this priority**: the project has 47 passing tests across four milestones. A protocol revision must not regress any of them. P1 because regressions here invalidate the entire prior cycle.

**Independent Test**: `cargo test` passes the full suite (M01 + M02 + M03 with revised snapshots + lib unit tests including Cursor). M04 manual QA per `specs/005-m04-ui-shell/quickstart.md` SC-008 procedure passes all 10 steps (with the visual improvements from US1 + US2 noted but no breakage).

**Acceptance Scenarios**:

1. **Given** the M03.1 changes have landed on `main`, **When** `cargo test --test m01` and `cargo test --test m02` run, **Then** both exit 0 with no snapshot drift.
2. **Given** the M03 evaluator has been revised, **When** `cargo test --test m03` runs against the updated snapshots, **Then** the suite passes and the snapshot diff (compared to pre-M03.1) shows only: removal of Copy-typed `SlotDrop` events, addition of `ReturnValue` events.
3. **Given** the M04 page is reloaded after M03.1 ships, **When** the maintainer walks the SC-008 procedure, **Then** all 10 steps pass with no crashes, no missing functionality, and visible improvements at the steps US1 + US2 target.

---

### Edge Cases

- **Non-Copy types in future levels**: M03.1 gates `SlotDrop` on Copy-ness. When M07 lands `Box`/`Vec`/`String` (all non-Copy), `SlotDrop` events fire for them — destructor semantics are real, heap memory is freed, the visualization correctly shows the slot's value going away.
- **Frame with no locals** (e.g. `fn empty() {}`): the frame card shows up at `FrameEnter`, no slot rows appear, then the return-value indicator briefly shows `()`, then `FrameLeave` closes the frame.
- **Frame whose body returns via a panic-equivalent runtime error**: the `Note { kind: RuntimeError }` event fires and the trace halts. No `ReturnValue` event is emitted for that frame (the function never returned a value). The frame card stays visible at the error point so the learner can see where things crashed.
- **Recursion**: each recursive call produces its own `FrameEnter` / `ReturnValue` / `FrameLeave` triple. The return-value indicator is per-frame, not global.
- **Trace replay determinism**: the new `ReturnValue` event is deterministic — it always appears between the last body evaluation and the matching `FrameLeave`. Snapshots are byte-stable.
- **Reverse playback (Step Back)**: stepping back from a `ReturnValue` event correctly returns to the pre-return state (body just finished evaluating); stepping back from the post-`ReturnValue` `FrameLeave` returns to the return-value visible state.
- **Trace JSON schema additions**: the new `MemEvent::ReturnValue` variant adds a JSON case to the externally-tagged enum representation. Pre-M03.1 traces are no longer valid (they lack the variant) — `gen_traces` regenerates them as part of the M03.1 build.
- **`FrameEnter.params` removal**: per MILESTONES.md M03.1 in-scope, the redundant `params` field on `FrameEnter` is dropped. M04's renderer currently ignores it; removing it tightens the contract without behavioral impact.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST gate `SlotDrop` event emission in the M03 evaluator on the slot's binding type. Only emit `SlotDrop` when the binding's `Ty` is non-Copy. In L1 (where `Ty ∈ {I32, Bool, Unit}`), no `SlotDrop` events fire — the L1 type lattice is entirely Copy.
- **FR-002**: System MUST add a new `MemEvent::ReturnValue { frame_id, value, span }` variant to the event enum, emitted by the evaluator immediately before the matching `FrameLeave` event. The `value` field carries the function's computed return value (mirroring `FrameLeave.return_value`, which remains in place for the closing event itself).
- **FR-003**: The `MemEvent` enum's "closed from M03" stability rule MUST be relaxed in M03's contract to permit **additive variants in revision milestones** with maintainer consent. M03.1 is the first invocation of this revised rule. Removing or renaming existing variants remains breaking.
- **FR-004**: System MUST remove the redundant `FrameEnter.params` field from the `MemEvent::FrameEnter` variant. The information it carried (per-param slot id, name, initial value) is already conveyed by the subsequent per-param `SlotAlloc` and `SlotWrite` events that fire immediately after `FrameEnter`.
- **FR-005**: System MUST update the M04 `Cursor::state_snapshot` logic to consume the new `ReturnValue` event. The state snapshot at the position immediately after a `ReturnValue` event MUST surface the returned value as a transient annotation attached to the relevant frame card.
- **FR-006**: System MUST regenerate the M03 snapshot tests (`tests/snapshots/emits_*.snap`) and the M04 pre-recorded traces (`web/traces/*.json`) to reflect the new event stream. Old snapshot files MUST NOT remain pinning the pre-M03.1 protocol.
- **FR-007**: M01 and M02 integration tests MUST pass byte-identically after M03.1 lands. Neither milestone touches event emission, so their snapshots MUST NOT change.
- **FR-008**: The M03 evaluator's runtime-error path MUST continue to terminate the event stream with a `Note { kind: RuntimeError }` event. A `ReturnValue` event MUST NOT be emitted for a frame that halted mid-execution.
- **FR-009**: The trace JSON schema (per `specs/005-m04-ui-shell/contracts/m04-api.md`) MUST be extended to include the new `ReturnValue` variant case. The schema's stability rule (closed enum, additive variants) is updated correspondingly.

### Key Entities

- **MemEvent::ReturnValue { frame_id, value, span }**: a new event variant marking the moment between body completion and frame exit. Carries the function's return value (`Value` type, same as `FrameLeave.return_value`) and points its span at the body's tail expression (or the function declaration if the body has no tail expression).
- **`Ty::is_copy()`** (or equivalent classification): an internal helper used by the evaluator to decide whether to emit `SlotDrop` for a given slot. For L1's `Ty ∈ {I32, Bool, Unit}`, all three are Copy and the function returns `true`. M07+'s heap-allocated types will return `false`.
- **`StateSnapshot.return_value` (M04 view addition)**: an optional field on `StateSnapshot` populated when the most recent event was `ReturnValue`. M04's renderer reads it and decorates the corresponding frame card with a transient return-value indicator.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After M03.1 lands, `cargo test --test m03` passes with revised snapshots showing: (a) zero `SlotDrop` events for any L1 sample (the L1 lattice is fully Copy); (b) one `ReturnValue` event per function call in any sample with non-error completion; (c) snapshots remain deterministic across reruns.
- **SC-002**: `cargo test --test m01` and `cargo test --test m02` pass byte-identically — zero snapshot drift in either suite. Verified by `git diff tests/snapshots/m01_*.snap m02_*.snap` returning empty.
- **SC-003**: `cargo test --lib ui::` passes after the Cursor logic is updated to consume the new `ReturnValue` event. At least one new unit test covers state-at-N when the cursor lands on a `ReturnValue` event.
- **SC-004**: For `m03_fn_call`, the new event count is **lower than the pre-M03.1 count** (13) by exactly the number of removed `SlotDrop` events (3 — `a`, `b`, `r`), partially offset by added `ReturnValue` events (2 — one per fn frame). Net: 13 − 3 + 2 = 12 events. Documented in the audit log.
- **SC-005**: M04's bundled WASM bundle size grows by no more than 5% over M04's baseline (78 KB gzipped). The change is additive to the enum and shouldn't bloat the WASM appreciably.
- **SC-006**: Manual QA of M04 by the maintainer (per `specs/005-m04-ui-shell/quickstart.md` SC-008 procedure) reports: (a) US1 acceptance — Copy slots persist until FrameLeave; (b) US2 acceptance — return value visible for ≥ 1 cursor step before frame closes; (c) all 10 SC-008 steps still pass; (d) no regressions in any of the 4 samples.
- **SC-007**: Zero warnings under `RUSTFLAGS="-D warnings" cargo build --release`. Both host and WASM targets clean.
- **SC-008**: The post-implementation audit log documents the relaxed "closed enum" rule for M03 and points to this milestone's spec as the first invocation of the rule.

## Assumptions

- M03 and M04 are closed and on `main`. The M03 `evaluate()` API and `MemEvent` enum are the starting point; M04's `Cursor` + `StateSnapshot` consume them.
- The `Ty::is_copy()` classification is well-defined for L1 (all three variants are Copy) and is a clean inline helper. It will need extension in M07 when non-Copy types arrive — that's not M03.1's concern, but M03.1's implementation MUST leave the door open (a method on `Ty`, not a magic constant).
- The `ReturnValue` event's `span` points at the body's tail expression for non-empty bodies (e.g. `a + b`'s span). For empty bodies (`fn empty() {}`), it points at the function declaration's body block span (the `{}`).
- M04's `Cursor` already pattern-matches every `MemEvent` variant defensively (per the M04 `apply_event` function's `_ =>` arm for forward-compat variants). The new `ReturnValue` variant gets explicit handling; the no-op forward-compat arms continue to ignore other future variants.
- The `FrameEnter.params` field removal is a breaking change to existing snapshot files but the M04 contract's "additive only" rule is updated in this milestone to permit it because it removes information that was already redundant — no consumer relies on it (M04's renderer ignored it from the start).
- The M03.1 milestone is implemented by AI agents under maintainer direction; sizing per the S/M/L rubric — M03.1 is rated **M**.
- The `MemEvent` contract update lives in `specs/004-m03-event-eval/contracts/m03-api.md` (M03's contract document), with M03.1's `research.md` cross-referencing the change.
