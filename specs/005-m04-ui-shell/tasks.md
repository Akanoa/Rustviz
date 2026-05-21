---

description: "Task list for M04 — UI shell + replay cursor"
---

# Tasks: M04 — UI Shell + Replay Cursor

**Input**: Design documents from `/specs/005-m04-ui-shell/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/m04-api.md ✓, quickstart.md ✓

**Tests**: Cursor logic is unit-tested in Rust (state-at-N determinism — SC-003). UI rendering is NOT auto-tested; a manual procedure is documented in `quickstart.md` SC-008. M01/M02/M03 integration tests must stay green (SC-006).

**Toolchain caveats**: M04 needs `rustup target add wasm32-unknown-unknown` and `cargo install trunk` (one-time). If these aren't installed when running cargo/trunk commands, the AI implementer installs the target but defers `trunk install` to the maintainer if it isn't already present. **Manual visual verification of the browser page is the maintainer's responsibility** (AI implementer can build the WASM + start the server but can't see the rendered page).

**Organization**: tasks grouped by user story. US1 is by far the heaviest (covers Cursor + WASM bindings + gen_traces + web assets); US2 and US3 verify behaviors built into US1's wiring.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1/US2/US3 tag, mandatory on user-story phases only
- File paths relative to repo root

## Path Conventions

Single Rust crate with dual `crate-type = ["cdylib", "rlib"]`. New Rust code under `src/ui.rs` + `src/bin/gen_traces.rs`. New web assets under `web/`. New build config: `Trunk.toml`.

---

## Phase 1: Setup

**Purpose**: register new crate type + new deps + new bin target in `Cargo.toml`; create `Trunk.toml`; scaffold empty placeholder files; update `.gitignore`.

- [X] T001 Edit `Cargo.toml`: (a) add `[lib] crate-type = ["cdylib", "rlib"]` block; (b) under `[dependencies]` add `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`, `wasm-bindgen = "0.2"`, `js-sys = "0.3"`, `console_error_panic_hook = "0.1"` — keeping existing `indexmap = "2"`; (c) add a new `[[bin]] name = "gen_traces" path = "src/bin/gen_traces.rs"` block; (d) keep all existing `[[test]]` entries (m01, m02, m03) and the `[dev-dependencies] insta = "1"`.
- [X] T002 Create `Trunk.toml` at the repo root with: `build` block setting `target = "web/index.html"` and `dist = "dist"`; `serve` block setting `addresses = ["127.0.0.1"]` and `port = 8080`; `[[hooks]]` block with `stage = "pre_build"`, `command = "cargo"`, `command_arguments = ["run", "--release", "--bin", "gen_traces"]`. Edit `.gitignore` to append two lines: `web/traces/` and `/dist`.
- [X] T003 Create the directory + file skeleton: `mkdir -p web/samples web/traces src/bin`. Create empty placeholder files with one-line `//!` or comment headers: `src/ui.rs`, `src/bin/gen_traces.rs`, `web/index.html`, `web/index.js`, `web/style.css`. The empty placeholders must let `cargo build` succeed (or fail only with a missing-modules error that T009 fixes).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: add the additive `serde` derives to M01-M03 types; define M04's public view types; wire `lib.rs`. Every user-story phase needs these.

**⚠️ CRITICAL**: no user-story phase begins until Phase 2 closes (`cargo build` succeeds + M01/M02/M03 tests still pass with derives in place).

