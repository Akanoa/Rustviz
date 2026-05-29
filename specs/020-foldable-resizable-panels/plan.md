# Implementation Plan: Foldable & resizable panels

**Branch**: `020-foldable-resizable-panels` | **Date**: 2026-05-29 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/020-foldable-resizable-panels/spec.md`

## Summary

Add user-controlled fold + drag-resize to the page's five horizontal panels (Editor / Stacks / Heap / VTABLES / Static). Build on the existing auto-collapse polish (commit `1d8a9e4`) by promoting it to a full UI-shell affordance: per-panel fold button collapses to a 28px sliver; drag handles between adjacent panels resize live; layout state persists in `localStorage`; reset-to-defaults escape hatch in the header. Editor auto-unfolds when a parse error fires.

**Pure UI-shell feature** — no protocol changes, no eval-side changes, no new `MemEvent` / `Ty` / `Value` variants. Touches `web/index.html` + `web/index.js` + `web/style.css` only. WASM stays byte-identical.

Authority chain: spec.md (this feature) → this plan. No MILESTONES.md entry (UX iteration, not a Rust-mechanism milestone).

## Technical Context

**Language/Version**: HTML5 / ES2022 / modern CSS (custom properties, container queries not needed). No Rust changes.
**Primary Dependencies**: existing only — `@codemirror/{state,view,language,lang-rust,commands}` imported via the existing import map; vanilla DOM + `localStorage`. **No new Rust deps**. **No new JS deps**.
**Storage**: `localStorage` — single key `rustviz.panel-layout.v1`, JSON-serialized. Schema-versioned (the `v1` suffix + an inner `version: 1` field) so future iterations can detect old shapes and fall back to defaults.
**Testing**: existing `cargo test` continues to pass (181 baseline; UI-shell changes don't touch the event protocol — bytewise-identical). New manual QA per the quickstart procedure. **No new Rust tests required** (no Rust code changes).
**Target Platform**: same as M01–M08 (host + `wasm32-unknown-unknown` for the WASM build, modern desktop browsers ≥ 1024px wide).
**Project Type**: Rust library + companion UI. Touches **only** `web/` files; src/ untouched.
**Performance Goals**: fold ≤ 100ms; drag resize is live (every mousemove updates flex-basis); reset ≤ 100ms. No re-render of the stacks/heap snapshot needed during resize — CSS handles the layout reflow.
**Constraints**: WASM bundle ≤ +3% vs post-M08 polish baseline (~440 KB → ≤ ~453 KB) per SC-008. Zero new Rust warnings (SC-009). M01-M08 samples render correctly at default layout (SC-005). Layout state restored within 1px tolerance on reload (SC-003).
**Scale/Scope**: ~200-300 LOC across `index.html` + `index.js` + `style.css`. **Sized S** per the project rubric — comparable to or smaller than the post-M08 auto-collapse polish.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

`.specify/memory/constitution.md` is still the unfilled speckit template. Same vacuous PASS as features 001–019.

**Post-design re-check**: still vacuous, still PASS.

## Project Structure

### Documentation (this feature)

```text
specs/020-foldable-resizable-panels/
├── plan.md                         # This file
├── spec.md                         # Feature spec
├── research.md                     # Phase 0: 8 design decisions
├── data-model.md                   # Phase 1: PanelState, LayoutState schema, localStorage key
├── quickstart.md                   # Phase 1: dev workflow + manual QA procedure
├── contracts/
│   └── layout-storage-schema.md    # Phase 1: localStorage schema + versioning contract
└── checklists/
    └── requirements.md             # From /speckit-specify (16/16 PASS)
```

### Source Code (repository root) — files this feature touches

```text
web/
├── index.html              # MODIFIED — wrap each <section> in a panel container with a header bar (fold button + label) so the existing `<section>` content stays inside. Add 4 <div role="separator"> divider elements between the 5 panels. Add a "Reset layout" button to <header>.
├── index.js                # MODIFIED — new `panelLayout` module: load/save state via localStorage, attach fold-button click handlers, attach divider mousedown/mousemove/mouseup handlers for live drag-resize, expose `unfoldEditorOnError()` for the error path. Hook into existing renderError() to auto-unfold the editor. Wire reset button. ~150-200 LOC.
└── style.css               # MODIFIED — new `.panel` wrapper class (replaces direct flex-basis on <section>); .panel-header with fold button; .panel.is-folded sliver layout (vertical-text label + chevron); .panel-divider with hover highlight + col-resize cursor; .panel-divider.is-dragging body-cursor lock. ~120-150 LOC. Adapt the existing `.panel-empty` rules to be subordinate to user state.

