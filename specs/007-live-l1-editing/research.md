# Research — M05 Implementation Decisions

Decision / Rationale / Alternatives for the live-pipeline milestone.

## Pipeline consolidation

### R-001 — One `run_pipeline(&str)` function in a new `src/pipeline.rs`

- **Decision**: introduce `pub fn run_pipeline(source: &str) -> Result<Vec<MemEvent>, CompileError>` in a new module `src/pipeline.rs`. It runs parse → resolve → typeck → evaluate sequentially and short-circuits on the first error.
- **Rationale**:
  - The four stages already exist independently. M05 just needs a single canonical entry point — both `gen_traces` (CLI) and `Player::set_source` (WASM) call the same function.
  - A dedicated module documents the canonical sequence and the error mapping. Future levels (M06+) extend the same function as new stages plug in.
  - Returning `Result<Vec<MemEvent>, CompileError>` matches the existing M01–M03 error idiom.
- **Alternatives considered**:
  - **Inline in `Player::set_source`**: works but duplicates the four-stage glue between `set_source` and `gen_traces`. Rejected.
  - **Trait-based orchestration**: over-engineered for a fixed 4-stage chain. Rejected.

### R-002 — `CompileError { span, stage, message }` unifies the four error types

- **Decision**: a new struct `CompileError { span: Span, stage: CompileStage, message: String }` with `CompileStage` enum (`Parse | Resolve | Typeck | Eval`). Each existing per-stage error has a `From` impl into `CompileError`.
- **Rationale**:
  - JS consumers don't need to distinguish stage logic — they need `(span, message)` to display the underline. `stage` is a useful informational field for the status bar ("typeck error: …") without forcing JS to pattern-match.
  - One serializable type at the WASM boundary instead of a tagged-union of four. Smaller JS payload.
  - `CompileStage` is a closed enum (4 variants) — additive-only growth in future milestones if pipeline stages change.
- **Alternatives considered**:
  - **Boxed `dyn Error`**: needs a custom JS-friendly serializer. Rejected.
  - **String-only error**: loses the span, can't drive the editor underline. Rejected.
  - **Per-stage Result types kept distinct, JS dispatches**: more JS branching, more WASM glue. Rejected.

## Player API extension

### R-003 — `Player::set_source(&str) -> String /* JSON */`

- **Decision**: extend the existing `Player` struct (in `src/ui.rs`) with a new wasm-bindgen method `set_source(&mut self, source: &str) -> String`. The return value is JSON of shape `{ok: true, state: <StateSnapshot>}` or `{ok: false, error: {span, stage, message}}`. On success the Player's internal cursor is replaced with a new one positioned at 0; the source string is stored for `source()`. On error the existing cursor is left **empty** (zero events) so playback controls have nothing to advance into.
- **Rationale**:
  - JS gets both the result AND the new state in one round-trip — saves a follow-up `state()` call.
  - Reseting the cursor on every successful re-run matches FR-009. Sending an empty trace on error matches FR-005 (Play / Step do nothing because there are no events).
  - Returning a single JSON string keeps the wasm-bindgen surface narrow (one string in, one string out) — minimal serde codegen.
  - Existing `Player::new(source: &str)` constructor is updated to call `set_source` internally; if construction is called with invalid source, the Player exists in error state.
- **Alternatives considered**:
  - **Two methods (compile + load)**: separates compilation from playback. More complex JS orchestration. Rejected.
  - **Throw a JS exception on error**: forces JS try/catch, asymmetric with the success path's structured data. Rejected.
  - **Keep cursor at previous position on error**: confusing — the source has changed; the previous events no longer make sense. Rejected per FR-009.

### R-004 — Player constructor signature is `Player::new(source: &str)`

- **Decision**: `Player::new(source: &str) -> Player` (infallible). Internally calls `set_source(source)`. On parse/resolve/typeck error, the Player exists with empty cursor + remembered error.
- **Rationale**:
  - JS code is simpler: `const player = new Player(""); player.set_source(initialSource);` is awkward. Folding into a single constructor matches the natural JS pattern.
  - Infallible constructor means the JS layer doesn't need to handle the "Player couldn't be created" case as a special path.
  - First call's result is queryable via the same JSON `state()` / `error()` shape.
- **Alternatives considered**:
  - `Player::new() -> Player` (no args): forces JS to always call `set_source` to do anything. Slightly redundant. Rejected.
  - `Player::new(source) -> Result<Player, JsValue>`: error in constructor means we can't even paint the editor. Rejected.