- [X] T004 [P] In `src/event.rs`, add `#[derive(serde::Serialize, serde::Deserialize)]` to every public type per `specs/005-m04-ui-shell/data-model.md` "Existing types: additive `serde` derives" table — `SlotId`, `FrameId`, `HeapAddr`, `BorrowId`, `Pointee`, `Value`, `NoteKind`, `MemEvent`. Combine with existing derives in a single `#[derive(...)]` per type. Do NOT change anything else — these are purely additive derives.
- [X] T005 [P] In `src/typeck.rs`, add `#[derive(serde::Serialize, serde::Deserialize)]` to `Ty`. Same combine-with-existing pattern. Do NOT touch `FnSig`, `BindingType`, or `TypeMap` — they aren't reached from `MemEvent` so no derives needed.
- [X] T006 [P] In `src/parse/span.rs`, add `#[derive(serde::Serialize, serde::Deserialize)]` to `Span` and `FileId`. Same pattern.
- [X] T007 Run M01 + M02 + M03 regression: `cargo test --test m01 && cargo test --test m02 && cargo test --test m03 && cargo test --lib`. All must exit 0 with no `.snap.new` files (Debug output is unaffected by serde derives, so snapshots stay byte-identical). If any test fails or any snapshot drifts, the derives leaked into Debug somehow — investigate.
- [X] T008 In `src/ui.rs`, define M04's public view types per `specs/005-m04-ui-shell/data-model.md`: `pub struct Cursor { pub trace: Vec<MemEvent>, pub position: usize }` (derives `Debug, Clone`); `pub struct StateSnapshot { pub frames: Vec<FrameCardView>, pub editor_highlight: Option<Span>, pub status: Option<StatusView>, pub position: usize, pub total: usize }`; `pub struct FrameCardView { pub frame_id: u32, pub fn_name: String, pub slots: Vec<SlotRowView> }`; `pub struct SlotRowView { pub slot_id: u32, pub name: String, pub ty: String, pub value: Option<String> }`; `pub struct StatusView { pub kind: String, pub message: String }`. All view types derive `Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize`. Add `impl Cursor` with method stubs `new`, `step_forward`, `step_back`, `rewind`, `state_snapshot` all returning `unimplemented!("T010 implements this")`. Imports from `crate::event::MemEvent`, `crate::parse::span::Span`. NO wasm-bindgen code yet (T012 adds it).
- [X] T009 Edit `src/lib.rs`: add `pub mod ui;` (sorted with other mod declarations) and `pub use ui::{Cursor, FrameCardView, SlotRowView, StateSnapshot, StatusView};` (sorted with other re-exports). Run `cargo build` — must succeed.

**Checkpoint**: serde derives in place, M01-M03 untouched-functionally, M04 public type surface compiles, Cursor is a stub.

---

## Phase 3: User Story 1 — Watch a sample program execute (Priority: P1) 🎯 MVP

**Goal**: a Rust learner opens the page → clicks Play → watches a sample L1 program execute step-by-step with editor highlight + stacks panel updating. Cursor state is deterministic; runtime errors appear as status messages.

**Independent Test** (per spec): serve the M04 build, select a sample, click Play. Cursor advances; editor highlight moves through the source; stacks panel populates and tears down correctly; runtime-error trace stops with a visible status message. Verified manually by the maintainer per `quickstart.md` SC-008 procedure.

### Implementation

