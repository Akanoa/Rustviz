# Feature Specification: M04 — UI Shell + Replay Cursor

**Feature Branch**: `005-m04-ui-shell`
**Created**: 2026-05-21
**Status**: Draft
**Input**: User description: "M04"

**Authoritative scope source**: [`MILESTONES.md` › M04 — UI shell + replay cursor](../../MILESTONES.md). The In scope / Out of scope / Entry criteria / Exit criteria / Demo lines in that block are the contract for this feature. This spec elaborates user stories, requirements, and success criteria; it does not redefine scope.

## User Scenarios & Testing *(mandatory)*

M04 is the **first end-user-facing milestone**. Up to now everything has been Rust library code with snapshot tests; M04 produces a browser page that a beginner Rust learner can open and interact with. The audience is two-fold: (a) the eventual learner who steps through a sample program to build intuition about let-bindings, function calls, and scope, and (b) the maintainer who'll extend this shell in M05 (live editing), M06 (borrow arrows), M07 (heap panel), M08 (multi-thread stacks). M04 plays back a **pre-recorded** trace; live interpretation from editor input is M05.

### User Story 1 — Watch a sample Rust program execute step-by-step (Priority: P1)

A Rust learner opens the rustviz page in a browser. They see a sample Level 1 program in a syntax-highlighted code editor on the left, an (initially empty) stacks panel in the middle, and a placeholder heap panel on the right. They click "Play". The cursor advances automatically through the pre-recorded event trace at a comfortable reading speed. As each event fires:

- The editor highlights the source span the current event was triggered by.
- The stacks panel reflects the current memory state: function-call frame cards stack vertically as `fn` calls happen; each frame's slots appear (with name, type, and value) as `SlotAlloc`/`SlotWrite` events fire; slots disappear as `SlotDrop` events fire; frame cards disappear as `FrameLeave` events fire.

By the time the trace ends, the learner has watched the full program execute, with every state transition visible in both panels. Clicking "Play" again replays from the start.

**Why this priority**: M04's whole purpose is to make the event stream *visible*. If a learner can't watch a sample program run end-to-end, the project has failed to demo. Every later milestone (M05 live editing, M06 borrows, M07 heap, M08 threads) extends this same shell — so the shell itself is the foundation that has to land first.

**Independent Test**: serve the M04 build (e.g. `trunk serve --open`), select a sample (e.g. the M03 `arithmetic` trace), click Play. The cursor advances on its own; the editor highlight moves through the source; the stacks panel populates `main → x = 5 → drop x → main returns`; the page does not hang, crash, or display the same step twice. Verified by manual interaction + a short screen-recording captured for the audit log.

**Acceptance Scenarios**:

1. **Given** the M04 page is loaded with a pre-recorded L1 trace, **When** the user clicks "Play", **Then** the cursor automatically advances through the trace at a configurable rate (default ≈ 1 event per 400 ms) until reaching the last event, then stops or loops (decision: stops). The editor highlight and stacks panel update visibly at each step.
2. **Given** a trace with a function call (`main` calls `add`), **When** the cursor reaches the `FrameEnter(add)` event, **Then** a new frame card for `add` appears stacked above `main`'s frame card. **When** the cursor reaches `FrameLeave(add)`, **Then** `add`'s frame card disappears and `main`'s card remains.
3. **Given** a trace with `SlotAlloc(x)` followed by `SlotWrite(x = 5)`, **When** the cursor advances over these two events, **Then** a slot row for `x` appears in the current frame card (after `SlotAlloc`), and its value updates to `5` (after `SlotWrite`).
4. **Given** the cursor is mid-trace and the user clicks anywhere else on the page (e.g. a sample selector), **Then** playback pauses, the user's action takes effect, and the cursor stays at its current step.
5. **Given** a trace that ends with a `Note { kind: RuntimeError, ... }` event, **When** the cursor reaches that event, **Then** the note's message is visible in a clearly-marked area of the UI (e.g. a status bar), and playback stops.

---

### User Story 2 — Step manually through the trace (Priority: P1)

A learner pauses playback and wants to inspect a specific transition closely. They use Step Forward / Step Back / Rewind controls to navigate manually. The two panels reflect the cursor's position deterministically at every step — stepping back to step N produces exactly the same visual state as stepping forward to step N from the start.

**Why this priority**: passive autoplay is useful but interactive stepping is where the visualizer earns its keep — a learner can pause on `SlotMove` (M07+) or a dangling-borrow `Note` (M06+) and study the transition. M04's cursor must support reverse navigation cleanly so later milestones inherit it.

**Independent Test**: load a trace, click Step Forward 5 times, observe the panels at step 5. Click Rewind, click Step Forward 5 times again. The visual state must match the first run.

**Acceptance Scenarios**:

