# Feature Specification: M05 — Live Level 1 (edit → run → watch)

**Feature Branch**: `007-live-l1-editing`
**Created**: 2026-05-22
**Status**: Draft
**Input**: User description: "M05"

**Authoritative scope source**: [`MILESTONES.md` › M05 — Live Level 1 (edit → run → watch)](../../MILESTONES.md). The In scope / Out of scope / Entry criteria / Exit criteria / Demo lines in that block are the contract for this feature. This spec elaborates user stories, requirements, and success criteria; it does not redefine scope.

## User Scenarios & Testing *(mandatory)*

M05 is **the project's first publicly demoable artifact**. M04 shipped pre-recorded traces; M05 swaps in a live pipeline that runs M01 → M02 → M03 on whatever the learner types in the editor and pipes the resulting `Vec<MemEvent>` into the existing M04 replay cursor. The dropdown that previously loaded a static JSON trace now loads source code into a writable editor; the trace updates as the learner types.

### User Story 1 — Edit L1 code, see the trace update live (Priority: P1)

A learner types an L1 program into the editor (or selects a sample to start from). After a brief debounce, the M01 → M02 → M03 pipeline runs on the typed source and the resulting event stream is replayed in the stacks panel. Existing M04 + M03.1 visualization carries over: frames open/close, slots persist until reused, return values bridge between callee and caller. The learner uses the existing Play / Step / Rewind controls to walk through their own program.

**Why this priority**: this is the milestone's headline value. Without it, M05 hasn't shipped. P1.

**Independent Test**: select the `Function Call` sample to load source into the editor, edit `add(2, 3)` to `add(10, 20)`, observe the trace re-runs and the stacks panel shows `a = 10`, `b = 20`, `→ 30` instead of the original values. Verified visually by the maintainer per the SC-008 procedure.

**Acceptance Scenarios**:

1. **Given** the editor contains a valid L1 program, **When** the learner stops typing for the debounce window (≤ 500 ms), **Then** the trace re-generates and the stacks panel resets to position 0 of the new trace.
2. **Given** the trace has just regenerated, **When** the learner clicks Play (or steps manually), **Then** the playback walks through events derived from their current source — not the previous version.
3. **Given** the learner selects a sample from the dropdown, **When** the sample loads, **Then** the editor's content is replaced with that sample's source code and the trace regenerates from it.
4. **Given** the learner edits a constant inside an existing sample (e.g. changes `2 + 3` to `40 + 2`), **When** the debounce elapses, **Then** subsequent step-through shows the new computed value (e.g. `→ 42`) in the `ReturnValue` annotation.

---

### User Story 2 — Errors are visible inline with span underlines (Priority: P1)

When the editor's content fails to parse, fails resolution, or fails type-checking, the editor displays a red underline at the error's source span and shows the error message — either as a tooltip on the underline or in the existing status bar. The stacks panel stops accepting Step / Play input (or shows the last successful trace) until the error is fixed.

**Why this priority**: the project is a *teaching* tool. Hiding errors silently or rendering an empty stacks panel without explanation defeats the purpose. P1.

**Independent Test**: type `let x = ;` (incomplete `let` initializer), observe a red underline at the missing-expression span plus an error message. Step / Play buttons either become inactive or do nothing visible. Fix the syntax (e.g. `let x = 1;`), observe the underline disappears and the trace becomes available.

**Acceptance Scenarios**:

1. **Given** a parse error in the editor content, **When** the debounce elapses, **Then** a red span underline appears at the error's source location AND the error message is displayed to the learner.
2. **Given** a resolve error (e.g. `let y = undefined_var;`), **When** the pipeline runs, **Then** the same underline + message UX appears, pointing at the undefined identifier.
3. **Given** a typeck error (e.g. `let z: i32 = true;`), **When** the pipeline runs, **Then** the same UX appears, pointing at the mismatched expression.
4. **Given** the editor content has an error, **When** the learner clicks Step Forward, **Then** the stacks panel does NOT advance (cursor stays put or is reset to position 0 of an empty trace).
5. **Given** the learner has fixed the error, **When** the debounce elapses, **Then** the underline disappears, the trace regenerates from the now-valid source, and Step / Play resume working.

---

### User Story 3 — Reference programs in `tests/samples/m05_*.rs` ship with the milestone (Priority: P2)

A small set of M05-specific reference programs (`tests/samples/m05_*.rs`) ships with the milestone, covering the edit-friendly cases: a minimal `let`, a simple arithmetic snippet, a function call, and an intentionally broken program for error-state demos. These are pre-loaded into the sample dropdown alongside the existing M04 samples; selecting them populates the editor.