# UNCHANGED:
src/                        # No Rust changes. WASM is byte-identical.
tests/                      # No new tests. 181 existing tests continue to pass.
specs/004-m03-event-eval/contracts/m03-api.md   # No protocol changes; no amendment.
```

**Structure Decision**: minimal UI-shell change. The five `<section>` panels keep their existing IDs (`#editor`, `#stacks`, `#heap`, `#vtables`, `#static`) — JS that queries them by selector still works. The new `.panel` wrapper sits BETWEEN `<main>` and `<section>`; flex behavior moves to `.panel` so `<section>` retains its existing `flex: 1; min-width: 0; overflow: auto;` semantics. The arrow SVG overlay stays in `<main>` and continues to span the full content area.

## Complexity Tracking

> No constitutional violations. Table omitted.

### Notable non-trivial complexity

- **Live drag math**: `mousemove` handler computes delta vs. mouse-down origin, applies `flex-basis: Xpx` (not %) to both adjacent panels during the drag. On `mouseup`, convert px → % of `<main>`'s width for storage (so the layout survives window resize). Uses `getBoundingClientRect()` once at mousedown to capture origin widths; subsequent `mousemove` deltas don't need DOM queries (avoids layout thrash).
- **Body-cursor lock during drag**: a divider drag must keep `col-resize` cursor visible even when the mouse strays off the divider (otherwise the cursor flickers between `col-resize` and `default`). Standard trick: add `body { cursor: col-resize !important; user-select: none; }` via a class while a drag is active.
- **Pointer capture**: use `pointerdown` + `setPointerCapture(pointerId)` so the drag continues even if the cursor leaves the divider element. `pointerup` releases.
- **Persistence schema versioning**: include `version: 1` in the saved JSON. On load, if the parsed shape doesn't match (or `version !== 1`), discard and use defaults. Logged once to `console.warn`.
- **`localStorage` not available** (private mode, disabled): try/catch the load + save; fall back to in-memory state. Layout doesn't persist across reload in that case but the feature still works.
- **Auto-collapse heuristic interaction**: the existing `.panel-empty` class is set by `renderUi()` based on snapshot content. With this feature, that class becomes a HINT, not an override. New rule: if the user has explicitly unfolded a panel, ignore `.panel-empty`. Tracked via a per-panel `userOverride: bool` flag in the in-memory state — cleared on reset.
- **Editor auto-unfold on parse error**: `renderError()` calls `panelLayout.unfoldEditorIfFolded()` which clears the editor's `is-folded` state AND clears its `userOverride` flag so the user's prior fold preference is respected when the next successful parse happens (the editor re-folds on the next clean parse if the user had folded it before the error).
- **Fold button placement**: a small ✕ / `−` icon-only button on the right edge of the panel header. When folded, becomes a chevron-only sliver-wide button. Plan-phase: use simple Unicode glyphs (`−` for fold, `›` for unfold-from-left, `‹` for unfold-from-right) to keep CSS minimal.
- **Reset button placement**: in `<header>`, next to the "Sample:" label, before the sample dropdown. Small button with a "⟲ Reset layout" label or similar.
- **Default widths**: the spec recommends Editor 25% / Stacks 30% / Heap 25% / VTABLES 10% / Static 10%, but the CURRENT layout has Editor and Stacks at `flex: 1` (so equal-share with Heap which is also `flex: 1`; VTABLES `flex: 0.6`; STATIC `flex: 0.6`). The plan keeps the existing computed-default behavior (don't bake in fixed percentages) — defaults = "no explicit flex-basis, use the existing CSS flex rules". The reset button removes the inline styles, falling back to the CSS defaults.
- **Minimum width enforcement**: drag handler clamps to a 120px floor. If the user drags below that, the divider snaps back at 120px during drag; on release, if intent was clearly "fold this panel" (e.g. dragged the divider all the way to one side), the adjacent panel auto-folds. Plan-phase recommendation: simple clamp-only behavior for v1 (no auto-fold on under-shrink — the user can fold manually).
- **The four divider elements**: inserted between panels at positions [0-1], [1-2], [2-3], [3-4]. When the panel on either side of a divider is folded, the divider is `display: none` (visually absent + non-interactive). Cleaner than the spec's "disabled or unfold" deferral.
- **Storage budget**: a layout state is ~5 entries × ~30 bytes = ~150 bytes JSON-serialized. Negligible.
- **No re-render of WASM-driven UI during drag**: the snapshot is unchanged during drag. Only CSS is mutated. The frame card slots, slot rows, thread columns, etc. all re-flow via CSS — no JS state change required.
- **Animation**: per the spec assumption, instant snap is acceptable. Plan adds `transition: flex-basis 120ms ease-out` to `.panel:not(.is-dragging)`, removed via `is-dragging` class during a drag. So drag is live (no transition), fold/unfold + reset are animated.
