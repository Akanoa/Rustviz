# Research — M04 Implementation Decisions

The chunkiest plan-phase yet — M04 introduces the project's whole web pipeline.

## Editor framework

### R-001 — CodeMirror 6, not Monaco

- **Decision**: CodeMirror 6 with `@codemirror/lang-rust` for syntax highlighting. Loaded via ESM CDN (`esm.sh`) — no bundler step.
- **Rationale**:
  - Bundle size: CodeMirror 6 is ~200 KB minified+gzipped; Monaco is ~5 MB. SC-005 (≤ 2 MB total) effectively requires the lighter option.
  - Decoration API: CM6's `Decoration.mark()` + `StateField` model is exactly the shape we need for span highlighting. We don't need an IDE-grade editor; we need a syntax-highlighted, read-only textbox with one-region decoration.
  - ESM-first: CM6 ships as ES modules; we can import directly from esm.sh in `index.js` with no build step.
  - Future-proof for M05: when live editing arrives, CM6's reactive `EditorView` handles input + decorations cleanly.
- **Alternatives considered**:
  - **Monaco**: nicer out-of-box Rust highlighting and an IntelliSense-style experience, but the bundle weight is unjustifiable for a single read-only editor in M04. Rejected.
  - **Plain `<textarea>` + hand-rolled syntax highlighting**: smallest possible bundle, but we'd hand-roll tokenizer + decoration math. Rejected — the parser already exists in WASM but wiring it to a textarea for highlighting is a rabbit hole.
  - **Ace editor**: similar weight to CodeMirror 6 but smaller ecosystem and less modern API. Rejected.

## UI rendering approach

### R-002 — Vanilla JS + wasm-bindgen

- **Decision**: write the DOM manipulation in a single `web/index.js` using vanilla JS. WASM exports a `Player` class via `wasm-bindgen`; JS calls into it and renders the returned `StateSnapshot` JSON to the DOM.
- **Rationale**:
  - All non-trivial logic (cursor state, state-at-N computation, sample loading) lives in Rust. JS is just the view layer.
  - No framework, no bundler, no transpile step — `trunk serve` + `index.js` + ESM imports from CDN is the whole toolchain.
  - For M04's static rendering needs (a few DOM nodes per state update), vanilla `document.createElement` + `replaceChildren` is fine. We're not rendering thousands of components.
  - M05+ may want a framework if interactivity grows; M04 doesn't justify one yet.
- **Alternatives considered**:
  - **Yew / Sycamore / Leptos**: write the UI in Rust → WASM. Tempting for one-language consistency but bloats WASM size and complicates CodeMirror integration (JS lib needs wasm-bindgen wrappers). Rejected for M04.
  - **Solid.js / Svelte / Preact**: small reactive framework. Real upside but requires a bundler (vite / esbuild) — new toolchain. Defer to a future milestone if M07/M08 hit complexity walls.
  - **React**: too heavy for M04's needs. Rejected.

## Build tooling

### R-003 — Trunk for WASM build + dev server