## Sample loading

### R-005 — Dropdown loads `.rs` source via `fetch('/samples/<id>.rs')`

- **Decision**: M05's index.js fetches the raw `.rs` file from `/samples/<id>.rs` and writes it into the editor. The editor's `updateListener` then debounces and calls `player.set_source(...)`. The trunk `<link rel="copy-dir" href="samples">` directive serves the directory.
- **Rationale**:
  - Source is the single point of truth — `web/samples/*.rs` files are what `gen_traces` consumed in M04 and what tests/samples/m05_*.rs covers. Reusing those files for the page is consistent.
  - Re-using the existing debounced update listener for sample-load means the same code path handles both manual edits and sample-switching. Less branching.
- **Alternatives considered**:
  - **Inline sample text in JS** (string constants): less network round-trips but couples the JS bundle to the sample contents — every sample edit requires an index.js change. Rejected.
  - **JSON manifest with `{id, source}`**: indirection for no real benefit. Rejected.

## Debouncing

### R-006 — 300 ms debounce via `setTimeout` in the editor's `updateListener`

- **Decision**: JS uses `EditorView.updateListener.of(update => ...)`. On `update.docChanged === true`, clear any pending timer and set a new one for 300 ms. When it fires, call `player.set_source(doc.toString())` and render.
- **Rationale**:
  - 300 ms is the user-perceived "live" threshold without being noisy mid-typing. Anything < 200 ms risks running the pipeline on partial keystrokes; > 500 ms feels laggy.
  - `setTimeout` + `clearTimeout` is the idiomatic browser debounce; no library needed.
  - `EditorView.updateListener` is CodeMirror 6's standard hook for reacting to changes.
- **Alternatives considered**:
  - **Explicit "Run" button**: matches MILESTONES.md's "click run" demo step literally, but feels manual in a "live" milestone. Decided against. (The existing Play button still exists for stepping; the "run" intent is folded into the auto-debounce.)
  - **No debounce (run on every keystroke)**: noisy, costly. Rejected.
  - **`requestIdleCallback`**: less precise timing, browser-dependent behavior. Rejected.

## Error UX in the editor

### R-007 — Red wavy underline + status-bar message

- **Decision**: a new CodeMirror `StateField` `errorField` painting a `Decoration.mark({ class: 'cm-error-span' })` at the error's span. CSS gives `.cm-error-span` a wavy red underline. The error message is shown in the existing M04 status bar (`<div id="status">`) styled with the existing `.status-error` class (already there for `RuntimeError` notes).
- **Rationale**:
  - Wavy red underline is the universal "this is wrong here" idiom (IDEs, browsers). Instantly readable.
  - Status bar already exists for M03 runtime errors; reusing it keeps the UI uncluttered and consistent.
  - The decoration field follows the same pattern as `highlightField` / `currentFnField` — implementer doesn't have to learn a new mechanism.
- **Alternatives considered**:
  - **Tooltip on hover**: extra discoverability cost; user has to hover the underline to read the message. Rejected for the status-bar approach.
  - **CodeMirror's `lintGutter` extension**: more featureful but pulls in `@codemirror/lint` — extra dep + import-map entry. Rejected for now; can revisit if multi-diagnostic UIs land later.
  - **Modal / banner alert**: too intrusive for what should be an inline cue. Rejected.

### R-008 — Playback controls disabled in error state

- **Decision**: on error, the JS render layer adds the `disabled` HTML attribute to Play / Step Forward / Step Back. Rewind stays enabled. The buttons get a CSS opacity tweak via the existing `:disabled` rule.
- **Rationale**:
  - There are no events to play. Disabling the buttons prevents confusing "click does nothing" interactions.
  - Rewind remains enabled because rewinding to position 0 is meaningful even when the trace is empty.
  - Native `disabled` attribute is accessible (screen readers, keyboard navigation).
- **Alternatives considered**:
  - **Hide the buttons**: feels jumpy. Rejected.
  - **Show "Fix errors first" tooltip**: redundant with the status-bar message. Rejected.

## gen_traces / pre-build hook

### R-009 — Remove the trunk pre-build hook; keep the binary as a CLI util

