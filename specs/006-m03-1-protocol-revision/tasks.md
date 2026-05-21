---

description: "Task list for M03.1 — Protocol revision (Copy-drop + return-value bridge)"
---

# Tasks: M03.1 — Protocol Revision

**Input**: Design documents from `/specs/006-m03-1-protocol-revision/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/m03-1-protocol-delta.md ✓, quickstart.md ✓

**Tests**: M03's existing snapshot suite is re-baselined (FR-006). One new unit test for the Cursor's `ReturnValue` handling (FR-005). M01/M02/M04 are not modified — their tests must stay byte-identical (SC-002).

**Organization**: a focused 3-user-story revision. US1 + US2 are the two visible improvements; US3 is the non-regression guarantee. No new modules or deps.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1/US2/US3 tag, mandatory on user-story phases only
- File paths relative to repo root

## Path Conventions

All work is in-place modifications to existing files. No new modules. See `specs/006-m03-1-protocol-revision/plan.md` Project Structure.

---

## Phase 1: Setup

**Purpose**: pre-flight — confirm starting state.

- [ ] T001 Verify pre-conditions: branch `006-m03-1-protocol-revision` checked out; `cargo test` from `main` passes (47 tests across m01/m02/m03/lib); `web/traces/*.json` exist from M04's last build. No code change in this task — just establishes the baseline.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: policy + classification helper that the user stories build on.

- [ ] T002 [P] Amend M03's contract in `specs/004-m03-event-eval/contracts/m03-api.md`: relax the closed-enum rule to permit additive variants + redundant-field removal in revision milestones with maintainer consent. Use the exact wording from `specs/006-m03-1-protocol-revision/contracts/m03-1-protocol-delta.md` "Closed-enum rule relaxation" section. Cross-reference M03.1 as the first invocation.
- [ ] T003 [P] Add `impl Ty { pub fn is_copy(self) -> bool }` in `src/typeck.rs`. Exhaustive match over the current variants (`I32`, `Bool`, `Unit`) returning `true` for all three. Add a doc-comment explaining the L1-all-Copy rationale and the M07+ forward-compatibility (Rust's exhaustiveness check forces new variants to be classified deliberately). Verify with `cargo build`.

**Checkpoint**: contract amended, `is_copy()` available. M01/M02/M03 tests still pass (changes are additive).

---

## Phase 3: User Story 1 — Copy-typed slots persist until FrameLeave (Priority: P1)

**Goal**: gate the `SlotDrop` event emission in `src/eval.rs::drop_current_scope` on the slot's binding type. For L1 (all Copy), no `SlotDrop` events are emitted; slots stay in their frame card until `FrameLeave`.

**Independent Test**: after this phase, regenerated M03 snapshots show zero `SlotDrop` events for any L1 sample. Verified in Phase 5 / T012.

### Implementation

- [ ] T004 [US1] In `src/eval.rs::drop_current_scope`, before pushing the `MemEvent::SlotDrop` event for each `LocalSlot`, look up the slot's type via `self.lookup_var_ty(local.binding_id).expect("var ty after typeck")` and gate the push on `!ty.is_copy()`. Use the new method from T003. Comment the gate inline ("Copy types have no destructor; their bytes persist on the stack until the frame is reused — no observable Drop event"). At this point M03 snapshot tests will fail (expected — Phase 5's T012 re-baselines).

**Checkpoint**: `cargo test --test m03` fails with snapshot drift for samples containing Copy-typed bindings. That's the desired state; Phase 5 will accept the new snapshots.

---

## Phase 4: User Story 2 — Return value visible between body and frame exit (Priority: P1)

**Goal**: add the `MemEvent::ReturnValue` variant; have the evaluator emit it between body completion and `FrameLeave`; extend `StateSnapshot.pending_return` so M04's renderer can paint a transient annotation. Drop the redundant `FrameEnter.params` field in the same pass.

**Independent Test**: after this phase, the M03 snapshot for `m03_fn_call` (regenerated in T012) contains a `ReturnValue` event right before each `FrameLeave`. The unit test added in T009 verifies `StateSnapshot.pending_return` is populated correctly.

### Implementation

- [ ] T005 [US2] In `src/event.rs`, modify the `MemEvent` enum: (a) **remove** the `params: Vec<(SlotId, String, Value)>` field from `MemEvent::FrameEnter`, keeping only `frame_id`, `fn_name`, and `span`; (b) **add** a new variant `MemEvent::ReturnValue { frame_id: FrameId, value: Value, span: Span }` between `FrameLeave` and the stack-slot variants (or place per the data-model.md layout — keep variant order stable across the file). The variant derives the same `Clone, Debug, PartialEq, Serialize, Deserialize` as the others. Update the in-source smoke test that constructs forward-compat variants if it referenced `FrameEnter.params` (it doesn't, but verify). After this task, `cargo build` should fail in `src/eval.rs` (because call_fn still constructs `FrameEnter.params`) — that's expected; T006 fixes it.
- [ ] T006 [US2] In `src/eval.rs::call_fn`, modify the flow per research R-003: (a) **stop building** the `frame_enter_params: Vec<(SlotId, String, Value)>` (the variable can be removed entirely); (b) emit `MemEvent::FrameEnter { frame_id, fn_name: decl.name.clone(), span: decl.span }` (no `params`); (c) after `eval_block(&decl.body)` returns the body's `Value`, **before** calling `self.drop_current_scope()`, push a new `MemEvent::ReturnValue { frame_id, value: body_value.clone(), span: <span> }`. The span is `decl.body.tail.as_ref().map(|t| t.span()).unwrap_or(decl.body.span)`. The existing `if self.halted { return Value::Unit; }` check after `eval_block` MUST happen **before** the ReturnValue push — halted frames don't emit ReturnValue (FR-008 / VR-4). Order of events for a successful frame is now: `FrameEnter → SlotAlloc+SlotWrite (params) → body events → ReturnValue → (drops, if any non-Copy) → FrameLeave`. Re-verify by reading the full `call_fn` method.
- [ ] T007 [US2] In `src/ui.rs`, extend the public types per `specs/006-m03-1-protocol-revision/data-model.md`: (a) add `pub struct PendingReturnView { pub frame_id: u32, pub value: String }` with derives `Debug, Clone, PartialEq, Serialize, Deserialize`; (b) add the field `pub pending_return: Option<PendingReturnView>` to `StateSnapshot` (place it next to `status` per the data-model.md ordering). Update the existing `StateSnapshot` literal in `Cursor::state_snapshot` to set `pending_return: <as computed below>`.
- [ ] T008 [US2] In `src/ui.rs::Cursor::state_snapshot`, populate `pending_return`: if `self.position == 0`, set to `None`. Otherwise look at `self.trace[self.position - 1]` — if it's `MemEvent::ReturnValue { frame_id, value, .. }`, set `pending_return = Some(PendingReturnView { frame_id: frame_id.0, value: render_value(value) })` (reuse the existing `render_value` helper). Otherwise `None`. Also: extend the `apply_event` function with a no-op arm for `MemEvent::ReturnValue { .. }` (the world state doesn't change — only the snapshot's transient `pending_return` is affected). Also: extend the `event_span` helper to handle the new variant (`MemEvent::ReturnValue { span, .. } => *span`).
- [ ] T009 [US2] In `src/ui.rs`'s `#[cfg(test)] mod tests` block, add a unit test `return_value_populates_pending_return`: build a trace `[FrameEnter(main, 0), ReturnValue(0, Value::Int(5)), FrameLeave(0, Value::Int(5))]`. Step the cursor twice (to position 2 = after ReturnValue). Assert `state.pending_return == Some(PendingReturnView { frame_id: 0, value: "5".into() })`. Step once more (position 3 = after FrameLeave). Assert `state.pending_return == None` (because the most-recent event is now FrameLeave). Use the existing `frame_enter` / `frame_leave` / `span` helpers in the test module; add a small `return_value` helper alongside them.
- [ ] T010 [P] [US2] In `web/index.js::render`, after rendering the frame cards, check `state.pending_return`. If `Some(pr)`, find the matching frame card element (the one whose `frame_id` equals `pr.frame_id` — easiest via a `data-frame-id` attribute set during card creation; add that attribute in the same edit), and append a small annotation element like `<span class="frame-return-value">→ ${pr.value}</span>` to that card's header. Clear the annotation on the next `render` call (which is automatic since `replaceChildren` rebuilds the panel). Add a comment tagging US2 above this code.
- [ ] T011 [P] [US2] In `web/style.css`, add a `.frame-return-value` rule. Suggested styling: same monospace as slot values, distinct color (e.g. `var(--accent)` or a green like `#2a8a3f`), slight font-weight bump, small left margin so it sits cleanly after the frame name. Keep the styling minimal — the final visual settles during M04 QA.

**Checkpoint**: code compiles; lib unit test for `pending_return` passes; M03 snapshot tests still fail (expected — Phase 5 re-baselines).

---

## Phase 5: User Story 3 — M01/M02/M04 behavior preserved (Priority: P1)

**Goal**: re-baseline M03 snapshots to reflect the new event stream, regenerate M04 traces, and confirm M01/M02 are byte-identical.

**Independent Test**: full `cargo test` passes (M01 + M02 + M03 with new snapshots + lib including T009's new test); M04 page loads + plays back samples per the maintainer's manual QA (deferred per the UI QA-split convention).

### Implementation

- [ ] T012 [US3] Re-baseline the M03 snapshot tests: run `INSTA_UPDATE=always cargo test --test m03`. Then visually inspect each `tests/snapshots/emits_*.snap` and verify the event-count diff per the research.md R-008 table: arithmetic 5→5 (−1 SlotDrop +1 ReturnValue), fn_call 13→12, if_then 5→5, if_else 5→5, shadow 8→7, nested_block 8→7, short_circuit 17→14, div_by_zero 2→2 (unchanged — halts before drops/returns). Also verify `FrameEnter` events no longer carry a `params` field anywhere. If any snapshot diverges from the predicted count, investigate before continuing.
- [ ] T013 [US3] Regenerate M04 traces: `cargo run --release --bin gen_traces`. Confirm all 4 `web/traces/m03_*.json` files are produced and contain the new schema (no `params` field on FrameEnter; `ReturnValue` events present for non-error samples). Spot-check `web/traces/m03_fn_call.json` with `jq '.events | length'` — must equal 12 (was 13).
- [ ] T014 [US3] Run M01 + M02 regression: `cargo test --test m01 && cargo test --test m02`. Both MUST exit 0 with no `.snap.new` files appearing in `tests/snapshots/`. If anything drifts, the M03.1 changes accidentally touched shared code — investigate (likely `Span` or `Value` Debug output change).

**Checkpoint**: all 47+ tests pass with the revised M03 snapshots + new lib test. M04 traces match the new protocol shape.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: verify SC-005, SC-007, audit-log close, stage.

- [ ] T015 [P] Verify SC-007 (zero warnings): `RUSTFLAGS="-D warnings" cargo build --release` AND `RUSTFLAGS="-D warnings" cargo test`. Both MUST exit clean. Separately attempt `cargo build --release --target wasm32-unknown-unknown` (without `-D warnings` per the M04 precedent of the `unreachable_pub` allow on wasm-bindgen).
- [ ] T016 [P] Verify SC-005 (bundle size ≤ 5% growth vs M04 baseline): rebuild the WASM with `cargo build --release --target wasm32-unknown-unknown`, then `gzip -kc target/wasm32-unknown-unknown/release/rustviz.wasm | wc -c`. M04's baseline was 79915 bytes (~78 KB) gzipped. New size MUST be ≤ 83910 bytes (~82 KB). If it exceeds the budget, investigate; the new variant + field shouldn't add much.
- [ ] T017 Append post-implementation audit log to `specs/006-m03-1-protocol-revision/checklists/requirements.md` (mirror the M01–M04 pattern). Table: SC-001…SC-008 with PASS/FAIL/DEFERRED. Document the per-sample event-count diff matching R-008. Document the (deferred) M04 manual QA as the maintainer's responsibility.
- [ ] T018 Run final clean verification: `cargo clean && RUSTFLAGS="-D warnings" cargo build --release && RUSTFLAGS="-D warnings" cargo test && cargo build --release --target wasm32-unknown-unknown`. Full suite must pass clean.
- [ ] T019 Stage all changed files: `git add Cargo.toml Cargo.lock src/typeck.rs src/event.rs src/eval.rs src/ui.rs src/lib.rs tests/snapshots/emits_*.snap web/traces/ web/index.js web/style.css specs/004-m03-event-eval/contracts/m03-api.md specs/006-m03-1-protocol-revision/ CLAUDE.md`. Note: `Cargo.toml`/`Cargo.lock` likely unchanged (no new deps) but include in the `git add` for safety. `web/traces/` is gitignored — the `git add` will be a no-op for those files; that's fine. Run `git status` and report. **Do not commit** — maintainer's call. Notably: maintainer's QA pass needs to happen between stage and commit (per the UI QA-split feedback memory).

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: no dependencies. Single pre-flight task.
- **Phase 2 (Foundational)**: T002 and T003 parallel (different files); both prerequisites for the user-story phases.
- **Phase 3 (US1)**: depends on T003. T004 is one task.
- **Phase 4 (US2)**: depends on Phase 2 (uses the contract relaxation). T005 → T006 sequential (event.rs first, then eval.rs that uses it). T007 → T008 → T009 sequential (all in src/ui.rs). T010/T011 [P] (different web files), can run anytime after T007. T010 also depends on the `pending_return` field existing.
- **Phase 5 (US3)**: depends on Phases 3 and 4 (the code changes must land before re-snapshotting). T012 → T013 → T014 (logical order; T013 needs the new evaluator to be working).
- **Phase 6 (Polish)**: depends on Phases 4 and 5 closing. T015 / T016 parallel. T017 → T018 → T019 sequential.

### Story-Level Dependencies

- US1 and US2 can be implemented in either order or in parallel — they touch different lines of `src/eval.rs` and don't conflict. US3 strictly last.

### Parallel Opportunities

- **T002 + T003**: contract doc + Ty::is_copy. Different files. [P] ✓
- **T010 + T011**: web/index.js + web/style.css. Different files. [P] ✓
- **T015 + T016**: warnings + bundle-size audits. Read-only. [P] ✓
- **US1 vs US2**: different lines in `src/eval.rs` but same file. Sequential edits if one agent; parallel-able if two agents coordinate the merge.

---

## Parallel Example: Phase 2 Foundational

```bash
# Both run in parallel (different files):
Task T002: "Amend M03 contract — relax closed-enum rule"
Task T003: "Add Ty::is_copy() in src/typeck.rs"
```

## Parallel Example: Phase 4 web assets

```bash
# After T007 lands the pending_return field:
Task T010: "Render pending_return annotation in web/index.js"
Task T011: "Style .frame-return-value in web/style.css"
```

---

## Implementation Strategy

### MVP First (US1 + US2 together)

1. **Phase 1** (T001): pre-flight.
2. **Phase 2** (T002, T003): contract + `Ty::is_copy()`.
3. **Phase 3** (T004): gate SlotDrop emission. M03 tests fail at this point — expected.
4. **Phase 4** (T005–T011): event protocol changes + Cursor extension + web rendering.
5. **Phase 5** (T012–T014): re-snapshot + regen traces + regression check.
6. **STOP and VALIDATE**: `cargo test` passes; M01/M02 byte-identical; M03 snapshots reflect R-008 expected counts.
7. **Maintainer QA**: hand off to maintainer for the M04 manual verification per `specs/005-m04-ui-shell/quickstart.md` SC-008.

### Incremental Delivery (alternative)

US1 alone is a valid intermediate (just the Copy-drop fix, no return-value bridge). Could ship after Phase 3 + a re-snapshot of M03 if the return-value work needs more thinking. But spec scopes both to one milestone — implementer's judgment, not the maintainer's call.

### Single-Agent Strategy

One AI agent:
1. T001 (no-op pre-flight) → T002 + T003 (parallel writes if possible; sequential in practice).
2. T004 (gate SlotDrop).
3. T005 → T006 → T007 → T008 → T009 → T010 → T011 (sequential reading of src/ui.rs after each touch; web/index.js + web/style.css can be reversed).
4. T012 → T013 → T014.
5. Phase 6: T015 + T016 (read-only), T017 (audit), T018, T019.

---

## Notes

- [P] tasks = different files, no incomplete-task dependencies.
- [Story] tag mandatory on user-story phases.
- **No new production deps.** Existing toolchain (serde, indexmap, wasm-bindgen, etc.) is sufficient.
- **No new files** — pure modifications to existing files + re-baselined snapshots + regenerated traces.
- **M03's contract is amended in lock-step** (T002). The relaxed closed-enum rule is the policy under which M03.1's variant addition is legal.
- **M03 snapshots WILL drift after Phase 3 / Phase 4** — that's expected. T012 re-baselines them. Don't panic at `cargo test --test m03` failures during implementation.
- **M01 and M02 must NOT drift.** If they do, M03.1 touched something it shouldn't have. Investigate before continuing.
- **`Cargo.toml` and `Cargo.lock`** should be unchanged in this milestone (no new deps), but they're in the staging list of T019 as a defensive measure.
- **`CLAUDE.md`** may get an auto-update from `/speckit-plan` (it did in prior milestones). Include it in the stage per the M02 lesson.
- **Maintainer QA between stage and commit** — same pattern as M04. The audit log records "code-side verified; M04 visual QA deferred to maintainer."
- Avoid: putting M07+ (non-Copy heap types) work into M03.1. The roadmap is the contract; M03.1 only gates the SlotDrop emission, it doesn't implement the new types.