- [X] T010 [US1] Implement `Cursor` in `src/ui.rs` per `specs/005-m04-ui-shell/research.md` R-007. Constructor `Cursor::new(trace: Vec<MemEvent>) -> Self` initializes `position: 0`. Methods: `step_forward(&mut self)` advances position by 1 capped at `trace.len()`; `step_back(&mut self)` saturating-subs 1; `rewind(&mut self)` sets position to 0. The core `state_snapshot(&self, source: &str) -> StateSnapshot` method replays events `[0..self.position)` over an internal `World { frames: Vec<FrameInProgress> }` (private struct; each `FrameInProgress { frame_id: u32, fn_name: String, slots: Vec<LiveSlot> }`; `LiveSlot { slot_id: u32, name: String, ty: String, value: Option<String> }`). Event handlers: `FrameEnter` pushes a new frame (with empty slots — params arrive via subsequent `SlotAlloc`s); `FrameLeave` pops a frame; `SlotAlloc` appends `LiveSlot { ..., value: None }` to current frame; `SlotWrite` updates the matching slot's value (find by `slot_id`); `SlotDrop` removes the matching slot from the current frame; `Note { kind: RuntimeError, .. }` sets a pending `status: StatusView { kind: "error", .. }`; `Note { kind: Info, .. }` sets `status: StatusView { kind: "info", .. }`. All other variants (Heap*, Borrow*, Lock*, Arc*, Thread*, SlotMove) are no-ops in M04 (defensive — they shouldn't appear in L1 traces but the impl is forward-compat). At the end of the replay, populate the StateSnapshot: `frames` = world.frames mapped to `FrameCardView`; `editor_highlight` = the span of `trace[position - 1]` if position > 0 (extract via a private `event_span` helper that matches every variant); `status` = the pending status (cleared if no Note event at the last applied step); `position` = self.position; `total` = self.trace.len(). The `Value`-to-`String` rendering: `Value::Int(i) → format!("{i}")`, `Value::Bool(b) → format!("{b}")`, `Value::Unit → "()".to_owned()`. The `Ty`-to-`String` rendering: `Ty::I32 → "i32"`, `Ty::Bool → "bool"`, `Ty::Unit → "()"`.
- [X] T011 [US1] In `src/ui.rs`'s `#[cfg(test)] mod tests` block, add table-driven unit tests for `Cursor` covering acceptance scenarios and SC-003 determinism: (a) `cursor_at_zero_is_empty` — `Cursor::new(vec![]).state_snapshot("")` returns `StateSnapshot { frames: vec![], editor_highlight: None, status: None, position: 0, total: 0 }`. (b) `frame_enter_pushes_frame` — single-event trace `[FrameEnter(main, frame_id=0)]`, after step_forward, the snapshot has 1 frame named "main". (c) `slot_alloc_then_write_then_drop` — sequence of 4 events on a single slot; verify the slot appears (value None), then has Some(value), then disappears. (d) `step_back_undoes_step_forward` — for SC-003: build a 5-event trace, step forward 3 times, capture snapshot, step back once to 2, capture, step forward back to 3, assert the snapshots at position 3 match byte-for-byte. (e) `rewind_zeros_position` — after stepping forward several times, rewind returns to position 0 + empty state. (f) `step_past_end_is_noop` — at trace.len(), step_forward leaves position unchanged. (g) `step_back_from_zero_is_noop`. (h) `runtime_error_note_surfaces_in_status` — trace ending in `Note { kind: RuntimeError, message: "div by zero" }`, after stepping to that event, snapshot.status is `Some(StatusView { kind: "error", message: "div by zero" })`. Use a `dummy_span()` helper to build events without depending on the M01 pipeline.
- [X] T012 [US1] Add wasm-bindgen `Player` exports at the bottom of `src/ui.rs`, gated by `#[cfg(target_arch = "wasm32")]`. Public type `pub struct Player { cursor: Cursor, source: String }` with `#[wasm_bindgen]` attr. Methods per `specs/005-m04-ui-shell/contracts/m04-api.md`: `#[wasm_bindgen(constructor)] new(trace_json: &str) -> Result<Player, JsValue>` (parses `TraceFile { source: String, events: Vec<MemEvent> }` via serde_json::from_str; on error maps to `JsValue::from_str(&e.to_string())`); `state(&self) -> String` (serializes `self.cursor.state_snapshot(&self.source)` via `serde_json::to_string` — `.unwrap()` is acceptable since the types are Serialize); `source(&self) -> String` (clone); `step_forward(&mut self) -> String` (delegates to cursor + returns state); `step_back / rewind` similar; `position(&self) -> usize`; `total(&self) -> usize`. Also add a `#[wasm_bindgen(start)] pub fn start_wasm()` that calls `console_error_panic_hook::set_once()` for better browser panic messages. The `TraceFile` helper struct (`#[derive(Deserialize)] struct TraceFile { source: String, events: Vec<MemEvent> }`) is private to the module.
- [X] T013 [US1] Implement `src/bin/gen_traces.rs`: a `fn main()` that walks the hardcoded sample list `["m03_arithmetic", "m03_fn_call", "m03_shadow", "m03_div_by_zero"]`. For each sample: read `web/samples/<name>.rs` (via `std::fs::read_to_string`); create a `SourceMap`, add the source; run `parse → resolve → typeck → evaluate` chaining `?`s; on success, build a serde_json `Value` of shape `{ "source": <src>, "events": <events> }` and write to `web/traces/<name>.json`. On any error in the pipeline for a sample, print `eprintln!("sample {name} failed: {error}")` and accumulate a failure counter; at end, if any failed, `std::process::exit(1)`. Print progress per sample (e.g. `"writing web/traces/m03_arithmetic.json (events: 13)"`). Imports: `use rustviz::{parse, resolve, typeck, evaluate, SourceMap};`.
- [X] T014 [P] [US1] Copy the four sample sources from `tests/samples/` to `web/samples/`: `cp tests/samples/m03_arithmetic.rs web/samples/`, `cp tests/samples/m03_fn_call.rs web/samples/`, `cp tests/samples/m03_shadow.rs web/samples/`, `cp tests/samples/m03_div_by_zero.rs web/samples/`. Byte-identical copies — these become the canonical M04 demo programs.
- [X] T015 [US1] Run `cargo run --release --bin gen_traces`. Must exit 0. Verify each of the 4 `web/traces/m03_*.json` files exists and is valid JSON (e.g. spot-check with `head -c 200 web/traces/m03_arithmetic.json` to confirm it looks like the schema in `contracts/m04-api.md`). The trace's event count should match M03's corresponding `.snap` snapshot.
- [X] T016 [US1] Write `web/index.html`. HTML5 skeleton. `<head>` has `<title>rustviz</title>`, `<meta charset="utf-8">`, `<meta name="viewport" content="width=1024">`, `<link data-trunk rel="css" href="style.css">`. `<body>` contains a top bar (`<header>` with title + sample selector dropdown with options for the 4 samples + friendly labels), main flex container (`<main>` containing three sibling `<section>`s with ids `editor`, `stacks`, `heap`), a `<div id="status"></div>` status area, and a toolbar (`<footer>` with five buttons: Rewind / Step Back / Play / Step Forward + a `<span id="step-indicator">0 / 0</span>`). The Play button uses an inline state attribute (e.g. `data-state="paused"`) toggled by index.js. Include `<noscript>rustviz requires JavaScript to be enabled.</noscript>` before `</body>` (FR-012). Include `<link data-trunk rel="rust" data-bin="rustviz" data-type="main" data-cargo-features="" />` — Trunk's directive to build the lib's WASM and inject the JS glue. Include `<script type="module" src="index.js"></script>` (NOT `data-trunk`, since we want it loaded as-is without Trunk processing).
- [X] T017 [US1] Write `web/style.css`. Minimal layout: `body { margin: 0; font-family: ui-sans-serif, system-ui, sans-serif; }`. `header` is a horizontal bar with title on the left and sample selector on the right. `main` is `display: flex; height: calc(100vh - <header + footer + status>); }` with three `section`s flex-1 each, separated by `border-right`. `#editor` hosts CodeMirror — set `height: 100%; overflow: auto`. `#stacks` and `#heap` are scrollable. `#heap` shows a placeholder `"Heap (Level 3+)"` text in a muted color (e.g. via `::after` content or a `<p>` placeholder). `.frame-card` has a border, padding, name in bold. `.slot-row` is a 3-col grid (name | type | value). The `.slot-value-pending` class shows `?` instead of value. `#status` is fixed-height or hidden when empty; `.status-error` has red text; `.status-info` is muted. Toolbar buttons styled as plain HTML buttons with min-width and consistent spacing. The active-decoration class for CodeMirror is named `.cm-current-span` and applies a yellow background.
- [X] T018 [US1] Write `web/index.js`. Structure: (a) ES module imports from esm.sh — `EditorView, basicSetup` from `https://esm.sh/codemirror@6.0.1`, `rust` from `https://esm.sh/@codemirror/lang-rust@6.0.1`, `Decoration, EditorView as ViewEV` (alias to avoid clash; or just use one) + `keymap` from `https://esm.sh/@codemirror/view@6`, `StateField, StateEffect` from `https://esm.sh/@codemirror/state@6`. (b) `import init, { Player } from "/rustviz.js"` (trunk emits a JS glue file at the root). (c) `const SAMPLES = [{ id: "m03_arithmetic", label: "Arithmetic" }, { id: "m03_fn_call", label: "Function Call" }, { id: "m03_shadow", label: "Shadowing" }, { id: "m03_div_by_zero", label: "Division by Zero" }]`. (d) Define a `highlightSpan` StateEffect + StateField that paints a `Decoration.mark({ class: "cm-current-span" })` at the given `from..to` byte range (UTF-8 = UTF-16 for ASCII L1 samples per research R-014). (e) `async function main()`: await `init()`; populate the sample selector from `SAMPLES`; instantiate the CodeMirror EditorView in `#editor` (read-only — set `EditorState.readOnly.of(true)`); load the default sample via `loadSample("m03_arithmetic")`. (f) `async function loadSample(id)`: pause if playing; fetch `/traces/${id}.json` as text; `player = new Player(jsonText)`; replace editor doc content with `player.source()`; `render(JSON.parse(player.state()))`. (g) `function render(state)`: clear `#stacks` (no events from frames — empty state); for each `frame` in `state.frames`, build a `<div class="frame-card">` with the fn name and a `<div class="slot-row">` per slot; append to `#stacks`. Update `#step-indicator` text to `${state.position} / ${state.total}`. Dispatch the `highlightSpan` effect with `state.editor_highlight` (or clear if null). Show / hide / class-toggle `#status` based on `state.status`. (h) Button event handlers: Rewind → `player.rewind()` + render; Step Back → `player.step_back()` + render; Step Forward → `player.step_forward()` + render; Play / Pause → toggle a `setInterval(() => { const newState = JSON.parse(player.step_forward()); render(newState); if (newState.position >= newState.total) { stopPlay(); } }, 400)`. (i) Sample selector `change` handler → `loadSample(selectedId)`. (j) Call `main().catch(err => { document.body.textContent = "Failed to start: " + err.toString(); })`.
- [X] T019 [US1] Build the WASM target to verify the bindings compile: `rustup target add wasm32-unknown-unknown` (no-op if already installed), then `cargo build --release --target wasm32-unknown-unknown`. Must exit 0. Output `.wasm` file under `target/wasm32-unknown-unknown/release/rustviz.wasm` should be present and < 1 MB uncompressed (sanity check; SC-005 measures the gzipped trunk output).

