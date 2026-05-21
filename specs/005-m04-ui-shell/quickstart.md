# Quickstart — M04 development + manual test

Audience: maintainer + contributors working inside M04 or extending the UI in M05+.

## One-time setup

```bash
# Trunk: WASM bundler + dev server.
cargo install trunk

# wasm32 target (if not already installed).
rustup target add wasm32-unknown-unknown
```

## Run the page

From the repo root:

```bash
cd web
trunk serve --open
```

This:
1. Runs the pre-build hook: `cargo run --bin gen_traces` regenerates `web/traces/*.json`.
2. Compiles the Rust library to `wasm32-unknown-unknown`.
3. Generates the JS glue via wasm-bindgen.
4. Serves the page at `http://localhost:8080` and opens the default browser.

Edit any file under `web/` or `src/` → trunk rebuilds and reloads.

## Regenerate traces manually

```bash
cargo run --release --bin gen_traces
```

Run this whenever you add a new sample to `web/samples/m03_*.rs` or change M01-M03 in a way that affects event emission.

## Run the cursor unit tests

```bash
# State-at-N logic, deterministic + table-driven (no browser needed).
cargo test --lib ui::

# M01 + M02 + M03 regression — must stay green (SC-006).
cargo test --test m01
cargo test --test m02
cargo test --test m03
```

## Add a new sample

1. Create `web/samples/m03_<name>.rs` (a valid L1 program — must pass M01 → M02 → M03 without static errors).
2. Append `"m03_<name>"` to the sample list in `src/bin/gen_traces.rs`.
3. Re-run `cargo run --bin gen_traces` (or restart `trunk serve`, which runs it via the pre-build hook).
4. Append a `<option>` entry for the new sample in `web/index.html`'s sample-selector dropdown.
5. Reload the page; the new sample is selectable.

## Manual test procedure (SC-008)

A short procedure to verify M04 didn't regress. Run after every change to `src/ui.rs`, `web/index.js`, or `web/index.html`:

1. `cd web && trunk serve --open` — page should load within 3 seconds.
2. **Default sample loaded**: the editor shows the `m03_arithmetic` (or first-listed) sample.
3. **Click Play**: cursor advances visibly. Within 5 seconds the trace finishes (auto-pause at end).
4. **Click Rewind**: panels reset to empty, position indicator shows `0 / <total>`.
5. **Click Step Forward 3 times**: editor highlight moves, stacks panel shows partial state.
6. **Click Step Back 2 times**: panels back-track. Visual state at step 1 must match what you saw earlier at step 1.
7. **Switch sample** via dropdown to `m03_fn_call`: editor source changes, stacks empty, cursor at 0.
8. **Click Play on the fn_call sample**: observe a second frame card appear (for `add`) stacking above `main`, then disappear at return.
9. **Switch to `m03_div_by_zero`** and Step Forward through it. On reaching the `Note` event, status area shows `"division by zero"` in error styling. Step Forward is a no-op past that.
10. Switch back to `m03_arithmetic` and click Rewind + Play. Page still responsive.

If any step fails, the regression is in M04 (or in M03 if event order changed). Compare to a known-good screen recording from the audit log.

## Use the WASM API from JS (if extending in M05+)

```javascript
import init, { Player } from "./pkg/rustviz.js"; // trunk emits this

await init();

const traceJson = await fetch("traces/m03_arithmetic.json").then(r => r.text());
const player = new Player(traceJson);

// Pull initial state
let state = JSON.parse(player.state());
console.log(state); // { frames: [], editor_highlight: null, status: null, position: 0, total: 13 }

// Step forward
state = JSON.parse(player.step_forward());

// Rewind
state = JSON.parse(player.rewind());
```

## File-system map (post-M04)

```
src/
├── ui.rs              ← M04: Cursor, StateSnapshot views, wasm-bindgen Player
├── bin/gen_traces.rs  ← M04: produces web/traces/*.json
├── event.rs / eval.rs / typeck.rs / resolve.rs / parse.rs / parse/  ← M01-M03 (unchanged code, additive serde derives)
└── lib.rs             ← re-exports updated

web/
├── index.html         ← 3-panel layout + toolbar
├── index.js           ← DOM + CodeMirror + WASM glue
├── style.css          ← layout + theming
├── samples/m03_*.rs   ← checked in
└── traces/*.json      ← gitignored, generated

Trunk.toml             ← trunk config + pre-build hook
.gitignore             ← gains `web/traces/`
```

## Implementer notes (for M05+)

When extending the UI in later milestones:

- **M05 (live editing)**: add a `Player::set_source(&str)` method that re-runs the M01-M03 pipeline on the new source and resets the cursor. Switch the CodeMirror editor from read-only to editable.
- **M06 (borrow arrows)**: extend `StateSnapshot` with `arrows: Vec<ArrowView>`. Add an SVG overlay layer over the 3-panel layout that draws arrows from slot anchors to slot anchors.
- **M07 (heap)**: extend `StateSnapshot` with `heap: Vec<HeapBoxView>`. The reserved heap region in `index.html` becomes active. Add animation when a heap box's position changes (CSS transitions on transform).
- **M08 (multi-thread)**: extend `StateSnapshot.frames` to be `Vec<ThreadColumnView>` where each thread has its own stack of frame cards. Layout becomes columnar.

Each extension is intended to be **additive** on top of M04's layout, not a rewrite.

## Limitations to revisit later

- **UTF-8 vs UTF-16 span positions**: Rust spans are UTF-8 byte offsets; CodeMirror works in UTF-16 code units. For ASCII samples (current L1) these coincide. If non-ASCII enters samples, add a conversion at the JS boundary.
- **CDN dependency**: CodeMirror loaded from `esm.sh`. If you need offline development or air-gapped builds, vendor the CodeMirror bundle.
- **Bundle size**: M04's WASM + JS + CSS should be ≤ 2 MB gzipped (SC-005). If it grows past, profile (likely culprits: wasm-bindgen + serde_json runtime size).
- **No automated browser tests**: regressions in DOM rendering need manual testing. Consider Playwright in a future infrastructure milestone.