**Why this priority**: convenient demo entry points + smoke tests. Not blocking M05's headline behavior but expected per the MILESTONES.md Demo block. P2.

**Independent Test**: open the sample dropdown, observe at least 3 M05-prefixed samples in addition to the M04 ones. Select each in turn — editor content swaps, trace regenerates without error, step-through works.

**Acceptance Scenarios**:

1. **Given** the page is loaded, **When** the learner opens the sample dropdown, **Then** at least 3 M05-prefixed reference programs are visible (in addition to the existing M04 samples).
2. **Given** a learner selects an M05 sample, **When** the editor content updates, **Then** the trace regenerates from that source within the debounce window.
3. **Given** one of the M05 samples is deliberately broken (for error-UX demonstration), **When** the learner selects it, **Then** the editor shows the broken source, the error UX from US2 fires, and the stacks panel reflects the error state.

---

### Edge Cases

- **Empty editor**: when the editor is empty, the pipeline produces an empty / minimal trace (e.g. just `FrameEnter(main) → ReturnValue(main, ()) → FrameLeave(main)` if there's an implicit main, OR a "type some code" hint depending on plan-phase decision). No error displayed for legitimately-empty input.
- **Editor content with only whitespace / comments**: parser should treat as empty body; trace is the empty-main trace.
- **Rapid keystrokes**: debounce coalesces — only the last keystroke before the debounce window's end triggers a re-run. Mid-typing parse errors don't appear constantly; they appear once the user pauses.
- **Switching samples while a previous trace is mid-playback**: the new sample's trace resets the cursor to position 0; the previous playback is abandoned.
- **Stale trace after editor edit, before debounce fires**: the displayed trace is briefly inconsistent with the editor source. Acceptable — the debounce window is short (≤ 500 ms).
- **Editor exhibits the same code as last time** (e.g. user typed and then undid back to same text): re-run is idempotent. State at position 0 is identical to the previous run; the user sees no flicker.
- **Cursor position after re-run**: cursor resets to 0 on every successful re-run. Even if the new trace happens to have ≥ the old position's event count, position is reset for clarity.
- **Runtime error in the new trace** (e.g. division by zero in the just-typed code): the trace ends with a `Note { kind: RuntimeError }`. Playback works up to the halt point; the existing M04 + M03.1 visualization handles this case already.
- **Very long edit history**: the editor is the source of truth; no undo history of traces is kept across edits. Each successful re-run replaces the previous trace.
- **Multiple errors in one pipeline pass**: M01 stops at the first parse error (per CLAUDE.md). For resolve/typeck the first error is also enough. So a single underline per pipeline invocation.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST make the editor writable (the M04 editor was read-only). Users can type, paste, and edit text in the source editor pane.
- **FR-002**: System MUST re-run the M01 → M02 → M03 pipeline on the editor's current content after a debounce window of ≤ 500 ms following the last edit.
- **FR-003**: System MUST replace the M04 Player API's "load pre-recorded trace from URL" entry point with a "compile and run from source string" entry point. The new entry point takes the editor's current source and produces either a fresh `Vec<MemEvent>` (and a new `Cursor` positioned at 0) or an error with a source span.
- **FR-004**: System MUST display parse, resolve, and typeck errors inline in the editor as a red underline at the error's source span, plus a readable message visible to the user (tooltip on the underline or in the status bar — implementer's choice during plan-phase).
- **FR-005**: System MUST disable or no-op the playback controls (Play / Step Forward / Step Back) while the editor's content is in an error state. Rewind remains available so the learner can reset the cursor visually.
- **FR-006**: System MUST keep the existing M04 + M03.1 visualization for ALL events the new live trace produces — frames go grayed on `FrameLeave`, return values bridge, current frame is red, current call site is highlighted, etc. No regressions to the visual contract established in M04 + M03.1.
- **FR-007**: System MUST replace the M04 sample-loading flow: the dropdown now loads source code into the editor (which triggers a fresh re-run), not a JSON trace. The existing M03/M04 samples (`m03_arithmetic`, `m03_fn_call`, `m03_fn_call_twice`, `m03_shadow`, `m03_div_by_zero`) MUST remain selectable.
- **FR-008**: System MUST ship at least 3 new reference programs under `tests/samples/m05_*.rs` (and `web/samples/m05_*.rs`) demonstrating M05's edit-friendly cases. At least one of them MUST be a deliberately broken program demonstrating the error-state UX.
- **FR-009**: System MUST regenerate the cursor at position 0 on every successful pipeline re-run. The previously-displayed cursor position is not preserved across edits.
- **FR-010**: System SHOULD remove or deprecate the pre-recorded trace JSON pipeline (the `gen_traces` binary and `web/traces/*.json` files) since the page no longer consumes them. The `gen_traces` binary may remain for offline / CLI use but the trunk pre-build hook should no longer require it.