- **Decision**: use [`trunk`](https://trunkrs.dev) (installed via `cargo install trunk`) as the WASM bundler + dev server. `trunk serve --open` is the demo command (FR-013).
- **Rationale**:
  - One command: builds the WASM, copies assets, starts a local server with hot reload.
  - First-class `wasm-bindgen` integration — handles the JS glue file generation automatically.
  - `Trunk.toml` supports pre-build hooks, which we use to run `cargo run --bin gen_traces` before each build to refresh traces.
  - Minimal config: a single `Trunk.toml` plus `data-trunk` attributes in `index.html` is enough.
  - Active project, well-documented, common for "Rust → simple web" use cases.
- **Alternatives considered**:
  - **wasm-pack**: lower-level, outputs an npm-style package. Need a separate server (webpack-dev-server / vite). More moving parts.
  - **wasm-bindgen-cli directly**: lowest level, manage assets and serving manually. Tedious.
  - **Just `cargo build --target wasm32-unknown-unknown` + a Python http.server**: works but no auto-rebuild, no asset copying, no JS glue management. Rejected.

## Trace serialization

### R-004 — JSON via `serde` + `serde_json` (with derived `Serialize` + `Deserialize` on event types)

- **Decision**: traces are serialized as JSON via `serde_json`. Add `serde = { version = "1", features = ["derive"] }` and `serde_json = "1"` as regular crate dependencies. Derive `Serialize` + `Deserialize` on `MemEvent`, `Value`, `NoteKind`, `Pointee`, `SlotId`, `FrameId`, `HeapAddr`, `BorrowId`, `Ty` (from M02), and `Span` / `FileId` (from M01).
- **Rationale**:
  - JSON is the lingua franca between WASM and the page. The JS side can parse traces or inspect them in browser devtools.
  - `serde` is the standard Rust serialization framework, WASM-portable, widely audited.
  - Adding the derives on M01/M02/M03 types is **additive** per their contract stability rules — existing consumers' Debug-snapshot tests don't change.
  - JSON traces are human-readable, which helps debugging during M04 development.
  - The "deps when needed" project preference (saved memory) explicitly permits this.
- **Alternatives considered**:
  - **`postcard` (binary)**: smaller traces but opaque to debugging. Rejected for M04 — readability wins.
  - **Hand-rolled JSON via custom `to_json()` methods**: avoid adding `serde`. Adds ~200 LOC of fragile serialization code. Rejected — the dep is well worth it.
  - **`bincode`, `messagepack`, etc.**: same downsides as postcard.

### R-004.1 — `Span` and `Ty` get `Serialize` + `Deserialize`, not just `Serialize`

- **Decision**: both directions on the M01/M02 reused types. The WASM `Player::new(json)` constructor deserializes a trace at runtime.
- **Rationale**: M04 architecture has WASM owning the trace + cursor state. JS hands WASM the JSON string at startup; WASM deserializes into `Vec<MemEvent>` and answers cursor commands by returning state-snapshot JSON. Two-way serde on the trace types lets this work.
- **Alternatives considered**:
  - **JS owns the trace**: JS parses JSON to JS objects, calls WASM with raw cursor commands, WASM computes state without seeing events directly. Splits cursor logic across two languages. Rejected.
  - **Only `Serialize`, no `Deserialize`**: WASM produces traces (via `gen_traces` bin) but can't read them. Would require the cursor logic in JS. Rejected for the same reason.

## Module layout

### R-005 — Single crate, dual crate-type

- **Decision**: `[lib] crate-type = ["cdylib", "rlib"]` in `Cargo.toml`. The same crate compiles to a normal library (for tests, `gen_traces` bin) and to WASM (for the browser).
- **Rationale**:
  - Avoids the complexity of a Cargo workspace for M04. We can introduce a workspace later if WASM-only deps start polluting host builds (e.g. if `web-sys` features get out of hand).
  - The WASM-specific bindings (`#[wasm_bindgen]` exports) live in `src/ui.rs` behind `#[cfg(target_arch = "wasm32")]` so non-WASM builds don't pull in JS-glue code at runtime.
  - The pure-Rust `Cursor` + state-snapshot logic lives in the same file but is NOT cfg-gated — it's testable via `cargo test --lib` on any target.
- **Alternatives considered**:
  - **Workspace with `rustviz` (lib) + `rustviz-web` (WASM bin)**: cleaner separation but more files. Defer until M05+ if WASM-only deps cause issues.
  - **Separate `src/wasm/` module tree**: doesn't add clarity over flat `ui.rs`. Rejected.

### R-006 — `src/ui.rs` flat file, not a directory

- **Decision**: single `src/ui.rs` file containing the `Cursor`, `StateSnapshot`/`FrameCardView`/`SlotRowView` view types, and the `#[wasm_bindgen] Player` exports. Estimated ~400–600 LOC; flat is sufficient.
- **Rationale**: same convention as M02 (`resolve.rs` + `typeck.rs`) and M03 (`event.rs` + `eval.rs`) — flat until it's too big. M04's UI logic isn't algorithmically complex; the cursor is a 50-line state machine and the view types are data classes.

## Cursor + state-at-N

### R-007 — `Cursor` is `{ trace: Vec<MemEvent>, position: usize }`

- **Decision**: a `Cursor` holds the full event vector and an integer position `0 ≤ position ≤ trace.len()`. State at position `N` is computed by replaying events `[0..N)` over an empty `World` (frames + slot table).
- **Rationale**:
  - L1 traces are tiny (< 100 events typically); O(N) replay per cursor move is irrelevant.
  - Simpler than maintaining incremental forward/backward state diffs.
  - The replay function is the same logic the UI rendered on first viewing — only place that knows how `FrameEnter`/`SlotAlloc` etc. mutate visible state.
- **Alternatives considered**:
  - **Persistent (immutable) state with structural sharing**: O(1) cursor moves, but L1 doesn't need the perf. Rejected.
  - **Incremental forward + reverse diffs**: complex, error-prone — would need a "reverse event" type for every forward event. Rejected.

### R-008 — Stop-at-runtime-error semantics

- **Decision**: if the trace's last event is a `MemEvent::Note { kind: NoteKind::RuntimeError, ... }`, advancing past it is a no-op (the cursor cannot move past the error). State-at-error-step shows the runtime-error message in the status area.
- **Rationale**: M03 emits the note then halts; M04's player respects that — stepping past the error wouldn't reveal anything new since no events follow.

## Snapshot view types

### R-009 — `StateSnapshot` is serializable, opaque, future-additive

- **Decision**: define `StateSnapshot { frames: Vec<FrameCardView>, editor_highlight: Option<Span>, status: Option<StatusView> }`. Serialized to JSON via serde; passed across the WASM boundary as a JSON string.
- **Rationale**: stable, additive type for the JS renderer. M07's heap panel will add a `heap: Vec<HeapBoxView>` field; M06 will add `arrows: Vec<ArrowView>`. M04's snapshot type is intentionally a strict subset — JS handles the L1 fields and ignores future fields it doesn't know about.

### R-010 — Pre-write slot values shown as `None` (placeholder `?`)

- **Decision**: a slot that has had `SlotAlloc` but not yet `SlotWrite` has `value: None` in the `SlotRowView`. The JS renderer displays this as `?` or a similar placeholder.
- **Rationale**: pedagogically honest. The slot exists, but the value is "yet to be written". The two-phase nature of let-binding (alloc, then write) is part of what M04 visualizes.

## Web asset layout

### R-011 — `web/` at repo root, gitignored `traces/`

- **Decision**: `web/index.html`, `web/index.js`, `web/style.css`, `web/samples/*.rs`, `web/traces/*.json` (gitignored). Trunk's working directory is `web/`.
- **Rationale**:
  - Clear separation: anything under `web/` is a web asset.
  - `samples/` are inputs (checked in); `traces/` are outputs of `gen_traces` (regenerated; gitignored).
  - Trunk's project-detection looks for `index.html`; placing it in `web/` keeps the repo root tidy.

### R-012 — CodeMirror via ESM CDN, not bundled

- **Decision**: `index.js` imports CodeMirror packages from `https://esm.sh/codemirror@6` (and friends). No npm, no bundler, no `node_modules`.
- **Rationale**:
  - Zero JS build step. The whole toolchain is `cargo` + `trunk` + `cargo install trunk` upfront.
  - esm.sh is reliable, widely used, fast (CDN-cached).
  - For production deployment, vendoring is possible later — not M04's concern.
- **Alternatives considered**:
  - **Vendor a CodeMirror bundle**: produce a single-file build via esbuild offline, check it in. Adds an offline step. Defer to deployment hardening.
  - **npm + esbuild**: introduces a JS build chain. Rejected for M04 simplicity.

## Trace generation

### R-013 — `cargo run --bin gen_traces` produces `web/traces/*.json` from `web/samples/*.rs`

- **Decision**: a small binary `src/bin/gen_traces.rs` reads each `web/samples/m03_*.rs`, runs the M01→M02→M03 pipeline, and writes a JSON file per sample to `web/traces/`. The JSON shape is `{ "source": "<rs text>", "events": [<MemEvent>, ...] }`. Trunk runs this as a pre-build hook.
- **Rationale**:
  - Pre-build hook means dev iteration "edit a sample" → save → trunk picks up → re-runs gen_traces → reloads page. Smooth.
  - `gen_traces` is a binary, not a build script, so it doesn't have build-script lifecycle weirdness.
  - JSON files are human-readable for debugging.

## Browser support and limitations

### R-014 — UTF-8 byte spans vs character positions

- **Decision**: CodeMirror 6 indexes by JavaScript string position (UTF-16 code unit). Rust spans are byte offsets into UTF-8 source. For pure-ASCII L1 samples, the two coincide. For samples with non-ASCII characters (e.g. comments with emoji), they'd differ.
- **Rationale**: L1 samples are ASCII by convention. M04 doesn't need to convert. Note this as a known limitation; revisit if non-ASCII enters L1+.
- **Alternatives considered**:
  - **Convert byte → UTF-16 position at JS boundary**: doable but adds code; YAGNI for M04. Rejected.

### R-015 — Auto-play rate is a fixed constant

- **Decision**: auto-play advances 1 event per 400 ms. Configurable rate is out of scope.
- **Rationale**: spec FR-006 says "approximately 1 event per 300–500 ms"; 400 ms is the midpoint. Configurability adds UI surface for no clear pedagogical win.

## Constitution

### R-016 — Same vacuous PASS

- **Decision**: `.specify/memory/constitution.md` still unfilled. No gates apply.

## Open questions — not blocking

- **Toolbar visual design**: button icons (svg) vs text labels (`Play`, `Pause`, `Step ▶`). Defer to implementation; spec doesn't pin it.
- **Frame card collapsed view for deep recursion**: spec edge case says "scrolls if exceeds viewport". M04 takes the simple scroll approach; later milestones may add an outline view.
- **Color scheme**: light mode by default. Dark mode is post-M04.
- **i18n**: English-only for M04. Spec doesn't require otherwise.