**Checkpoint**: WASM compiles, Cursor unit tests cover state-at-N + SC-003 determinism, gen_traces produces JSON files, web assets are written. Maintainer can `cd web && trunk serve --open` to verify the page renders. AI implementer can't visually verify but reports the state up to "WASM built, server-ready".

---

## Phase 4: User Story 2 — Step manually (Priority: P1)

**Goal**: Step Forward / Step Back / Rewind controls work; state-at-N is deterministic.

**Independent Test**: the Cursor unit tests cover this at the Rust level. Browser-level verification is in the SC-008 manual procedure.

### Implementation

- [X] T020 [US2] Review and confirm the Step Forward / Step Back / Rewind wiring in `web/index.js` (added in T018). For each control: verify the button has an explicit `id` (e.g. `#btn-step-forward`); verify the event listener calls the corresponding `player.<method>()` and re-renders. Add a `// US2:` comment header above the three event listeners for traceability. The implementation lives entirely in T018; this task is a code review + tag.
- [X] T021 [US2] Verify the SC-003 determinism property by running the cursor unit test from T011: `cargo test --lib ui::tests::step_back_undoes_step_forward`. Must pass. If it fails, fix `state_snapshot` in T010 — likely the State replay isn't actually pure / re-entrant.

**Checkpoint**: cursor controls wired (in T018) and verified at the Rust level. Browser verification is part of the SC-008 manual procedure (deferred to maintainer).