1. **Given** the cursor is at step 0 (initial empty state), **When** the user clicks Step Forward, **Then** the cursor advances by exactly one event and the panels update accordingly.
2. **Given** the cursor is at step N > 0, **When** the user clicks Step Back, **Then** the cursor moves to step N-1 and the panels reflect the state at that earlier step — including slots that have been re-allocated, frame cards that re-appear, and editor highlight that moves back to the earlier event's span.
3. **Given** the cursor is mid-trace, **When** the user clicks Rewind, **Then** the cursor returns to step 0; the panels reset to their initial empty state.
4. **Given** the cursor is at the last event, **When** the user clicks Step Forward, **Then** the cursor does not advance (no-op or visibly-disabled control).
5. **Given** the cursor is at step 0, **When** the user clicks Step Back, **Then** the cursor does not move (no-op or visibly-disabled control).

---

### User Story 3 — Choose from multiple pre-recorded sample programs (Priority: P2)

The page exposes a dropdown (or equivalent selector) listing the available sample programs. Selecting a different sample loads its source into the editor and its pre-recorded trace into the cursor, ready to play from step 0.

**Why this priority**: a single demo is the minimum; a small library of demos is what makes the page genuinely useful for self-directed learning. P2 because US1 + US2 work with one hardcoded sample as MVP; the selector is the polish that lets the maintainer add new samples without touching code.

**Independent Test**: open the page (default sample), select a different sample from the dropdown. The editor's source code changes; the stacks panel clears; the cursor resets to step 0. Click Play and confirm the new sample plays through correctly.

**Acceptance Scenarios**:

