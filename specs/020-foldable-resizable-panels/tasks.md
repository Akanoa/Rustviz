---

description: "Task list for foldable & resizable panels — pure UI-shell feature"
---

# Tasks: Foldable & resizable panels

**Input**: Design documents from `/specs/020-foldable-resizable-panels/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/layout-storage-schema.md ✓, quickstart.md ✓

**Tests**: No Rust code changes — 181 existing tests must continue to pass byte-identical (UI-shell changes don't touch the event protocol). No new Rust tests required. Manual QA per `quickstart.md` covers end-to-end verification across all 3 user stories + edge cases.

**Organization**: 3 user stories (US1+US2 P1 foundational; US3 P2 escape-hatch reset). Sized S — UI-shell only, ~200-300 LOC across `web/index.html` + `web/index.js` + `web/style.css`.

**No UX checkpoint expected**: fold + drag are standard web idioms. If the initial visual cut reveals ambiguity (sliver design, button placement, divider hit-target), a checkpoint may be added between Phase 3 and Phase 4 by user request.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1/US2/US3 tag, mandatory on user-story phases only
- File paths relative to repo root

## Path Conventions

3 files touched: `web/index.html` (DOM structure for `.panel` wrappers + dividers + Reset button), `web/index.js` (new `panelLayout` module — load/save/fold-handlers/drag-handlers/reset), `web/style.css` (new `.panel*` rules). `src/` untouched — WASM byte-identical.

---

## Phase 1: Setup

**Purpose**: pre-flight — confirm starting state.

- [X] T001 Verify pre-conditions: branch `020-foldable-resizable-panels` checked out; `cargo test` from `main` passes (baseline 181 tests post-M08 polish); WASM bundle baseline ~440 KB noted; `localStorage` is empty (no pre-existing `rustviz.panel-layout.v1` key in the dev browser).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: DOM structure (`.panel` wrappers + dividers + Reset button) and the `panelLayout` JS module skeleton (load/save/defaults/storage-key constant) — required by all user stories. Once Phase 2 lands, the page renders identically to today; nothing is folded or resizable yet, but the scaffolding is in place.

- [X] T002 In `web/index.html`, wrap each of the five `<section id="…">` panels in a `<div class="panel" data-panel="<id>"><div class="panel-header"><span class="panel-title">…</span><button class="panel-fold-btn" aria-expanded="true" aria-label="Fold panel">−</button></div><section id="…">…</section></div>` structure. Insert four `<div class="panel-divider" role="separator" aria-orientation="vertical" data-divider-between="left,right" tabindex="-1"></div>` divider elements between panels. The `#arrow-overlay` SVG stays as the last child of `<main>` so it spans all panels. Panel-title labels: `EDITOR`, `STACKS`, `HEAP`, `VTABLES`, `STATIC`.

- [X] T003 In `web/index.html`, add a `<button id="btn-reset-layout" type="button" aria-label="Reset panel layout">⟲ Reset</button>` to `<header>`, placed BEFORE the `<label for="sample-selector">` so it sits at the left edge of the header.

- [X] T004 In `web/style.css`, add the new `.panel` flex rules:
  - `.panel { display: flex; flex-direction: column; min-width: 0; overflow: hidden; border-right: 1px solid var(--border); transition: flex-basis 120ms ease-out; }`
  - `.panel:last-child { border-right: none; }`
  - `.panel > section { flex: 1; min-width: 0; min-height: 0; overflow: auto; border-right: none; }` (override the legacy `main > section` rule for sections INSIDE a `.panel` wrapper — keeps non-wrapped sections, if any, on legacy behavior).
  - Initial flex-basis: `.panel[data-panel="editor"] { flex: 0 0 25%; }`, stacks `30%`, heap `25%`, vtables `10%`, static `10%`. Sums to 100%.
  - `.panel-header { display: flex; align-items: center; justify-content: space-between; padding: 0.3rem 0.5rem; font-size: 11px; color: var(--muted); text-transform: uppercase; letter-spacing: 0.5px; border-bottom: 1px solid var(--border); }`
  - `.panel-title { font-family: ui-monospace, SFMono-Regular, monospace; }`
  - `.panel-fold-btn { background: transparent; border: 1px solid transparent; color: var(--muted); cursor: pointer; padding: 1px 6px; border-radius: 3px; font-size: 12px; line-height: 1; }`
  - `.panel-fold-btn:hover { background: var(--frame-bg); color: var(--text); border-color: var(--border); }`
  - Style the Reset button: `#btn-reset-layout { background: transparent; border: 1px solid var(--border); border-radius: 3px; padding: 2px 8px; font-size: 11px; color: var(--muted); cursor: pointer; margin-right: 0.6rem; } #btn-reset-layout:hover { background: var(--frame-bg); color: var(--text); }`

