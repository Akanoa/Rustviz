# Specification Quality Checklist: M05 — Live Level 1

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
- **First publicly demoable milestone**: the spec calls out polish (screen recording, git tag, README demo asset) that earlier milestones didn't require. SC-007 commits to the tag + recording explicitly.
- **Editor becomes writable**: a small but real UX shift from M04. The existing `EditorState.readOnly.of(true)` + `EditorView.editable.of(false)` calls in M04's index.js will need to be removed/replaced; spec leaves the exact toggle to plan-phase.
- **Debounce window ≤ 500 ms**: spec's upper bound. Plan-phase will choose the exact value. 300 ms is a reasonable default; 500 ms is the user-perceived "live" threshold.
- **Error UX**: spec allows either "tooltip on underline" or "status bar message" for the error text. Plan-phase decides. Same span-underline mechanic in either case.
- **Three plan-phase deferrals** (no NEEDS CLARIFICATION markers because reasonable defaults exist):
  1. **Debounce timing** (300–500 ms — either works).
  2. **Error message location** (tooltip vs. status bar — both are standard).
  3. **Fate of `gen_traces`** (delete it vs. keep as CLI util — minor cleanup, not load-bearing).
- **No new ID-level entity**: M05 doesn't introduce a new MemEvent variant or a new BindingId-like type. It's a pipeline-wiring milestone. The "Key Entities" section captures conceptual artifacts (source string, live run result, decoration layer, sample file) rather than typed Rust structs.
- **SC-005 bundle-size budget (≤ 5% growth)**: the new live-pipeline code adds the parse + resolve + typeck + evaluate paths to the WASM entry surface. They already compile in for M01–M03 (the cdylib), so the growth should be marginal (mostly the new `compile_and_run` wrapper + error serialization). If growth exceeds 5% during implementation, document in the audit log and decide whether to revisit.
- **No NEW MemEvent variants**: M05 is intentionally additive at the wiring layer. M03.1 added `ReturnValue`; M05 adds nothing to the protocol. This keeps M05 sized **S** per the rubric.
- **Existing M01–M04 samples preserved**: per FR-007. They keep working as starting points; new M05 samples expand the gallery.
- **Edge case "Empty editor"**: deliberately under-specified — plan-phase decides between "empty trace" and "placeholder hint." Either is acceptable per current M04 behavior (the empty initial state was unspecified there too).

## Post-implementation audit (2026-05-22)

Following `/speckit-implement` execution of M05 (19 tasks T001–T019).

### Success-criteria results

| ID    | Description | Result |
|-------|-------------|--------|
| SC-001 | Editor → trace updates within 1 s | **DEFERRED to maintainer** (visual QA) — code path: 300 ms debounce + in-WASM pipeline (sub-100 ms typical) + DOM re-render |
| SC-002 | Error underline appears + clears within 1 s | **DEFERRED to maintainer** (visual QA) |
| SC-003 | M01/M02/M03 byte-identical | PASS — `cargo test --test m01` (8), `--test m02` (16), `--test m03` (8) clean, no `.snap.new` |
| SC-004 | ≥ 4 M05 reference programs | PASS — 4 samples shipped: `minimal`, `let_chain`, `double`, `broken_parse`. Each present in both `tests/samples/` and `web/samples/` |
| SC-005 | WASM bundle growth ≤ +5% vs M04+M03.1 baseline (79,973 B gzipped) | PASS — **63,144 B gzipped (185 KB raw)**, actually **−21%** because removing the pre-recorded trace deserialization path made `Deserialize` for `MemEvent` and friends dead code that the linker discards |
| SC-006 | Zero warnings under `-D warnings` | PASS — host build + full test suite (57 tests across 6 suites) clean, WASM target clean |
| SC-007 | Closing commit tagged + screen recording | **DEFERRED to maintainer** post-merge to main |
| SC-008 | Manual QA procedure | PROCEDURE DOCUMENTED in `quickstart.md` (7 steps); EXECUTION DEFERRED to maintainer |

### Implementation findings

- **WASM bundle decreased by 21%** because the M04 trace-loading path (which deserialized JSON into `Vec<MemEvent>`) is gone. The `MemEvent` enum's `Deserialize` impls are now dead code — only `Serialize` is used (going FROM WASM to JS). The linker discards the unused half. Net: −16,829 B gzipped.

- **All four pipeline stages share `ParseError`** — discovered during T003. No need for separate `From` impls for distinct error types; one `From<ParseError, CompileStage> → CompileError` helper does all four conversions. Cleaner than expected.

- **`pipeline.rs` is small** (~80 lines including 7 unit tests covering Ok + parse/resolve/typeck errors + serde round-trip). The consolidation work was mostly moving boilerplate from `gen_traces.rs` into a shared module.

- **Player API change is a clean signature flip** — `Player::new(trace_json: &str) -> Result<Player, JsValue>` becomes `Player::new(source: &str) -> Player` (infallible). Internally calls `set_source` and discards the JSON. JS callers update one line; existing `state()/step_forward()/etc.` methods are unaffected.

- **JS debounce + updateListener pattern is straightforward** — `EditorView.updateListener.of(update => …)` watches `update.docChanged`, `setTimeout/clearTimeout` coalesces keystrokes. 300 ms feels live without being noisy.

- **Same-file event-trace stays consistent**: `gen_traces` output for the existing `m03_*` samples is **byte-identical** to before the refactor (same event counts: arithmetic 5, fn_call 12, fn_call_twice 21, shadow 7, div_by_zero 2). The pipeline runner is a pure refactor of the orchestration.

- **`@codemirror/commands@6`** import-map entry added pre-plan for the Tab keymap; M05 reuses it without modification.

- **Trunk pre-build hook removal** simplifies the `Trunk.toml` to just `[build]`, `[serve]`, and `[watch]`. The `gen_traces` binary stays for CLI use.

- **No new MemEvent variants**, no new evaluator behavior, no protocol changes. M05 is pipeline wiring + UI affordances. M06 will introduce real protocol changes (borrow events).

### Test summary

```
$ RUSTFLAGS="-D warnings" cargo test
57 passed
  - m01: 8 (byte-identical snapshots)
  - m02: 16 (byte-identical snapshots)
  - m03: 8 (byte-identical snapshots)
  - lib: 25 (6 event smoke + 12 cursor + 7 pipeline)
total: 57 passed; 0 failed; 0 ignored

$ cargo build --release --target wasm32-unknown-unknown
WASM: 185 KB raw / 63,144 B gzipped (was 79,973 B; −21%)
```

### Conclusion

M05 code-side complete. **Shipping for QA.** Maintainer:

1. Walks `quickstart.md` SC-008 procedure (~5 min) covering live-edit, error UX, sample switching.
2. Confirms US1 (typing edits → trace updates) and US2 (errors show underline + status + disabled controls) work end-to-end.
3. After QA passes, **tags the closing commit** (e.g. `git tag m05-edit-run-watch`) per SC-007 and captures a short screen recording for the project README's first demo asset.

This is the project's **first publicly demoable artifact**. Worth a short README polish pass alongside the screen recording.
