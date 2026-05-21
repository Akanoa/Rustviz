---

description: "Task list for M05 — Live Level 1 (edit → run → watch)"
---

# Tasks: M05 — Live Level 1

**Input**: Design documents from `/specs/007-live-l1-editing/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/m05-api.md ✓, quickstart.md ✓

**Tests**: M01/M02/M03 byte-identical (SC-003). New unit tests for `run_pipeline` cover the 4 stage error paths + a couple of success cases. Manual M04+M05 QA per the SC-008 procedure.

**Organization**: 3 user stories (US1+US2 P1, US3 P2). One new module (`src/pipeline.rs`). Pre-plan adjustment (editor writable + `indentWithTab`) already on this branch.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1/US2/US3 tag, mandatory on user-story phases only
- File paths relative to repo root

## Path Conventions

One new module + edits to existing files. See `specs/007-live-l1-editing/plan.md` Project Structure.

---

## Phase 1: Setup

**Purpose**: pre-flight — confirm starting state.

- [X] T001 Verify pre-conditions: branch `007-live-l1-editing` checked out; `cargo test` from `main` passes (50 tests across m01/m02/m03/lib); page loads via `cd web && trunk serve` and the editor is writable (per the pre-plan adjustment). No code change in this task — just confirms the baseline.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: the consolidated pipeline runner + the unified error type — the foundation both user stories build on.

- [X] T002 [P] Amend the M04 contract in `specs/005-m04-ui-shell/contracts/m04-api.md`: document that `Player::new` is being changed (M04 took `trace_json`, M05 takes `source`) and that this is a breaking signature change permitted by M05 with maintainer consent. Relax the "additive only" rule for `Player` methods per the same precedent M03.1 set for `MemEvent`. Reference `specs/007-live-l1-editing/contracts/m05-api.md` for the M05 shape.
- [X] T003 Create `src/pipeline.rs` with: (a) `pub struct CompileError { pub span: Span, pub stage: CompileStage, pub message: String }` with serde derives + Debug + Clone + PartialEq; (b) `pub enum CompileStage { Parse, Resolve, Typeck, Eval }` with the same derives + Copy; (c) `From` impls converting `ParseError`, `resolve::Error`, `typeck::Error`, and `eval::Error` into `CompileError` (set `stage` accordingly, copy `span` and `message`); (d) `pub fn run_pipeline(source: &str) -> Result<Vec<MemEvent>, CompileError>` that runs parse → resolve → typeck → evaluate sequentially, short-circuiting on the first `Err` via `?`. The function constructs its own `SourceMap` internally and adds the source as `"editor.rs"`. Verify with `cargo build`.
- [X] T004 Wire the new module into `src/lib.rs`: add `pub mod pipeline;` and re-export `pub use pipeline::{run_pipeline, CompileError, CompileStage};`. Then update `src/bin/gen_traces.rs` to use `rustviz::run_pipeline` instead of the duplicated inline four-stage chain. The binary's overall behavior is preserved (it still walks `web/samples/m03_*.rs` and writes `web/traces/*.json`); only the internal pipeline glue is replaced. Verify with `cargo run --release --bin gen_traces` — output is identical to the prior version.

**Checkpoint**: `run_pipeline` callable; `gen_traces` still works; M04 contract documents the upcoming Player signature change.

---

## Phase 3: User Story 1 — Edit L1 code, see the trace update live (Priority: P1)

**Goal**: replace M04's pre-recorded-trace flow with live compilation from editor input. Editor change → 300 ms debounce → `Player::set_source(source)` → render the new trace.

**Independent Test**: select Function Call from the dropdown, edit `add(2, 3)` to `add(10, 20)`, observe the stacks panel reset and show `a=10, b=20, →30` once stepped through. Verified by the maintainer per the SC-008 procedure.

### Implementation

- [X] T005 [US1] Update `src/ui.rs::Player` API: (a) change `Player::new(trace_json: &str)` to `Player::new(source: &str) -> Player` (infallible — calls `set_source(source)` internally and discards the returned JSON); (b) add `pub fn set_source(&mut self, source: &str) -> String` returning JSON per `contracts/m05-api.md`. On `run_pipeline` Ok, replace `self.cursor` with a fresh `Cursor::new(events)` at position 0, update `self.source`, and `self.last_error = None`. On Err, replace cursor with empty (`Cursor::new(Vec::new())`), still update `self.source`, set `self.last_error = Some(err)`. Use `serde_json::to_string(&...)` with an inline anonymous serde struct for the discriminated `{ok, state} | {ok, error}` shape. Add a `last_error: Option<CompileError>` field to the `Player` struct. The existing `source()`, `state()`, `step_forward()`, `step_back()`, `rewind()`, `position()`, `total()` methods keep working with the new internal layout.
- [X] T006 [US1] In `src/pipeline.rs`, add a `#[cfg(test)] mod tests` block with ≥ 6 unit tests covering: (a) `run_pipeline_minimal` — `fn main() { let x = 5; }` returns Ok with non-empty events; (b) `run_pipeline_arithmetic` — `fn main() { let x = 2 + 3; }` returns Ok; (c) `run_pipeline_fn_call` — the existing m03_fn_call source returns Ok with the same event count as the M03 snapshot (post-M03.1: 12); (d) `run_pipeline_parse_error` — `fn main() { let x = ; }` returns `Err(CompileError { stage: Parse, .. })`; (e) `run_pipeline_resolve_error` — `fn main() { let y = undefined_var; }` returns `Err(stage: Resolve)`; (f) `run_pipeline_typeck_error` — `fn main() { let z: i32 = true; }` returns `Err(stage: Typeck)`. Each error test asserts the stage and that `span` is non-empty.
- [X] T007 [US1] In `web/index.js`, replace M04's trace-fetch + `new Player(traceJson)` flow: (a) the editor's existing extensions list gets an `EditorView.updateListener.of(...)` that watches for `update.docChanged`; on change, `clearTimeout` any prior debounce and `setTimeout(..., 300)` a new one; the timer fires `recompile(editor.state.doc.toString())`; (b) `recompile(source)` calls `JSON.parse(player.set_source(source))`. If `result.ok`, call `render(result.state)`; if not, call `renderError(result.error)` (defined in US2's task T010). The existing Play / Step / Rewind controls already call `player.step_forward()` etc.; nothing changes there. Player is created once at startup via `new Player(initialSource)` (US1's `loadSample` task T008 writes the initial source). Add a comment tagging US1 above the new code blocks.
- [X] T008 [US1] In `web/index.js`, change `loadSample(id)` to fetch `/samples/<id>.rs` (raw source text) instead of `/traces/<id>.json`. Replace `player = new Player(traceText)` with `editor.dispatch({ changes: { from: 0, to: editor.state.doc.length, insert: source } })` — the existing `updateListener` from T007 will pick it up and debounced-recompile. In `web/index.html`, replace `<link data-trunk rel="copy-dir" href="traces" />` with `<link data-trunk rel="copy-dir" href="samples" />` so `/samples/<id>.rs` resolves. Also remove the now-unused `setEditorSource` helper if it becomes dead code (or keep it if used for initial load — implementer's call).
- [X] T009 [US1] In `web/Trunk.toml`, remove the `[[hooks]]` block (the pre-build `gen_traces` invocation). Trunk's `[watch] ignore` list keeps `traces` and `dist` (no-op now since traces won't regenerate, but harmless). Verify `cd web && trunk serve` still works.

**Checkpoint**: editor edits trigger debounced re-compiles; sample dropdown loads source from `/samples/<id>.rs`; the stacks panel updates per the new trace. Acceptance via in-browser verification (SC-008 step 2).

---

## Phase 4: User Story 2 — Errors visible inline with span underlines (Priority: P1)

**Goal**: when `set_source` returns Err, paint a red wavy underline on the error span and show the message in the status bar. Disable Play / Step controls.

**Independent Test**: type `let x = ;` (parse error), observe red wavy underline at the error span + a red status message. Step Forward button is grayed/disabled. Fix the syntax — underline clears, controls re-enable.

### Implementation

- [X] T010 [US2] In `web/index.js`, add a new CodeMirror StateField `errorField` paired with a `setError` `StateEffect`. The field stores either a `null` decoration set or a single `Decoration.mark({ class: 'cm-error-span' })` at the error span's `[start..end]`. Add the field to the editor's extensions list (alongside `highlightField` and `currentFnField`). Add a `renderError(error)` helper that calls `editorView.dispatch({ effects: setError.of({ start, end }) })` and also writes `error.message` (prefixed with the stage, e.g. `"Parse error: expected expression"`) into `#status` with the existing `.status-error` class. The success path (`render(state)` after Ok) should also dispatch `setError.of(null)` to clear any prior error and reset the status bar.
- [X] T011 [US2] In `web/style.css`, add a `#editor .cm-error-span` rule. Suggested styling: `text-decoration: red wavy underline; text-decoration-thickness: 1px;` (the wavy red underline is the universal "something is wrong here" idiom). Verify the underline doesn't conflict visually with the existing `.cm-current-span` (yellow background) or `.cm-current-fn` (red border) — the wavy underline lives at a different layer.
- [X] T012 [US2] In `web/index.js`, gate the playback controls on the error state. Add a helper `setControlsEnabled(enabled)` that toggles the `disabled` attribute on `#btn-play-pause`, `#btn-step-back`, `#btn-step-forward`. Rewind (`#btn-rewind`) stays always enabled. Call `setControlsEnabled(false)` from `renderError` and `setControlsEnabled(true)` from `render`. Also ensure that on `set_source` Err, any active play interval is stopped (`stopPlay()`). The existing `:disabled` CSS rule in `web/style.css` handles the visual.

**Checkpoint**: an error in editor content produces an inline underline + status message + disabled controls. Fix → all three clear.

---

## Phase 5: User Story 3 — M05 reference samples (Priority: P2)

**Goal**: ship 4 new reference programs covering the L1 edit cases, including one deliberately broken.

**Independent Test**: open the dropdown; verify ≥ 3 M05-prefixed entries; select each; broken sample triggers the error UX from US2.

### Implementation

- [X] T013 [P] [US3] Create 4 new sample files (8 files total since each lives in both `tests/samples/` and `web/samples/` with identical content):

  - `tests/samples/m05_minimal.rs` + `web/samples/m05_minimal.rs` — `fn main() { let x = 5; }`
  - `tests/samples/m05_let_chain.rs` + `web/samples/m05_let_chain.rs` — `fn main() { let x = 1; let y = x + 2; let z = y * 3; }`
  - `tests/samples/m05_double.rs` + `web/samples/m05_double.rs` — `fn double(n: i32) -> i32 { n + n }\nfn main() { let r = double(21); }`
  - `tests/samples/m05_broken_parse.rs` + `web/samples/m05_broken_parse.rs` — `fn main() { let x = ; }`

  Match the formatting of the existing `m03_*.rs` files (trailing newline, no extra whitespace). The `tests/samples/` copies aren't picked up by any M03 integration test (no `sample_test!` lines added); they're there for parity with M03/M04 samples.

- [X] T014 [US3] In `web/index.html`, add 4 new `<option>` entries to the sample dropdown after the existing M03 entries:

  ```html
  <option value="m05_minimal">Minimal (M05)</option>
  <option value="m05_let_chain">Let chain (M05)</option>
  <option value="m05_double">Double fn (M05)</option>
  <option value="m05_broken_parse">Broken parse (M05)</option>
  ```

  Order is alphabetical within the M05 group. The trailing `(M05)` label clarifies provenance vs. the M03/M04 samples.

**Checkpoint**: dropdown has the new entries; each loads a valid source (or the deliberate parse error for the broken one).

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: verify SC-003, SC-005, SC-006; final clean verify; audit log; stage. The git tag (SC-007) is the maintainer's act post-merge; screen recording is also maintainer-side.

- [X] T015 [P] Verify SC-003 (M01/M02/M03 byte-identical): `cargo test --test m01 && cargo test --test m02 && cargo test --test m03`. All exit 0 with no `.snap.new` files in `tests/snapshots/`. If anything drifts, M05 has touched code it shouldn't — investigate.
- [X] T016 [P] Verify SC-005 (bundle size ≤ +5% from M04+M03.1 baseline) AND SC-006 (zero warnings under `-D warnings`). Commands:
  - `RUSTFLAGS="-D warnings" cargo build --release` — clean host build.
  - `RUSTFLAGS="-D warnings" cargo test` — clean test suite.
  - `cargo build --release --target wasm32-unknown-unknown` — WASM clean.
  - `gzip -kc target/wasm32-unknown-unknown/release/rustviz.wasm | wc -c` — must be ≤ 83910 B (M03.1 baseline 79,973 B + 5% ≈ 83,972 B). If exceeded, investigate before continuing.
- [X] T017 Run final clean verification: `cargo clean && RUSTFLAGS="-D warnings" cargo build --release && RUSTFLAGS="-D warnings" cargo test && cargo build --release --target wasm32-unknown-unknown`. Full pipeline must pass clean from scratch.
- [X] T018 Append post-implementation audit log to `specs/007-live-l1-editing/checklists/requirements.md` (mirror the M01–M04 + M03.1 pattern). Table covering SC-001 through SC-008. Mark SC-007 (git tag + screen recording) as **DEFERRED to maintainer** since those happen post-merge. Document any QA-driven follow-ups discovered during the maintainer's pass.
- [X] T019 Stage all changed files: `git add Cargo.toml Cargo.lock src/pipeline.rs src/lib.rs src/ui.rs src/bin/gen_traces.rs tests/samples/m05_*.rs web/samples/m05_*.rs web/index.html web/index.js web/style.css web/Trunk.toml specs/005-m04-ui-shell/contracts/m04-api.md specs/007-live-l1-editing/ CLAUDE.md`. Cargo.toml/Cargo.lock likely unchanged (no new Rust deps); include defensively. Run `git status` and report. **Do not commit** — maintainer's QA pass happens between stage and commit per the UI QA-split convention.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: no dependencies.
- **Phase 2 (Foundational)**: T002 parallel to T003 (different files). T004 depends on T003 (lib.rs re-exports the new types). All blocking the user-story phases.
- **Phase 3 (US1)**: depends on Phase 2. T005 → T006 → T007 → T008 sequential (each builds on prior). T009 (Trunk.toml) parallel to T007/T008 once T005 lands.
- **Phase 4 (US2)**: depends on Phase 3 (the error decoration needs the `set_source` Err path). T010 → T011 → T012 sequential.
- **Phase 5 (US3)**: depends on Phase 3 (sample-load wiring). T013 parallel to T014 (different files). Both can be done in parallel to Phase 4.
- **Phase 6 (Polish)**: depends on Phases 3–5 closing. T015 / T016 parallel. T017 → T018 → T019 sequential.

### Story-Level Dependencies

- US1 is the foundational user story. US2 depends on US1's `set_source` Err path. US3 depends on US1's sample-load wiring + US2's error UX (so the broken sample's behavior is observable).

### Parallel Opportunities

- **T002 + T003**: M04 contract amendment vs. new pipeline module. Different files. [P] ✓
- **T013 (sample files)**: 4 file pairs, all writable in parallel. [P] ✓
- **T015 + T016**: regression vs. bundle/warnings checks. Read-only. [P] ✓

---

## Parallel Example: Phase 5 samples

```bash
# All 4 sample-file pairs are independent (different files):
Task T013a: "Create m05_minimal.rs in tests/samples/ + web/samples/"
Task T013b: "Create m05_let_chain.rs in tests/samples/ + web/samples/"
Task T013c: "Create m05_double.rs in tests/samples/ + web/samples/"
Task T013d: "Create m05_broken_parse.rs in tests/samples/ + web/samples/"
```

(Above is illustrative — the actual tasks.md bundles them as T013 for brevity since each file is < 10 lines.)

---

## Implementation Strategy

### MVP First (US1 alone)

1. **Phase 1** (T001): pre-flight.
2. **Phase 2** (T002, T003, T004): contract + pipeline + lib wiring.
3. **Phase 3** (T005–T009): Player API + JS wiring + sample-loading change + Trunk hook removal.
4. **STOP and VALIDATE**: edit the editor, observe the trace update live. M01/M02/M03 tests still pass. **At this point M05's headline value is shippable.**

Phase 4 (error UX) and Phase 5 (samples) can ship in the same milestone but are independently testable.

### Single-Agent Strategy

One AI agent:

1. T001 (no-op pre-flight) → T002 + T003 (parallel writes if possible; sequential in practice) → T004 (depends on T003).
2. T005 (Player API) → T006 (unit tests in same file) → T007 (JS debounce) → T008 (sample-load change) → T009 (Trunk.toml).
3. T010 (errorField + setError) → T011 (CSS) → T012 (disable controls).
4. T013 (sample files) → T014 (dropdown HTML).
5. Phase 6: T015 + T016 (read-only checks), T017 (final clean), T018 (audit), T019 (stage).

---

## Notes

- [P] tasks = different files, no incomplete-task dependencies.
- [Story] tag mandatory on user-story phases.
- **No new Rust deps**. JS gains `@codemirror/commands@6` (already added pre-plan for the Tab keymap; M05's plan reuses the same importmap entry).
- **Pre-plan adjustment already on branch**: editor is writable + `indentWithTab` keymap; T005's Player API replaces M04's read-only flow.
- **`tests/samples/m05_*.rs` not used by any test driver yet** — only `web/samples/m05_*.rs` is consumed (by the page). The `tests/samples/` copies exist for parity with `m03_*` (where they ARE used by `m03.rs`). If a future M06+ wants to assert M05 sample event streams, adding `sample_test!` lines to a new `tests/m05.rs` is the next step.
- **`gen_traces` binary stays in the repo** but is no longer in trunk's pre-build hook. `cargo run --release --bin gen_traces` still works as a CLI verification utility.
- **`web/traces/*.json`** become orphaned artifacts after M05. They're already gitignored; no cleanup needed.
- **SC-007 git tag + screen recording**: maintainer's act, post-merge to main. Not an AI implementer task. T018 audit log notes this explicitly.
- **M04 contract change is documented** in T002 (relax "additive only" for Player methods, same precedent as M03.1's MemEvent rule).
- **No new MemEvent variants, no new evaluator behavior**: M05 is wiring + UI, not protocol. Existing M03+M03.1 events flow through unchanged.
- **`CLAUDE.md`** may get an auto-update from `/speckit-plan` (it did in prior milestones). Include in the T019 stage list.
- **Maintainer QA between stage and commit** — same pattern as M04 and M03.1.
- Avoid: implementing M06 (borrows) work in M05. M05 is strictly L1 wiring.