- [X] T005 [P] In `web/style.css`, override the legacy `main > section { flex: 1; min-width: 0; overflow: auto; border-right: 1px solid var(--border); }` rule to be scoped to direct children of `<main>` that are NOT `.panel` wrappers: `main > section:not(.panel > section)` (or restructure — sections are no longer direct children of `<main>`, so the rule no-ops naturally; verify and remove if dead). Also remove or scope the legacy per-panel flex values (`#editor`, `#stacks`, `#heap`, `#static`, `#vtables`) so the new `.panel` wrapper's flex rules drive the layout.

- [X] T006 In `web/index.js`, add the `panelLayout` module skeleton near the top of the file (after the import map / WASM init):
  - `const STORAGE_KEY = "rustviz.panel-layout.v1";`
  - `const DEFAULTS = { version: 1, panels: { editor: { folded: false, width_pct: 25, user_override: false }, stacks: { folded: false, width_pct: 30, user_override: false }, heap: { folded: false, width_pct: 25, user_override: false }, vtables: { folded: false, width_pct: 10, user_override: false }, static: { folded: false, width_pct: 10, user_override: false } } };`
  - `const MIN_WIDTH_PX = 120;`
  - `let layoutState = loadLayout();` (in-memory copy).
  - `function loadLayout()` — try/catch JSON.parse from localStorage. On any failure (missing, malformed, `version !== 1`), return `structuredClone(DEFAULTS)`. Log a single `console.warn` on parse failure (not on missing-key).
  - `function saveLayout()` — try/catch JSON.stringify + `localStorage.setItem`. Silent failure.
  - `function clampWidth(pct)` — `Math.max(5, Math.min(95, pct))`.
  - `function getPanelEl(id)` — `document.querySelector(\`.panel[data-panel="\${id}"]\`)`.
  - Defer fold-button + divider + reset wiring to user-story phases.

- [X] T007 In `web/index.js`, add `function applyLayoutState()` that walks `layoutState.panels` and applies the persisted state to the DOM:
  - For each panel id: get the `.panel` element. Apply `style.flexBasis = state.width_pct + "%"`. Toggle `.is-folded` class on `folded === true`. Toggle `.is-user-overridden` class on `user_override === true`. Update the fold button's `aria-expanded` accordingly.
  - Call `applyLayoutState()` once at startup AFTER the initial WASM render.

- [X] T008 [P] Run `cargo test` and verify all 181 baseline tests still pass byte-identical (zero changes to Rust source — this is a hygiene check). Run `cd web && trunk build` and verify the dev build succeeds. Load the page and confirm: the layout renders identically to today's defaults; no JS errors in the console; no visual regressions in M01-M08 samples.

**Checkpoint**: scaffolding live, behaviorally transparent — page renders identically; layout state is loaded from localStorage but no user controls react yet.

---

## Phase 3: User Story 1 — Fold any panel (Priority: P1)

**Goal**: clicking a fold button collapses the panel to a 28px sliver; clicking the sliver restores it. State persists. Auto-collapse heuristic becomes subordinate to user-override. Editor auto-unfolds on parse error.

**Independent Test**: load any sample, click the fold button on Editor, observe collapse to sliver. Click sliver, observe restore. Reload, observe state persists.

