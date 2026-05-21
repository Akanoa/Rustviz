# Specification Quality Checklist: M04 — UI Shell + Replay Cursor

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
- **First end-user-facing milestone**: spec calls this out explicitly. The "users" framing flips from internal (M01–M03 audiences were the next milestone's implementer) to external — a beginner Rust learner who actually interacts with the page. This is a significant tonal shift; future milestones that extend the UI (M05–M08) will continue with the same end-user framing.
- **Three plan-phase deferrals** (no NEEDS CLARIFICATION markers because reasonable defaults exist):
  1. **Editor framework**: Monaco vs CodeMirror vs alternative. Both work; weight, language support, and bundle-size impact differ. Plan-phase decision.
  2. **UI rendering framework**: vanilla JS / a JS framework / Yew / Sycamore. Trade-offs in bundle size, complexity, future extensibility (M05+ needs more interactivity). Plan-phase decision.
  3. **Trace serialization format**: JSON via serde, or hand-rolled, or some other format. Adding `serde` + `serde_json` as deps is fine per the project's "deps when needed" stance. Plan-phase decision.
- **No automated browser tests**: deliberately scoped out. Manual test procedure documented in SC-008 + quickstart. Future infrastructure milestone (separate from L1–L4 plan) may add Playwright/WebDriver.
- **SC-001 ("5 min from clone to first Play click")** is an end-user-experience metric that depends on toolchain availability. The wording explicitly assumes Rust toolchain installed, which is fair given the project's audience.
- **SC-005 bundle-size target (2 MB gzipped)** is a soft target. If the editor framework choice (Monaco) blows past it, document the actual size in the audit log and decide whether to switch frameworks or accept the size.
- **Heap panel reserved but empty**: per CLAUDE.md, three panels are part of the architecture. M04 ships the layout with the heap region present so M07 can fill it in additively rather than restructuring the page.
- **Edge case "Note RuntimeError stops playback"**: this is the first place the runtime-error pedagogy from M03 gets visible. Worth confirming in the manual test (SC-008) procedure.

## Post-implementation audit (2026-05-21)

Following `/speckit-implement` of M04. All 30 tasks (T001–T030) executed; code-side checks all pass; visual QA deferred to maintainer per the established UI handoff arrangement.

### Success-criteria results

| ID    | Description | Result |
|-------|-------------|--------|
| SC-001 | < 5 min from clone to first Play click | **DEFERRED to maintainer** (visual QA) |
| SC-002 | ≥ 3 samples ship as user-selectable traces | PASS — **4 samples** ship: `m03_arithmetic` (5 events), `m03_fn_call` (13), `m03_shadow` (8), `m03_div_by_zero` (2). All four pre-recorded as valid JSON under `web/traces/` |
| SC-003 | Cursor determinism (rewind+step N == step back from later) | PASS — `step_back_undoes_step_forward` unit test in `src/ui.rs::tests` passes |
| SC-004 | Auto-play visibly advances at ≈ 1 event/400 ms | DEFERRED to maintainer (visual QA) |
| SC-005 | Page loads in < 3s; WASM ≤ 2 MB gzipped | PASS for the WASM bundle — **78 KB gzipped (239 KB raw)**, far under target. Full page-load timing deferred to maintainer's QA |
| SC-006 | M01, M02, M03 tests still pass | PASS — 8 + 16 + 8 = 32 tests, no snapshot drift; serde derives were additive |
| SC-007 | Zero warnings under `-D warnings` | PASS for host build (`RUSTFLAGS="-D warnings" cargo build --release` + full `cargo test` clean). One `unreachable_pub` warning surfaced on the WASM target for the wasm-bindgen items in the nested `mod wasm`; suppressed via `#[allow(unreachable_pub)]` with rationale (wasm-bindgen exports them via macro attrs regardless of Rust visibility) |
| SC-008 | Manual test procedure documented + executable | PROCEDURE WRITTEN (`quickstart.md` 10-step procedure); **EXECUTION DEFERRED to maintainer** |

### Implementation findings

- **`gen_traces` requires a real `main()` from the start**: the empty placeholder in T003 broke cargo build because the bin target referenced an empty file. Quick fix: added a stub `fn main() { eprintln!("placeholder") }`. Real impl arrived in T013.
- **WASM unreachable_pub warning**: the `#[wasm_bindgen]`-decorated items inside `mod wasm { ... }` are `pub` from Rust's POV but the parent `mod wasm` is non-pub, so the lint flags them as unreachable. wasm-bindgen exports them via the macro attrs regardless. Resolved with a single `#[allow(unreachable_pub)]` on the `mod wasm` line. Alternative would've been `pub mod wasm`, but the inner items shouldn't be importable as `rustviz::wasm::Player` from non-WASM code — they only exist in WASM builds via the cfg gate.
- **Serde additive derives confirmed safe**: derives added to `Span`, `FileId`, `Ty`, `MemEvent`, `Value`, `NoteKind`, `Pointee`, `SlotId`, `FrameId`, `HeapAddr`, `BorrowId`. M01-M03 snapshot tests show no Debug-output drift; passes byte-identically.
- **Bundle size**: WASM `rustviz.wasm` is 239 KB raw, **78 KB gzipped**. With CodeMirror loaded via esm.sh (not bundled), the project-served assets (HTML + CSS + JS + WASM) total under 100 KB gzipped. SC-005's 2 MB target has substantial headroom even after CodeMirror lands.
- **Per-sample event counts (FR-007 trace-format sanity)**: arithmetic=5, fn_call=13, shadow=8, div_by_zero=2. These match M03's `tests/snapshots/emits_*.snap` event counts — the M03 evaluator behaves identically when driven by `gen_traces` as by the M03 integration test driver.
- **CodeMirror via esm.sh CDN**: per R-012, no JS bundler. `web/index.js` imports from `https://esm.sh/codemirror@6.0.1`, `@codemirror/state@6.4.1`, `@codemirror/view@6.26.3`, `@codemirror/lang-rust@6.0.1`. First page load fetches from CDN; subsequent loads benefit from CDN caching. For offline / air-gapped deploys, the maintainer can vendor the bundles later.
- **15 lib unit tests pass** (6 event-variant smoke tests from M03 + 9 cursor tests from M04). All deterministic, all sub-second.

### AI-implementer limitations

- The AI implementer can build the WASM, run all cargo tests, run gen_traces, and verify the JSON trace outputs. The AI **cannot** visually verify the rendered browser page — that requires a human looking at the DOM.
- **Maintainer-side QA** (per the project's UI QA-split feedback memory): `cargo install trunk` if not already; then `cd web && trunk serve --open`. Walk through the 10-step procedure in `quickstart.md` (load → Play → Rewind → Step Back/Forward → switch samples → confirm runtime-error sample shows the status message).
- If the visual QA reveals a bug, treat it as a normal bug-fix turn — diagnose, patch, re-run code-side checks, re-signal ready.

### Test summary

```
$ RUSTFLAGS="-D warnings" cargo test
running 47 tests
- m01: 8 passed
- m02: 16 passed
- m03: 8 passed
- lib (event + ui): 15 passed (6 event smoke + 9 cursor)
total: 47 passed; 0 failed; 0 ignored
```

```
$ cargo build --release --target wasm32-unknown-unknown
Finished `release` profile [optimized] target(s) in ~15s
WASM: 239 KB raw / 78 KB gzipped
```

### Conclusion

M04 code-side complete. **Shipping for QA.** Maintainer runs `cd web && trunk serve --open` and walks through `quickstart.md` SC-008 procedure. M05 (live editing — replace pre-recorded traces with live re-runs) can begin once M04 lands on `main`.