---

## Phase 5: User Story 3 — Sample selector (Priority: P2)

**Goal**: the page exposes a dropdown listing ≥ 3 samples; selecting one swaps the loaded source + trace; the cursor resets to step 0.

**Independent Test**: open the page, switch sample, confirm editor source changes + stacks empty + cursor at 0. Maintainer verification per SC-008 step 7.

### Implementation

- [X] T022 [US3] Verify the sample selector in `web/index.html` (added in T016) lists 4 options with friendly labels: "Arithmetic" (m03_arithmetic), "Function Call" (m03_fn_call), "Shadowing" (m03_shadow), "Division by Zero" (m03_div_by_zero). Verify the selector's `change` event handler in `web/index.js` (T018) calls `loadSample(event.target.value)` which (per T018 spec): pauses if playing; fetches the new trace JSON; instantiates a new `Player`; replaces the editor doc content; renders initial state. Add a `// US3:` comment header above the selector handler in index.js.
- [X] T023 [US3] Run `cargo run --release --bin gen_traces` again to confirm all 4 samples still produce valid JSON traces. Document in the audit log the event count per sample (e.g. `m03_arithmetic: 5 events, m03_fn_call: 13, m03_shadow: 9, m03_div_by_zero: 2`).

**Checkpoint**: 4 selectable samples ship; selector wiring verified.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: verify SC-005 / SC-006 / SC-007 / SC-008; close out the audit log.