### Implementation

- [X] T009 [US1] In `web/style.css`, add the `.is-folded` sliver styling:
  - `.panel.is-folded { flex: 0 0 28px !important; min-width: 28px; }` (override the inline `flex-basis` set by `applyLayoutState`).
  - `.panel.is-folded .panel-header { writing-mode: vertical-rl; padding: 0.5rem 0; justify-content: flex-start; gap: 0.5rem; border-bottom: none; border-right: 1px dashed var(--border); height: 100%; }` — vertical text, no horizontal border-bottom.
  - `.panel.is-folded .panel-title { transform: rotate(180deg); }` (reads bottom-to-top).
  - `.panel.is-folded > section { display: none; }` — hide content.
  - `.panel.is-folded .panel-fold-btn { transform: rotate(90deg); }` (chevron-like rotation; the `−` glyph reads as `|` rotated, signaling "I'm vertical now").
  - `.panel.is-folded { cursor: pointer; }` — whole sliver is clickable to unfold.

- [X] T010 [US1] In `web/index.js`, wire fold-button click handlers:
  - `document.querySelectorAll(".panel-fold-btn").forEach(btn => btn.addEventListener("click", onFoldClick));`
  - `function onFoldClick(ev) { ev.stopPropagation(); const panelEl = ev.currentTarget.closest(".panel"); const id = panelEl.dataset.panel; if (layoutState.panels[id].folded) unfoldPanel(id); else foldPanel(id); }`
  - `function foldPanel(id)` — capture current rendered width as `width_pct` (via `panelEl.getBoundingClientRect().width / main.getBoundingClientRect().width * 100`), set `folded = true`, clear `user_override = false`, `saveLayout()`, re-`applyLayoutState()`.
  - `function unfoldPanel(id)` — set `folded = false`. If the panel currently carries `.panel-empty` (auto-collapse hint), set `user_override = true`. `saveLayout()`, re-`applyLayoutState()`.

- [X] T011 [US1] In `web/index.js`, attach a click handler on the whole `.panel.is-folded` element so clicking ANYWHERE on the sliver unfolds it (forgiving target):
  - `document.querySelectorAll(".panel").forEach(p => p.addEventListener("click", onSliverClick));`
  - `function onSliverClick(ev) { const panelEl = ev.currentTarget; if (!panelEl.classList.contains("is-folded")) return; const id = panelEl.dataset.panel; unfoldPanel(id); }`
  - Note: the existing `.panel-fold-btn` handler calls `ev.stopPropagation()` to prevent double-fire when clicking the button itself.

- [X] T012 [US1] In `web/style.css`, adapt the existing `.panel-empty` auto-collapse rules to be subordinate to user state. Update `#static.panel-empty, #vtables.panel-empty { ... }` to `.panel.panel-empty:not(.is-folded):not(.is-user-overridden) > section { /* existing collapse styling */ }`. Effectively: auto-collapse fires only when the user hasn't explicitly folded or unfolded.

- [X] T013 [US1] In `web/index.js`, in `renderUi()` (the existing function that toggles `.panel-empty` based on `state.static_region` / `state.vtables` content), apply `.panel-empty` to the `.panel` wrapper (NOT the inner `<section>`) so the new CSS selectors match. Change `staticEl.classList.toggle("panel-empty", ...)` to `staticEl.closest(".panel").classList.toggle("panel-empty", ...)` (and same for vtables).

- [X] T014 [US1] In `web/index.js`, extend `renderError(err)` to call a new `ensureEditorVisible()` function. `function ensureEditorVisible() { const editorPanel = getPanelEl("editor"); if (!editorPanel || !editorPanel.classList.contains("is-folded")) return; editorPanel.classList.remove("is-folded"); /* re-apply width from layoutState; do NOT touch layoutState.panels.editor.folded */ editorPanel.style.flexBasis = layoutState.panels.editor.width_pct + "%"; }`. This transient-unfold doesn't touch persisted state — the next successful parse triggers `applyLayoutState` which re-applies the user's fold preference.