### Key Entities

- **Source string**: the editor's current text content. Single source of truth for the pipeline input.
- **Live run result**: either `Ok(Vec<MemEvent>)` or `Err(ErrorInfo)`. `ErrorInfo` carries a `Span` (start/end byte offsets in the source) and a human-readable message. The same `Span` type already used by M01/M02/M03 errors.
- **Editor decoration set**: extends M04's existing CodeMirror decoration layers (event-span yellow, current-call-span red) with a new error-underline layer painted when the pipeline returns `Err`.
- **Sample source file**: `.rs` files under `web/samples/` selectable from the dropdown; loading one writes its content into the editor and triggers a re-run.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After M05 ships, a learner can type a valid L1 program in the editor and observe the corresponding trace update in the stacks panel **within 1 second** of the last keystroke. This includes parse + resolve + typeck + evaluate + JSON serde + cursor reset + DOM re-render.
- **SC-002**: A learner can introduce a parse / resolve / typeck error and see an editor underline appear at the error's source span within the same 1-second window. The underline disappears within 1 second after the error is fixed.
- **SC-003**: Existing M01, M02, M03, lib tests pass byte-identically — M05 is purely additive at the library level. Verified by `cargo test --test m01 / m02 / m03 / --lib` exit 0 with no `.snap.new` files.
- **SC-004**: The M05 reference programs (`tests/samples/m05_*.rs`) cover at least: (a) a minimal `let x = N;` program, (b) a small arithmetic snippet, (c) a function call with parameters, (d) an intentionally broken program for error-state demos. ≥ 4 samples total.
- **SC-005**: WASM bundle size growth ≤ 5% vs M04 baseline (79,915 B gzipped). The new live-pipeline glue is small — adding `parse + resolve + typeck + evaluate` to the WASM entry surface should be < 10 KB given those modules already compile for M01–M03.
- **SC-006**: Zero warnings under `RUSTFLAGS="-D warnings" cargo build --release` AND `cargo test`. Both host and WASM targets clean.
- **SC-007**: The closing M05 commit is **tagged** in git per the MILESTONES.md exit criterion (e.g. `m05-edit-run-watch` or similar). A short screen recording is captured for the project README's first demo asset.
- **SC-008**: Manual test procedure documented in `quickstart.md` and executable by the maintainer. Procedure walks through: (a) typing a fresh program, (b) editing an existing sample, (c) introducing an error and fixing it, (d) verifying no M04 regressions.

## Assumptions

- The existing M04 sample dropdown is repurposed to load source into the editor (not JSON traces). The dropdown UI doesn't need to change visually; just its action wiring.
- The M03 evaluator is fully driveable from a `&str` source — M01's parser already takes a `&str`. There are no hidden cross-pipeline dependencies blocking M05.
- The wasm-bindgen `Player` API can grow a new method (e.g. `Player::set_source(&str)` or a free `compile_and_run(&str)` function) without breaking M04's existing Player methods. M03.1's relaxed-but-additive rule for the M04 contract applies analogously.
- Debounce timing of ≤ 500 ms is sufficient: M01–M03's pipeline runs in well under 100 ms for L1 programs typically ≤ 50 lines. Even with WASM overhead the response is sub-second.
- Error display style (red underline + message location) is finalized during plan-phase. Reasonable defaults exist (CodeMirror has a built-in `lintGutter` or we can use a custom decoration like M04 does for the event-span highlight); the implementer picks one in plan-phase research.
- The Cursor's `pending_return`, `current_call_span`, `current` field, etc. all work identically for live traces because the Cursor is source-agnostic — it consumes any `Vec<MemEvent>` regardless of origin.
- `gen_traces` binary stays in the repo as a CLI-runnable utility for traceability but is removed from trunk's pre-build hook. The decision can also flip in plan-phase if the maintainer prefers to delete it entirely.
- The first publicly demoable artifact framing means the M05 commit message + the README's screen recording matter more than they did for M01–M04. Worth a polish pass that earlier milestones didn't need.