- [X] T024 [P] Verify SC-006 (M01/M02/M03 regression): `cargo test --test m01 && cargo test --test m02 && cargo test --test m03`. All MUST exit 0 with no snapshot drift. The serde derives from T004-T006 must NOT have changed Debug output.
- [X] T025 [P] Verify cursor unit tests: `cargo test --lib ui::`. All 8+ cursor tests from T011 must pass.
- [X] T026 [P] Verify SC-007 (zero warnings on lib): `RUSTFLAGS="-D warnings" cargo build --release`. The host build must be clean. Separately attempt `cargo build --release --target wasm32-unknown-unknown` (without `-D warnings`) — some wasm-bindgen-generated code may produce benign warnings; document any in the audit log.
- [X] T027 [P] Bundle-size check for SC-005: if `trunk` is installed, run `trunk build --release` (in `web/` dir) and measure `dist/*.wasm` + `dist/*.js` raw + gzipped sizes via `gzip -kc <file> | wc -c`. Target: total gzipped ≤ 2 MB. If trunk is NOT installed, document the wasm bundle size from the `target/wasm32-unknown-unknown/release/rustviz.wasm` artifact and defer the full bundle size to the maintainer.
- [X] T028 Append post-implementation audit log to `specs/005-m04-ui-shell/checklists/requirements.md` (mirror M01/M02/M03 pattern). Table of SC-001…SC-008 with PASS/FAIL/DEFERRED + notes. Sections: success-criteria results; per-sample event counts; bundle sizes; manual test procedure status (likely "deferred to maintainer per AI implementer limitation"); any deviations from research/data-model/contract. Explicitly note: the AI implementer compiled the WASM and verified the code-side tests; the browser-rendered behavior (clicking Play, watching frames push/pop, span highlighting) is the maintainer's manual verification per `quickstart.md` SC-008.
- [X] T029 Run final clean verification: `cargo clean && RUSTFLAGS="-D warnings" cargo build --release && cargo test`. Full test suite (M01 + M02 + M03 + lib including new Cursor tests) MUST pass.
- [X] T030 Stage changed files: `git add Cargo.toml Cargo.lock Trunk.toml .gitignore src/event.rs src/typeck.rs src/parse/span.rs src/ui.rs src/lib.rs src/bin/gen_traces.rs web/index.html web/index.js web/style.css web/samples/ specs/005-m04-ui-shell/ CLAUDE.md`. Notably, `web/traces/` is gitignored and NOT staged (regenerated by gen_traces on every build). Run `git status` and report. **Do not commit** — maintainer's call.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1**: no dependencies. T001 (Cargo.toml) before T002 (Trunk.toml) before T003 (file skeleton).
- **Phase 2**: depends on Phase 1. T004/T005/T006 parallel (different files). T007 (regression check) depends on T004-T006. T008 (view types) parallel with T004-T006. T009 (lib.rs) depends on T008.
- **Phase 3 (US1)**: depends on Phase 2 closing. T010 (Cursor impl) → T011 (unit tests) → T012 (wasm bindings) → T013 (gen_traces) → T014 (samples — parallel anytime after Phase 1) → T015 (run gen_traces) → T016/T017/T018 parallel (different web files) → T019 (wasm build verify).
- **Phase 4 (US2)**: depends on Phase 3 (T018, T011). Both tasks are read/verify, not new code.
- **Phase 5 (US3)**: depends on Phase 3 (T016, T018). Same shape — verify wiring.
- **Phase 6**: depends on Phases 4 + 5. T024–T027 read-only audits in parallel. T028–T030 sequential.

### Story-Level Dependencies

- US1 first; US2 and US3 are mostly verification of US1's wiring.

### Parallel Opportunities

- **T004 / T005 / T006**: three serde-derive edits in three files. [P] ✓
- **T014**: sample copies, parallel with T010-T013 implementation.
- **T016 / T017 / T018**: three different web files; can be written in parallel if multiple agents. Sequential for one agent is fine.
- **T024 / T025 / T026 / T027**: four read-only audits.

---

## Parallel Example: Phase 2 Foundational

```bash
Task T004: "Add serde derives in src/event.rs"
Task T005: "Add serde derives in src/typeck.rs"
Task T006: "Add serde derives in src/parse/span.rs"
# Sequential after:
Task T007: "M01/M02/M03 regression"
Task T008: "Define M04 view types in src/ui.rs"
Task T009: "Wire lib.rs re-exports"
```

## Parallel Example: Phase 3 web assets

