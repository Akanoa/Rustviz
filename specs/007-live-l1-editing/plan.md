# Implementation Plan: M05 — Live Level 1 (edit → run → watch)

**Branch**: `007-live-l1-editing` | **Date**: 2026-05-22 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/007-live-l1-editing/spec.md`

## Summary

Wire M03's L1 evaluator + M01/M02 parser/resolver/typeck behind a single live-pipeline entry point exposed through the M04 WASM `Player`. The editor (already writable from this branch's pre-plan adjustment) gets a 300 ms debounce that calls `Player::set_source(&str)` on the current text. The pipeline returns either an `Ok` (events are replayed by the existing Cursor / StateSnapshot machinery) or an `Err` with a `Span` + message that the editor surfaces as a red wavy underline + a status-bar message. The sample dropdown now loads `.rs` source into the editor (instead of pre-recorded JSON traces), which triggers the same re-run path. Four `m05_*.rs` reference samples ship with the milestone, including one deliberately broken to demonstrate the error UX.

Authority chain: `MILESTONES.md` › M05 → `spec.md` (this feature) → this plan.

## Technical Context

**Language/Version**: Rust 2024 edition (same toolchain as M01–M03.1). No new toolchain requirements.
**Primary Dependencies**: existing `indexmap`, `serde`, `serde_json`, `wasm-bindgen`, `js-sys`, `console_error_panic_hook`. **No new Rust deps**. New JS dependency on `@codemirror/commands@6` (already added pre-plan for the Tab-handling adjustment; reused here for the keymap).
**Storage**: in-memory; no new files. `web/traces/*.json` files become deprecated artifacts (FR-010); trunk's pre-build `gen_traces` hook is dropped.
**Testing**: existing `cargo test --test m01 / m02 / m03` byte-identical (SC-003); new `cargo test --lib pipeline::` for the consolidated runner (≥ 4 tests covering Ok/Err for parse/resolve/typeck/eval); manual M05 QA per the SC-008 procedure.
**Target Platform**: same as M01–M04 (host + `wasm32-unknown-unknown`).
**Project Type**: Rust library + companion M04/M05 UI; no new modules in `src/`, one new module `src/pipeline.rs` consolidating the four-stage runner.
**Performance Goals**: SC-001 commits to ≤ 1 s editor-to-stacks latency for any L1 program ≤ 50 lines. M01–M03 already run in < 10 ms per L1 program; debounce 300 ms; WASM/JS boundary serde < 50 ms. Comfortable budget.
**Constraints**: M01/M02/M03 snapshots untouched (SC-003 byte-identical); WASM bundle growth ≤ 5% vs M04 baseline 79,915 B gzipped (SC-005); zero warnings under `-D warnings` (SC-006); existing M04 UI features preserved (FR-006); manual QA passes (SC-008).
**Scale/Scope**: ~7 files modified + 1 new module + 4 new sample files + 1 new contract doc. Estimated ~250 LOC of net code changes. Sizing per the rubric: **S** as MILESTONES.md classifies (single primary module, single integration boundary).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

`.specify/memory/constitution.md` is still the unfilled speckit template. Same vacuous PASS as features 001–006.

**Post-design re-check**: still vacuous, still PASS.

## Project Structure

### Documentation (this feature)

```text
specs/007-live-l1-editing/
├── plan.md                 # This file
├── spec.md                 # Feature spec
├── research.md             # Phase 0: ~12 design decisions
├── data-model.md           # Phase 1: CompileError + Player::set_source + decoration layer
├── quickstart.md           # Phase 1: dev workflow + manual QA procedure
├── contracts/
│   └── m05-api.md          # Phase 1: Player::set_source contract + sample-loading semantics
├── checklists/
│   └── requirements.md     # From /speckit-specify
└── tasks.md                # NOT created here — /speckit-tasks output
```

### Source Code (repository root) — files M05 touches

```text
src/
├── pipeline.rs             # NEW — `pub fn run_pipeline(source: &str) -> Result<Vec<MemEvent>, CompileError>`
│                           #       consolidates parse → resolve → typeck → evaluate behind one function.
│                           #       The CompileError type unifies the four error types behind a single
│                           #       `{span, stage, message}` shape.
├── lib.rs                  # MODIFIED — re-export `pipeline::{run_pipeline, CompileError, CompileStage}`.
├── ui.rs                   # MODIFIED — new `Player::set_source(source: &str) -> String /* JSON */`
│                           #            method. Returns `{ok: true, state: <StateSnapshot>}` or
│                           #            `{ok: false, error: {span, stage, message}}`. Existing Player
│                           #            methods (state, step_forward, etc.) unchanged.
├── bin/gen_traces.rs       # MODIFIED — switch to use `rustviz::run_pipeline`; remove the duplicated
│                           #            inline pipeline orchestration. Binary stays as CLI util.
└── (other src/ files unchanged)

tests/
├── m01.rs, m02.rs, m03.rs  # Unchanged. Snapshots byte-identical.
├── snapshots/              # Unchanged.
└── samples/
    ├── m03_*.rs            # Unchanged (existing).
    └── m05_*.rs            # NEW (4 files): minimal, let-chain, fn-call, broken-parse.

web/
├── samples/                # MODIFIED — add 4 `m05_*.rs` files (mirrors `tests/samples/m05_*.rs`).
├── traces/                 # OBSOLETE — page no longer fetches these. Gitignored already.
├── index.html              # MODIFIED — `<link data-trunk rel="copy-dir" href="samples">` (so
│                           #            `/samples/<name>.rs` is served); option dropdown updated
│                           #            with M05 entries.
├── index.js                # MODIFIED — sample-loading switches from `fetch(/traces/...)` to
│                           #            `fetch(/samples/<name>.rs)` + editor.setValue; debounced
│                           #            update listener calls `player.set_source(source)`; error
│                           #            decoration field; controls disable on error.
├── style.css               # MODIFIED — `.cm-error-span` red wavy underline.
└── Trunk.toml              # MODIFIED — remove the pre_build `gen_traces` hook (FR-010); `[watch]
                            #            ignore` updated to also skip the now-unused `traces/`.

# The M04 contract document gets an extension amendment:
specs/005-m04-ui-shell/contracts/m04-api.md   # MODIFIED — document the new Player::set_source method;
                                              #   note the deprecation of the trace-JSON loading path.
```

**Structure Decision**: one new module (`src/pipeline.rs`) for the four-stage consolidation. The rest is in-place modification at existing seam points (`Player` in ui.rs, sample loader in index.js, CSS for the error underline). M05 doesn't introduce a new abstraction; it wires existing ones together at the WASM boundary.

## Complexity Tracking

> No constitutional violations. Table omitted.