- [X] T015 [US1] In `web/index.js`, ensure `applyLayoutState()` runs at the appropriate moments. It already runs at startup (T007). Also call it after a successful WASM source change so the editor re-folds if the error path transiently unfolded it. The hook point: end of `render(state)` (the existing success-render path).

- [ ] T016 [US1] Manual QA for US1: load any sample. Click Editor's fold button → collapses to sliver with `EDITOR` vertical text. Click sliver → restores to prior width. Reload → state persists. Load `Box (M07)` sample (no static region) → STATIC auto-collapses. Click STATIC sliver → unfolds; `user_override` set. Load `Arithmetic (M03)` sample (also no static region) → STATIC stays unfolded. Edit source to introduce a parse error → Editor auto-unfolds. Fix error → Editor re-folds.

**Checkpoint**: US1 fully functional. Fold/unfold is the foundational UX; users can already manage panel visibility manually. Resize from US2 makes it continuous.

---

## Phase 4: User Story 2 — Drag-resize between adjacent panels (Priority: P1)

**Goal**: dragging a divider between two panels resizes them live. Cursor changes on hover. State persists across reloads. Minimum-width clamp at 120px.

**Independent Test**: hover divider between Editor and Stacks, observe `col-resize` cursor + highlight. Drag rightward, observe live resize. Release, reload, observe widths persist.

### Implementation

- [X] T017 [US2] In `web/style.css`, add divider styling:
  - `.panel-divider { flex: 0 0 6px; cursor: col-resize; background: transparent; position: relative; }` — 6px hit-target.
  - `.panel-divider::before { content: ""; position: absolute; top: 0; bottom: 0; left: 50%; width: 1px; background: var(--border); transform: translateX(-50%); transition: background 120ms ease-out, width 120ms ease-out; }` — 1px center line; visible at rest.
  - `.panel-divider:hover::before, .panel-divider.is-dragging::before { background: var(--accent); width: 3px; }` — thickens + accent color on hover/drag.
  - `body.is-resizing { cursor: col-resize !important; user-select: none; }` — global cursor lock during drag.
  - `body.is-resizing .panel { transition: none !important; }` — suppress flex-basis animation during drag (otherwise live resize feels laggy).

- [X] T018 [US2] In `web/style.css`, hide dividers adjacent to a folded panel:
  - `.panel.is-folded + .panel-divider, .panel-divider:has(+ .panel.is-folded) { display: none; }` — uses `:has()` (modern Chromium/Firefox/Safari ≥ 2023). For broader support, add a JS-driven `.panel-divider.adjacent-folded` class as a fallback applied by `applyLayoutState`.

- [X] T019 [US2] In `web/index.js`, wire divider pointerdown handlers:
  - `document.querySelectorAll(".panel-divider").forEach(div => div.addEventListener("pointerdown", onDividerDown));`
  - `function onDividerDown(ev)` — must:
    - Skip if either adjacent panel is folded (early-return).
    - `ev.preventDefault()` + capture `divider.setPointerCapture(ev.pointerId)`.
    - Read `data-divider-between` → `[leftId, rightId]`.
    - Snapshot starting widths: `startLeftPx = leftPanel.getBoundingClientRect().width`, `startRightPx = rightPanel.getBoundingClientRect().width`, `startX = ev.clientX`.
    - Add `is-dragging` class to the divider; add `is-resizing` class to `<body>`.
    - Store drag context on the divider element (or a module-level let).
    - Attach `pointermove` and `pointerup` listeners on the divider (works because of `setPointerCapture`).

- [X] T020 [US2] In `web/index.js`, implement the `pointermove` handler:
  - `function onDividerMove(ev)` — compute `delta = ev.clientX - startX`. New widths: `leftPx = startLeftPx + delta`, `rightPx = startRightPx - delta`.
  - Clamp: if `leftPx < MIN_WIDTH_PX`, snap `leftPx = MIN_WIDTH_PX, rightPx = startLeftPx + startRightPx - MIN_WIDTH_PX`. Same for `rightPx < MIN_WIDTH_PX` (swap).
  - Apply: `leftPanel.style.flexBasis = leftPx + "px"; rightPanel.style.flexBasis = rightPx + "px";`. (Px during drag is fine — no layout thrash.)