```bash
# After T010-T015 close:
Task T016: "Write web/index.html"
Task T017: "Write web/style.css"
Task T018: "Write web/index.js"
# Sequential after:
Task T019: "cargo build wasm verify"
```

---

## Implementation Strategy

### MVP First (US1)

1. **Phase 1** (T001–T003): Cargo.toml + Trunk.toml + file skeleton.
2. **Phase 2** (T004–T009): serde derives + M04 view types + lib.rs re-exports. M01-M03 regression clean.
3. **Phase 3** (T010–T019): Cursor impl + unit tests + WASM bindings + gen_traces + 4 samples + web assets + WASM build verification.
4. **STOP and VALIDATE**: cargo test --lib ui:: passes; M01-M03 still green; WASM builds clean; web/traces/*.json exist.
5. **Maintainer-facing handoff**: AI hands the build to the maintainer who runs `cd web && trunk serve --open` and walks through `quickstart.md` SC-008. If the visual page works, MVP ships.

### Incremental Delivery

1. **MVP** = Phases 1–3 (US1 ✓). Page works for the maintainer.
2. **Hardening 1** = Phase 4 (US2 ✓). Step controls verified at the Rust level.
3. **Hardening 2** = Phase 5 (US3 ✓). Sample selector verified.
4. **Ready to commit** = Phase 6 polish closed (SC-005 / SC-006 / SC-007 verified; SC-008 deferred to maintainer; audit log written).

### Single-Agent Strategy

One AI agent works:
1. Phase 1 → Phase 2 (T004-T006 in parallel writes; T007-T009 sequential).
2. Phase 3: T010 (Cursor impl, the meat) → T011 (tests) → T012 (wasm bindings) → T013 (gen_traces) → T014 (sample copies, anytime) → T015 (run gen_traces) → T016/T017/T018 sequential web writes → T019 (wasm build).
3. Phase 4 → Phase 5 sequential (mostly review).
4. Phase 6: audits → audit log → stage.

### Parallel-Agent Strategy

After Phase 2:
- Agent A: T010 → T011 → T012 (the Cursor + wasm bindings — all in `src/ui.rs`).
- Agent B: T013 + T014 + T015 (gen_traces binary + samples).
- Agent C: T016 + T017 + T018 (web/ assets).
- Then sequentially T019 → Phase 4/5/6.

---

## Notes

- [P] tasks = different files, no incomplete-task dependencies.
- **AI implementer limitations**: the agent can write all the code, run cargo build / cargo test, run gen_traces, build the WASM target. The agent CANNOT visually verify the browser page — that requires a human looking at the rendered DOM. SC-008's manual procedure is the maintainer's job. Tasks T021, T022, T023 are at the code-review level (do the buttons wire up?) not the visual level.
- **New regular deps**: this milestone adds 5 (serde, serde_json, wasm-bindgen, js-sys, console_error_panic_hook). All WASM-portable and standard. Per the user's "deps when needed" preference (saved memory).
- **First time the project produces an HTML page**. All code up to this point has been Rust library code. M04 is the first milestone where a user actually sees pixels.
- **`crate-type = ["cdylib", "rlib"]`**: dual-output. The `rlib` keeps `cargo test --lib` working and lets `src/bin/gen_traces.rs` depend on the library. The `cdylib` produces the WASM.
- **`#[cfg(target_arch = "wasm32")]`**: gates the wasm-bindgen exports. Non-WASM builds (e.g. cargo test) don't see them; WASM builds do.
- **CodeMirror via esm.sh CDN**: no JS bundler step. For offline development or air-gapped deploys, vendor the CodeMirror bundles later.
- **T030 staging list**: explicitly excludes `web/traces/` (gitignored). Includes `web/samples/` (checked-in sources). Includes `CLAUDE.md` per the M02 lesson.
- If T011's Cursor unit tests reveal a bug in T010's state-at-N logic, fix T010 and re-run tests. The data-model.md API is not yet committed to anything M04-external (only the StateSnapshot/Player JSON shape is M04 contract); adjustments within the Cursor private impl are fine.
- **No browser-tests / Playwright in M04** — explicitly out of scope per spec. The manual procedure in `quickstart.md` SC-008 is the runtime verification.
- Avoid: putting M05+ (live editing) or M06+ (borrow arrows) work into M04. The roadmap is the contract. Specifically: the editor must be read-only in M04; live re-runs are M05.
