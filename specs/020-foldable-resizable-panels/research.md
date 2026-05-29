# Research — Foldable & resizable panels: design decisions

8 decisions covering layout structure, drag/persistence machinery, fold visuals, auto-collapse interaction, and accessibility.

## R-001 — Wrap each `<section>` in a `.panel` container, not retrofit `<section>` itself

- **Decision**: introduce a new `<div class="panel">` between `<main>` and each existing `<section>`. The panel hosts the fold/header bar + the section content. `<section>` keeps its existing id (`#editor`, `#stacks`, …) and existing CSS — selectors that query by id continue to work.
- **Rationale**: minimum-blast-radius change. Existing JS that does `document.getElementById("stacks")` keeps working. The flex behavior moves from `<section>` to `.panel`; `<section>` becomes a content host. Folding is a class change on `.panel`, not `<section>`.
- **Alternatives considered**:
  - Make `<section>` itself the fold container — would require all existing `<section>`-targeting CSS rules to be re-checked for fold-state interaction. Rejected.
  - Use a custom element (`<rv-panel>`) with shadow DOM — overkill for a 200-LOC feature. Rejected.

## R-002 — Resize via pointer events + flex-basis (not grid + CSS variables)

- **Decision**: `<main>` stays `display: flex; flex-direction: row;`. Each `.panel` has a default `flex: 1 1 0` (Editor/Stacks/Heap) or `flex: 0.6 1 0` (VTABLES/Static) — same numbers as the current CSS. Drag updates `flex-basis: Xpx` on the two adjacent panels live during a `pointermove` handler. On `pointerup`, convert `Xpx → Y%` and write to localStorage. CSS `transition: flex-basis 120ms ease-out` for fold/unfold (removed during drag).
- **Why flex over CSS Grid**: the current layout is flex; switching to grid would introduce a layout-mode change that's risky for the existing `<section>`-internal CSS (slot grids, heap-cell strips, etc.). Flex is sufficient and minimal.
- **Why pointer events**: handle mouse + pen + touch uniformly; `setPointerCapture()` keeps the drag alive even if the cursor leaves the divider element (avoids the common "drag escapes the handle" bug).

## R-003 — Persist as percentages, drag in pixels

- **Decision**: persist `width_pct` (0-100, float). Reload reads pct and applies as `flex-basis: <pct>%`. During drag, use pixels for live updates (no conversion overhead per mousemove); convert to pct on `pointerup` for storage.
- **Rationale**: pct survives window resize. The user's stored 35%/30%/25% layout adapts to any viewport. Px-based storage would break on viewport change.

## R-004 — Storage schema: single key, versioned, fallback-safe

- **Decision**: localStorage key `rustviz.panel-layout.v1`. Value: JSON-serialized object:
  ```json
  {
    "version": 1,
    "panels": {
      "editor":  { "folded": false, "width_pct": 25, "user_override": false },
      "stacks":  { "folded": false, "width_pct": 30, "user_override": false },
      "heap":    { "folded": false, "width_pct": 25, "user_override": false },
      "vtables": { "folded": false, "width_pct": 10, "user_override": true },
      "static":  { "folded": false, "width_pct": 10, "user_override": false }
    }
  }
  ```
- **`user_override` flag**: tracks whether the user has explicitly unfolded a panel that the auto-collapse heuristic wants to fold. Persistent across reloads. Cleared by "reset layout".
- **`width_pct`**: the LAST EXPLICIT width when the user dragged or last value before fold. Used to restore on unfold per FR-002. Missing fields fall back to defaults (e.g. a `v1` blob saved before a future enhancement adds a new field still loads correctly).
- **Schema mismatch**: if the parsed JSON's `version !== 1`, discard and use defaults. Log a single `console.warn`.
- **Storage failure**: try/catch the `localStorage.getItem` + `setItem`; on failure, fall back to in-memory state (the feature still works, just doesn't persist).

## R-005 — Fold visuals: 28px sliver with vertical text label + chevron

- **Decision**: a folded panel becomes a 28px-wide vertical strip with:
  1. The panel's name as vertical text (`writing-mode: vertical-rl`) at the top, muted color.
  2. A chevron button (`›` or `‹` depending on which side is "open") at the bottom of the strip, full-width clickable.
- **Whole-sliver click also unfolds**: not just the chevron — to be forgiving. Implemented via a click handler on the sliver element.
- **Width matches the existing auto-collapse sliver** (28px) so the visual idiom is consistent.
- **Why vertical text over rotated text**: `writing-mode` is well-supported and renders cleanly without anti-aliasing artifacts that 90° rotation can introduce.

## R-006 — Auto-collapse heuristic becomes a HINT subordinate to user state

- **Decision**: the existing `.panel-empty` class (set by `renderUi()` based on snapshot's empty `vtables` / `static_region`) continues to be applied. BUT the fold logic now has 3 inputs in priority order:
  1. User-fold-state (from localStorage) — wins.
  2. User-override-state (user explicitly unfolded a panel the auto-collapse wanted to fold) — sticks.
  3. Auto-collapse heuristic (`.panel-empty`) — applies only if neither (1) nor (2) is set.
- **Concrete rule** (CSS-driven):
  - `.panel.is-folded` → user explicitly folded → render as sliver.
  - `.panel.is-user-overridden` → user explicitly unfolded → render full even if `.panel-empty`.
  - `.panel.panel-empty:not(.is-folded):not(.is-user-overridden)` → auto-collapse sliver.
- **Why this order**: matches spec FR-010 (user choice overrides auto-collapse) and Acceptance Scenario US1#4 (explicit unfold sticks).

## R-007 — Editor auto-unfold on parse error

- **Decision**: extend `renderError(err)` (existing JS function called when the WASM pipeline returns a `ParseError`). After the existing logic (underline span in editor + status bar message), call a new `panelLayout.ensureEditorVisible()` which:
  1. If Editor is folded → unfold it (clear `is-folded`, remove fold styling, apply previous-width flex-basis).
  2. DOES NOT touch `user_override` — when the next successful parse fires, the editor's fold state is re-applied per the user's preference.
- **Why "ensure" not "force"**: matches spec FR-011 ("auto-unfolds when a parse error is shown") without permanently overriding the user's preference. The transient unfold is for error visibility only.

## R-008 — Reset layout: header button + simple confirmation

- **Decision**: a small button in `<header>`, placed before the Sample dropdown. Label: `⟲ Reset` (icon + short text). On click: clear localStorage key, remove all `is-folded` / `is-user-overridden` / inline `flex-basis` styles from `.panel` elements. Animate (the existing 120ms ease-out transition handles it).
- **No confirmation dialog**: the action is reversible (the user can manually re-fold/resize), and a confirmation modal would be heavyweight for this UI. Match the spec's "single click" expectation.
- **Why header over context-menu-on-divider**: discoverable, single-location. Context menu would require right-click which is non-obvious for many users.