- [X] T021 [US2] In `web/index.js`, implement the `pointerup` handler:
  - `function onDividerUp(ev)` — convert final px widths to pct of `<main>`: `mainPx = mainEl.getBoundingClientRect().width; leftPct = clampWidth(leftPx / mainPx * 100); rightPct = clampWidth(rightPx / mainPx * 100);`.
  - Write to `layoutState.panels[leftId].width_pct` and `layoutState.panels[rightId].width_pct`. `saveLayout()`.
  - Reapply as pct (so subsequent viewport resizes scale): `leftPanel.style.flexBasis = leftPct + "%"; rightPanel.style.flexBasis = rightPct + "%";`.
  - Remove `is-dragging` from divider; remove `is-resizing` from body. Release pointer capture.

- [ ] T022 [US2] Manual QA for US2: hover divider — cursor + highlight visible. Drag right — left panel shrinks, right panel grows, live. Release — state persists across reload (verify the localStorage key). Drag past 120px floor — clamps; doesn't go below. Fold a panel — adjacent divider disappears (per T018).

**Checkpoint**: US2 fully functional. Combined with US1, the user has full manual control over panel visibility AND continuous size.

---

## Phase 5: User Story 3 — Reset layout (Priority: P2)

**Goal**: a single click on `⟲ Reset` in the header clears all saved state and animates panels back to defaults.

**Independent Test**: fold a panel; resize a divider; click `⟲ Reset` — all panels unfold to defaults; layout state cleared.

### Implementation

- [X] T023 [US3] In `web/index.js`, wire the Reset button:
  - `document.getElementById("btn-reset-layout").addEventListener("click", resetLayout);`
  - `function resetLayout()` — clear `localStorage.removeItem(STORAGE_KEY)`. Set `layoutState = structuredClone(DEFAULTS);`. Call `applyLayoutState()`. The existing `.panel` CSS `transition: flex-basis 120ms ease-out` animates the change.

- [ ] T024 [US3] Manual QA for US3: fold Editor; resize Editor/Stacks divider; trigger an Editor parse error to set transient unfold; click Reset. Confirm: all panels at default widths; no folds; no overrides; localStorage key absent (devtools).

**Checkpoint**: US3 fully functional. Single-click escape hatch ready.

---

## Phase 6: Polish & cross-cutting

**Purpose**: snapshot verify, bundle-size check, warnings, manual QA across all stories, doc updates.

- [X] T025 [P] Run `cargo test` and verify all 181 baseline tests still pass byte-identical. UI-shell changes don't touch the event protocol — this is a hygiene check.
- [X] T026 [P] Build WASM release and measure bundle size: `cd web && trunk build --release` (wasm-opt may fail per the pre-existing tooling issue; use the staged size at `dist/.stage/*.wasm`). Compare to the post-M08 polish baseline (~440 KB). Acceptable if ≤ +3% (~453 KB). Expected: identical or near-identical since no Rust changes.
- [X] T027 [P] Run `RUSTFLAGS="-D warnings" cargo build --release`. Should be clean — no Rust changes were made. Hygiene check.
- [ ] T028 Full manual QA per `specs/020-foldable-resizable-panels/quickstart.md` — ~10-minute walk covering all 3 user stories + edge cases + storage failure path + schema-mismatch fallback + regression sweep across M01-M08 samples at default layout.
- [ ] T029 Verify the existing UI overlays still align under various layouts: arrow overlay still tracks slot positions when stacks is resized; vtables dispatch arrow + Arc dashed-purple arrows still hit their targets; tooltips and pending-copy arrow positioning correct.
- [ ] T030 Final commit prep. MR note: "Pure UI-shell feature — no protocol changes, no Rust code changes, WASM byte-identical. Builds on the existing M08 auto-collapse polish (commit `1d8a9e4`) by adding user-controlled fold + drag-resize across all five panels. localStorage persistence with v1 schema. ≤ +3% bundle (zero Rust delta expected). 181 tests continue to pass."