1. **Given** the page is loaded with one sample, **When** the user selects a different sample from the selector, **Then** the editor source is replaced; the stacks panel resets to empty; the cursor returns to step 0.
2. **Given** at least 3 pre-recorded samples are available (chosen from M03's working samples — `arithmetic`, `fn_call`, `if_then`, `shadow`, `nested_block`, and the `div_by_zero` runtime-error case), **When** the user cycles through them, **Then** each plays correctly without page reload.

---

### Edge Cases

- **Runtime-error trace** (e.g. `div_by_zero`): playback stops at the `Note { kind: RuntimeError }` event. The note's message is displayed prominently. The user can still step back to inspect pre-error state. Step Forward from the note is a no-op.
- **Empty trace** (no events): the page loads with empty panels; controls work but stepping doesn't advance. Useful for verifying the UI handles edge cases gracefully.
- **Trace ends without `FrameLeave`** (in theory possible if the M03 evaluator halted): the last frame card stays in the stacks panel; the user can see it was mid-execution when something went wrong.
- **Multi-frame trace with deep stacks** (e.g. recursion): the stacks panel grows vertically; if it exceeds the viewport, it scrolls. M04 doesn't need to be elegant about deep recursion, just functional.
- **Browser resize / reflow**: the page is responsive enough that it doesn't break at common desktop widths (≥ 1024px). Mobile is not a goal for M04.
- **No JavaScript**: M04 requires JS to be enabled (the WASM module needs a host). A graceful fallback message ("rustviz requires JavaScript") is shown if JS is disabled.
- **Auto-play rate**: not user-configurable in M04 (a single fixed default). User-configurable rate can come later.
- **Persistence**: cursor position and sample selection are NOT persisted across page reloads. Reload returns to default sample at step 0. Persistence is out of scope.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST deliver a browser-loadable page (HTML + WASM + supporting assets) that renders three regions per CLAUDE.md › The three panels: editor (left), stacks (middle), heap (right; reserved but unpopulated for L1). A toolbar at the bottom holds the cursor controls and a step indicator.
- **FR-002**: The editor region MUST display the source code of the currently-selected sample with syntax highlighting appropriate for Rust. At any cursor position N where the next event has a `Span`, the editor MUST highlight that span (e.g. via underline, background color, or equivalent visual decoration).
- **FR-003**: The stacks region MUST render the runtime memory state at the current cursor position N — namely the active function-call frames (oldest at bottom, innermost at top) and within each frame, the active stack slots in declaration order. Each slot row MUST show the binding name, type, and current value. A slot that has been allocated but not yet written shows a placeholder value (e.g. `?`).
- **FR-004**: The heap region MUST exist as a reserved area in the page layout with a clear "Heap (Level 3+)" or equivalent placeholder. It MUST NOT contain interactive elements in M04.
- **FR-005**: A toolbar control MUST provide at minimum five actions: Play (auto-advance), Pause (stop auto-advance), Step Forward (cursor + 1), Step Back (cursor − 1), Rewind (cursor = 0). The toolbar MUST also display the current step number and total step count (e.g. `step 7 / 13`).
- **FR-006**: Auto-advance (Play) MUST run at a fixed default rate of approximately 1 event per 300–500 ms. Reaching the last event auto-pauses; clicking Play again from the last position is a no-op (or restarts — implementation choice, but no infinite-loop behavior).
- **FR-007**: The cursor's state-at-step-N MUST be deterministic. Stepping forward to N then backward to N-1 then forward to N MUST produce the same visual state as a fresh rewind + step-forward N times.
- **FR-008**: The page MUST ship with at least 3 pre-recorded sample programs (chosen from M03's working samples — `m03_arithmetic`, `m03_fn_call`, `m03_if_then`, `m03_shadow`, `m03_nested_block`, `m03_div_by_zero`). The user selects between them via a UI control.
- **FR-009**: A pre-recorded trace MUST consist of (a) the source code text of the sample and (b) the `Vec<MemEvent>` produced by running the M01 → M02 → M03 pipeline on it. Both are bundled with the page as static assets; the M04 page does NOT re-run the pipeline at load time (live re-runs are M05).
- **FR-010**: A `Note { kind: NoteKind::RuntimeError, ... }` event in the trace MUST cause playback to stop on reaching that event and MUST display the note's message in a visible area (e.g. status bar). Step Back from the runtime error works; Step Forward from it is a no-op.
- **FR-011**: The page MUST function in current versions of major desktop browsers (Chromium, Firefox, Safari) at viewport widths ≥ 1024 px. Mobile, IE, and screen-reader optimization are out of scope.
- **FR-012**: The page MUST require JavaScript to be enabled. If JS is disabled, a fallback message MUST be visible.
- **FR-013**: The build MUST produce a single command (e.g. `trunk serve --open` or equivalent) that starts a local web server, opens the browser, and serves the page. The exact command is documented in the `quickstart.md` for M04.

### Key Entities

- **Sample**: a (source-code, event-trace) pair. The source is a `.rs` text file; the trace is the serialized `Vec<MemEvent>` for that source. Samples are bundled as static assets.
- **Cursor**: an integer step index `0 ≤ N ≤ trace.len()`. State at step `N` = effect of replaying events `[0..N)` from the empty initial state.
- **Frame card**: visual representation of one active function-call frame. Contains the function name and the active slots, in declaration order.
- **Slot row**: visual representation of one active stack slot. Shows binding name, type label (`i32`, `bool`, `()`), and value (or placeholder if allocated-but-not-written).
- **Status note**: textual feedback area showing the most recent `Note` event's message — most importantly, runtime errors.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A new contributor or beginner Rust learner can clone the repo, run the documented serve command, and reach an interactive page where they can click Play and watch a sample execute, in under 5 minutes from clone to first Play click (assuming Rust toolchain installed).
- **SC-002**: At least 3 of the 6 candidate M03 samples (`arithmetic`, `fn_call`, `if_then`, `shadow`, `nested_block`, `div_by_zero`) ship as user-selectable pre-recorded traces in M04. All shipped samples play end-to-end without visual glitches.
- **SC-003**: Cursor determinism: rewinding to step 0 and stepping forward N times produces the same visual state as stepping back to step N from a later position. Verified by manual visual inspection at multiple step positions.
- **SC-004**: Auto-play visibly advances the cursor — a viewer watching for 5 seconds sees the cursor advance through at least 8 events at the default rate.
- **SC-005**: The page loads to first-interactive state within 3 seconds on a typical broadband connection (≤ 50 Mbps), with a WASM bundle size under 2 MB (gzipped).
- **SC-006**: M01, M02, and M03 tests still pass — `cargo test --test m01 && cargo test --test m02 && cargo test --test m03` exit 0. The library crate's public API is unchanged or only additively extended.
- **SC-007**: Zero warnings under `RUSTFLAGS="-D warnings" cargo build --release` for the library crate; the WASM build target is permitted to use additional dev tooling but must compile clean (or with documented exceptions in the audit log).
- **SC-008**: A short manual-test procedure (≤ 10 steps) is documented in `quickstart.md` covering: launching the server, selecting a sample, clicking Play, stepping back, switching samples. Future contributors can follow this procedure to verify the UI didn't regress.

## Assumptions

- M01, M02, and M03 are closed and on `main`. The M03 `evaluate()` API and the `MemEvent` enum are the input vocabulary for M04.
- The M04 deliverable is a single browser page (one HTML route). Multi-page navigation, accounts, persistence, history — all out of scope.
- The source editor offers syntax highlighting but is **read-only** in M04. Editing the source to live-re-run is M05.
- Pre-recorded traces are generated at build time (e.g. by a build script that runs the M03 pipeline on each sample). The page loads them as static assets — JSON or equivalent. The choice of trace serialization format is a plan-phase decision; serde + serde_json is a reasonable default per the "deps when needed" project preference.
- The editor framework choice (Monaco vs CodeMirror vs alternative) is a plan-phase decision. The spec only requires "a syntax-highlighting code editor with span-decoration support".
- The UI framework choice (vanilla JS, a JS framework, a Rust→DOM framework like Yew, etc.) is a plan-phase decision. The spec only requires the behavioral outcomes.
- Testing strategy: unit tests in Rust for the cursor's state-at-step-N logic (deterministic, snapshot-able); manual visual verification per the SC-008 procedure for the UI itself. Automated end-to-end browser tests (Playwright / WebDriver / etc.) are out of scope for M04; future milestones may add them.
- Performance and bundle size targets (SC-005) assume a modern desktop browser on broadband. Embedded / low-power / slow-connection scenarios are out of scope.
- The `MemEvent` payload variants M04 does NOT visualize (HeapAlloc, BorrowShared, ThreadSpawn, etc.) are simply ignored by the UI — they won't appear in L1 traces but the UI should not crash if it encounters one (e.g. from a future-extended sample).
- Implementation is by AI agents under maintainer direction. Sizing per the S/M/L rubric — M04 is rated L.