- **Decision**: remove `[[hooks]]` from `web/Trunk.toml`. The page no longer needs pre-generated trace JSONs. `src/bin/gen_traces.rs` stays in the repo but is updated to use `run_pipeline` (R-001). Useful for offline pipeline verification.
- **Rationale**:
  - Per FR-010, the page no longer consumes the trace files. Running gen_traces on every trunk build wastes time.
  - Keeping the binary as `cargo run --bin gen_traces` (without trunk's pre-build hook) is cheap insurance — a maintainer can verify the pipeline runs end-to-end on the samples without booting a browser.
  - Deleting `web/traces/` directory and `.gitignore` entry isn't strictly required (still gitignored, just unused).
- **Alternatives considered**:
  - **Delete `gen_traces` entirely**: loses the CLI verification utility. Could be added back later but no reason to remove. Rejected.
  - **Keep the pre-build hook**: wastes ~1-2 s of trunk build time for unused artifacts. Rejected.

## Reference samples

### R-010 — Four M05 reference programs

- **Decision**: ship four `m05_*.rs` reference files:

  | File                       | Purpose                                                      |
  |----------------------------|--------------------------------------------------------------|
  | `m05_minimal.rs`           | `fn main() { let x = 5; }` — the smallest possible L1 program. |
  | `m05_let_chain.rs`         | `fn main() { let x = 1; let y = x + 2; let z = y * 3; }` — chain of dependent lets. |
  | `m05_double.rs`            | `fn double(n: i32) -> i32 { n + n } fn main() { let r = double(21); }` — fn call with single param. |
  | `m05_broken_parse.rs`      | `fn main() { let x = ; }` — deliberate parse error, demonstrates US2 UX. |

- **Rationale**:
  - Per FR-008 / SC-004, need ≥ 4 samples covering edit-friendly cases. These four cover: smallest valid program, sequential lets with cross-reference, fn-call-with-result, and error-state.
  - `m05_double` differs from the existing `m03_fn_call` (which uses two-arg `add`) by being single-arg, slightly smaller, and arithmetically more interesting (squaring-like double).
  - `m05_broken_parse` is intentionally never going to compile — its purpose is to show the editor underline UX. Tests using it should assert the error appears, not that it parses.
- **Alternatives considered**:
  - **More samples**: scope creep. Four is enough.
  - **Use only broken-resolve or broken-typeck examples**: parse error is the most pedagogically obvious. We can add resolve/typeck broken samples later if M05's QA shows the parse-only error case isn't sufficient.
  - **Skip the broken sample**: misses the chance to demo US2 in the dropdown. Rejected.

## WASM / serde wire format

### R-011 — `set_source` return shape uses a discriminated `ok` boolean

- **Decision**: the JSON returned by `Player::set_source` is `{ "ok": true, "state": <StateSnapshot> }` or `{ "ok": false, "error": { "span": {start, end, file}, "stage": "Parse"|"Resolve"|"Typeck"|"Eval", "message": "..." } }`. JS reads `result.ok` to branch.
- **Rationale**:
  - Discriminated unions in JSON are well-understood by JS consumers.
  - Including the `StateSnapshot` in the success payload saves a separate `state()` call after every `set_source`.
  - `stage` is a string (not a number) for human-readable debugging in DevTools.
- **Alternatives considered**:
  - **`null | { error: ... }` (error sentinel)**: missing `ok` indicator; JS has to do null checks AND error checks. Rejected.
  - **Throwing a JsError**: asymmetric with success path. Rejected.

### R-012 — Tab keymap already landed pre-plan

- **Decision**: the `indentWithTab` keymap and writable-editor toggle landed in a small pre-plan adjustment on this branch. M05's `/speckit-implement` doesn't need to re-add them. The same import-map entry for `@codemirror/commands` covers future keymap needs (history, default keymap, etc.) if we extend.
- **Rationale**: the spec writer (`/speckit-specify`) authorized making the editor writable as part of US1. Doing it inline during the spec phase didn't break anything (the trace just doesn't update on edit yet) and lets QA exercise editing immediately.
- **Alternatives considered**: revert the pre-plan adjustment and re-do during implement — pointless churn. Rejected.

## Constitution

### R-013 — Same vacuous PASS

- **Decision**: `.specify/memory/constitution.md` still unfilled. No gates apply.

## Open question — not blocking

- **Visual style of the wavy underline**: defaults to `text-decoration: red wavy underline`. Implementer tunes thickness / color tone during M04-style QA.