---

## Dependencies

```text
Phase 1 (Setup)
  └─ T001 (verify baseline)

Phase 2 (Foundational) — blocks ALL user stories
  ├─ T002 (HTML restructure — .panel wrappers + dividers + Reset button)
  ├─ T003 (Reset button in header — depends on T002's overall HTML pass)
  ├─ T004 (base .panel CSS — depends on T002 DOM)
  ├─ T005 [P] (legacy CSS cleanup — depends on T002 DOM)
  ├─ T006 (panelLayout JS skeleton — depends on T002 for getPanelEl selectors to work)
  ├─ T007 (applyLayoutState — depends on T006)
  └─ T008 [P] (verify baseline + dev build — depends on T002-T007)

Phase 3 (US1) — depends on Phase 2
  ├─ T009 (.is-folded CSS sliver)
  ├─ T010 (fold-button click handlers — depends on T006 + T007)
  ├─ T011 (sliver click-to-unfold)
  ├─ T012 (panel-empty + user_override CSS subordination)
  ├─ T013 (apply panel-empty to .panel wrapper, not <section>)
  ├─ T014 (ensureEditorVisible — depends on T010)
  ├─ T015 (re-apply layout state at render-end — depends on T007)
  └─ T016 (manual QA US1)

Phase 4 (US2) — depends on Phase 2 (independent of US1's fold logic functionally, but US1 lands first per priority)
  ├─ T017 (divider CSS)
  ├─ T018 (hide divider adjacent to folded panel — uses US1's is-folded class)
  ├─ T019 (pointerdown handler)
  ├─ T020 (pointermove handler — depends on T019)
  ├─ T021 (pointerup handler — depends on T019, T020)
  └─ T022 (manual QA US2)

Phase 5 (US3) — depends on Phases 2-4
  ├─ T023 (Reset button click handler — depends on T003 + T007)
  └─ T024 (manual QA US3)

Phase 6 (Polish) — depends on Phases 3-5
  └─ T025–T030 (test/build/warnings/QA/docs/commit)
```

---

## Parallel execution opportunities

- **Phase 2**: T005 + T008 are file-disjoint or hygiene-only [P].
- **Phase 6**: T025/T026/T027 [P] (independent verification tasks).
- **Across user stories**: US1 and US2 are largely orthogonal — once Phase 2 lands, an implementer could work on US1 fold UX + US2 drag UX in parallel. The plan keeps them sequential by priority but the structural dependency is only "both depend on Phase 2".

---

## Implementation strategy

**MVP scope** = **US1 only** (fold/unfold + state persistence + auto-collapse subordination + editor auto-unfold on error). Lands the foundational fold UX with the localStorage machinery + auto-collapse priority resolution. ~120-150 LOC.

**Incremental delivery**:
1. **MVP (US1)**: Phases 1+2+3. Manual fold/unfold + persistence + auto-collapse subordination + editor error-visibility. Useful by itself.
2. **+US2 (drag-resize)**: Phase 4. Continuous size control between adjacent panels.
3. **+US3 (reset)**: Phase 5. Escape hatch.
4. **+Polish**: Phase 6. Bundle check + manual QA + commit.

**Recommended landing order**: ship all three user stories + polish in one merge. The feature is small (~250 LOC); splitting at US1 alone would leave the resize gap visible to users for a session and isn't worth a separate merge. Single-merge matches the post-M08 polish pattern (commit `1d8a9e4`).

**No UX checkpoint planned**: fold + drag are standard web idioms. If the initial visual cut feels off (sliver too narrow, divider hit-target too thin, Reset button placement off), a checkpoint can be added between Phase 3 and Phase 4 by user request.

**Sequence note**: this is a pure UI-shell feature. After this lands, the next natural piece of work is M08.1 (real Mutex parking + parked-thread visual) — that's a Rust-side milestone with the full speckit workflow.
